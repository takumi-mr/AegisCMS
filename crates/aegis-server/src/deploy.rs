//! `aegis-server deploy generate`: `deploy/sites.toml`（サイトの一覧）から、Caddy で HTTPS 化した
//! 複数サイトの構成（`compose.yaml`・`Caddyfile`・サイトごとの `sites/<name>/aegis.toml`）を作る。
//!
//! 1 サイト = 1 コンテナ（DB・アップロード・管理者・プラグインがサイトごとに分かれる）で、
//! Caddy がドメインで振り分ける。サイトどうしは別のネットワークに置き、Caddy だけが全サイトに届く。
//! 手順は `deploy/README.md`

use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use serde::Deserialize;

use crate::config::AegisConfig;

/// サイトの設定の土台（コンテナ用の設定）
const BASE_CONFIG: &str = include_str!("../../../config/docker.toml");
const DEFAULT_CADDY_IMAGE: &str = "caddy:2.10-alpine";
const LETS_ENCRYPT_STAGING: &str = "https://acme-staging-v02.api.letsencrypt.org/directory";
const GENERATED_NOTE: &str =
    "deploy/sites.toml から aegis-server deploy generate（./deploy.sh up）で生成。直接編集しない";

/// `sites.toml`
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SitesFile {
    #[serde(default)]
    pub deploy: DeployOptions,
    #[serde(default)]
    pub sites: Vec<SiteSpec>,
}

/// `[deploy]`: 全サイト共通
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeployOptions {
    /// Let's Encrypt に登録する連絡先（証明書の期限切れなどの通知が届く）。空なら登録しない
    #[serde(default)]
    pub acme_email: String,
    /// Let's Encrypt のステージング（試し用。発行回数の制限を受けないが、ブラウザーは信頼しない）
    #[serde(default)]
    pub acme_staging: bool,
    /// Docker Compose のプロジェクト名（コンテナ・ボリュームの名前の接頭辞）
    #[serde(default = "default_project")]
    pub project: String,
    #[serde(default = "default_caddy_image")]
    pub caddy_image: String,
}

impl Default for DeployOptions {
    fn default() -> Self {
        Self {
            acme_email: String::new(),
            acme_staging: false,
            project: default_project(),
            caddy_image: default_caddy_image(),
        }
    }
}

fn default_project() -> String {
    "aegiscms".to_string()
}

fn default_caddy_image() -> String {
    DEFAULT_CADDY_IMAGE.to_string()
}

/// `[[sites]]`: 1 サイト
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteSpec {
    /// サービス名・データのボリューム名（英小文字で始まり、英小文字・数字・`-`）
    pub name: String,
    /// 公開するドメイン。先頭が正規のドメインで、それ以外は先頭へリダイレクトする
    pub domains: Vec<String>,
    #[serde(default)]
    pub site_title: Option<String>,
    #[serde(default)]
    pub site_description: Option<String>,
    /// `[theme].active`
    #[serde(default)]
    pub theme: Option<String>,
    /// `[plugins].enabled`
    #[serde(default)]
    pub plugins: Vec<String>,
    /// コンテナのメモリの上限（Docker の `mem_limit`。例: `"512m"`）
    #[serde(default)]
    pub memory: Option<String>,
    /// `true` でヘルスチェック（`/health`・`/api/v1/health`）をインターネットに公開する
    /// （外部の死活監視で使うとき）。既定では `/metrics` と同じく Caddy が 404 を返す
    #[serde(default)]
    pub public_health: bool,
    /// このサイトの `aegis.toml` に重ねる設定（`[sites.config.commerce]` など）
    #[serde(default)]
    pub config: Option<toml::Table>,
}

/// 生成したファイル（`dir` からの相対パスと中身）
pub struct Generated {
    pub files: Vec<(PathBuf, String)>,
    pub warnings: Vec<String>,
}

/// サービス名として使えるか（英小文字で始まり、英小文字・数字・`-`、40 文字以内。`caddy` は不可）
fn is_valid_site_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 40
        && bytes[0].is_ascii_lowercase()
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
        && name != "caddy"
}

/// 公開するドメインとして使えるか（英小文字・数字・`-` のラベルを `.` でつないだもの、または
/// `localhost`。ワイルドカード・IP アドレス・ポートは不可）
fn is_valid_domain(domain: &str) -> bool {
    domain == "localhost"
        || (aegis_core::is_valid_frame_domain(domain)
            && !domain
                .split('.')
                .all(|l| l.bytes().all(|b| b.is_ascii_digit())))
}

/// ローカルで試すためのドメイン（Caddy は自分の認証局で証明書を作る）
fn is_local_domain(domain: &str) -> bool {
    domain == "localhost" || domain.ends_with(".localhost")
}

fn is_valid_memory(memory: &str) -> bool {
    let digits = memory.trim_end_matches(['b', 'k', 'm', 'g']);
    !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && memory.len() - digits.len() <= 1
}

/// 名前のディレクトリ（`dir/<kind>/<name>/`）の一覧（無ければ空）
fn subdirectories(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    names
}

/// `overlay` を `base` に重ねる（表は再帰的に、それ以外は置き換え）
fn merge(base: &mut toml::Table, overlay: &toml::Table) {
    for (key, value) in overlay {
        match (base.get_mut(key), value) {
            (Some(toml::Value::Table(b)), toml::Value::Table(o)) => merge(b, o),
            _ => {
                base.insert(key.clone(), value.clone());
            }
        }
    }
}

fn set(table: &mut toml::Table, section: &str, key: &str, value: toml::Value) {
    let entry = table
        .entry(section.to_string())
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if let toml::Value::Table(t) = entry {
        t.insert(key.to_string(), value);
    }
}

/// 設定の中身のハッシュ（FNV-1a 64 bit の 16 進数）。compose.yaml のラベルに入れ、設定が変わった
/// サイトのコンテナだけを `docker compose up` が作り直すようにする
fn fnv1a(text: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}

/// YAML の二重引用符の文字列（値は検証済みの名前・ドメイン・パスだけ）
fn yaml_str(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// `dir/sites.toml` を読み、生成するファイルを返す（書き込みはしない）。
/// `builtin` は AegisCMS のイメージの中の組み込みのテーマ・プラグインの場所（`/app`）
pub fn generate(dir: &Path, builtin: &Path) -> anyhow::Result<Generated> {
    let path = dir.join("sites.toml");
    let text = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "{} を読めません（sites.example.toml をコピーして作る）",
            path.display()
        )
    })?;
    let sites: SitesFile = toml::from_str(&text)
        .with_context(|| format!("{} の形式が正しくありません", path.display()))?;
    generate_from(&sites, dir, builtin)
}

/// [`generate`] の本体（テスト用に読み込みと分けている）
pub fn generate_from(sites: &SitesFile, dir: &Path, builtin: &Path) -> anyhow::Result<Generated> {
    let deploy = &sites.deploy;
    if sites.sites.is_empty() {
        bail!("[[sites]] がありません。少なくとも 1 つのサイトを書いてください");
    }
    if !is_valid_site_name(&deploy.project) {
        bail!(
            "[deploy].project {:?} は英小文字で始まり、英小文字・数字・- だけにしてください",
            deploy.project
        );
    }

    let custom_themes = subdirectories(&dir.join("themes"));
    let custom_plugins = subdirectories(&dir.join("plugins"));
    let builtin_themes = subdirectories(&builtin.join("themes"));
    let builtin_plugins = subdirectories(&builtin.join("plugins"));
    let can_check_builtin = builtin.join("themes").is_dir();
    let mut warnings = Vec::new();
    if !can_check_builtin {
        warnings.push(format!(
            "{} が無いため、組み込みのテーマ・プラグインの名前は確かめていません（AegisCMS のイメージの中で実行すると確かめます）",
            builtin.join("themes").display()
        ));
    }

    let mut names = HashSet::new();
    let mut domains = HashSet::new();
    let mut files = Vec::new();
    let mut base_paths = Vec::new();
    for site in &sites.sites {
        if !is_valid_site_name(&site.name) {
            bail!(
                "サイト名 {:?} は英小文字で始まり、英小文字・数字・- だけ（40 文字以内、caddy 以外）にしてください",
                site.name
            );
        }
        if !names.insert(site.name.as_str()) {
            bail!("サイト名 {:?} が 2 回あります", site.name);
        }
        if site.domains.is_empty() {
            bail!("サイト {:?} に domains がありません", site.name);
        }
        for domain in &site.domains {
            if !is_valid_domain(domain) {
                bail!(
                    "サイト {:?} のドメイン {:?} は使えません（小文字のホスト名だけ。https://・ポート・パス・ワイルドカード・IP アドレスは不可）",
                    site.name,
                    domain
                );
            }
            if !domains.insert(domain.as_str()) {
                bail!("ドメイン {:?} が複数のサイトにあります", domain);
            }
        }
        if let Some(memory) = &site.memory {
            if !is_valid_memory(memory) {
                bail!(
                    "サイト {:?} の memory {:?} は 512m・1g のように書いてください",
                    site.name,
                    memory
                );
            }
        }
        if can_check_builtin {
            if let Some(theme) = &site.theme {
                if theme != "default"
                    && !custom_themes.contains(theme)
                    && !builtin_themes.contains(theme)
                {
                    bail!(
                        "サイト {:?} のテーマ {:?} がありません（組み込み: {}。自作のテーマは deploy/themes/{}/ に置く）",
                        site.name,
                        theme,
                        builtin_themes.join(", "),
                        theme
                    );
                }
            }
            for plugin in &site.plugins {
                if !custom_plugins.contains(plugin) && !builtin_plugins.contains(plugin) {
                    bail!(
                        "サイト {:?} のプラグイン {:?} がありません（組み込み: {}。自作のプラグインは deploy/plugins/{}/ に置く）",
                        site.name,
                        plugin,
                        builtin_plugins.join(", "),
                        plugin
                    );
                }
            }
        }

        let (config, base_path) = site_config(site)?;
        base_paths.push(base_path);
        files.push((
            PathBuf::from(format!("sites/{}/aegis.toml", site.name)),
            config,
        ));
    }

    let config_hashes: Vec<String> = files.iter().map(|(_, c)| fnv1a(c)).collect();
    files.push((PathBuf::from("Caddyfile"), caddyfile(sites, &base_paths)));
    files.push((
        PathBuf::from("compose.yaml"),
        compose(sites, dir, &custom_themes, &custom_plugins, &config_hashes),
    ));
    Ok(Generated { files, warnings })
}

/// サイトの `aegis.toml`: コンテナ用の設定に、サイトの値と `config` を重ねる。
/// 中身と、正規化した `[server].base_path` を返す
fn site_config(site: &SiteSpec) -> anyhow::Result<(String, String)> {
    let mut table: toml::Table = toml::from_str(BASE_CONFIG).context("config/docker.toml")?;
    // Caddy の後ろで動くので、X-Forwarded-For の先頭を閲覧者の IP とみなす
    set(
        &mut table,
        "audit",
        "trust_forwarded_for",
        toml::Value::Boolean(true),
    );
    if let Some(title) = &site.site_title {
        set(
            &mut table,
            "theme",
            "site_title",
            toml::Value::String(title.clone()),
        );
    }
    if let Some(description) = &site.site_description {
        set(
            &mut table,
            "theme",
            "site_description",
            toml::Value::String(description.clone()),
        );
    }
    if let Some(theme) = &site.theme {
        set(
            &mut table,
            "theme",
            "active",
            toml::Value::String(theme.clone()),
        );
    }
    set(
        &mut table,
        "plugins",
        "enabled",
        toml::Value::Array(
            site.plugins
                .iter()
                .cloned()
                .map(toml::Value::String)
                .collect(),
        ),
    );
    if let Some(overlay) = &site.config {
        merge(&mut table, overlay);
    }

    // 起動時と同じ検証を、生成の時点で済ませる
    let parsed: AegisConfig = toml::Value::Table(table.clone())
        .try_into()
        .with_context(|| format!("サイト {:?} の設定（config）が正しくありません", site.name))?;
    let check = |r: aegis_core::Result<()>| {
        r.map_err(|e| anyhow::anyhow!("サイト {:?} の設定: {}", site.name, e))
    };
    check(parsed.theme.validate())?;
    check(parsed.plugins.validate())?;
    check(parsed.commerce.validate())?;
    check(parsed.comments.validate())?;
    check(parsed.forum.validate())?;
    check(parsed.notifications.validate())?;
    check(parsed.cdn.validate())?;
    if parsed.contact.enabled {
        check(parsed.contact.validate())?;
    }
    #[cfg(feature = "analytics")]
    parsed
        .analytics
        .validate()
        .map_err(|e| anyhow::anyhow!("サイト {:?} の設定: {}", site.name, e))?;
    let base_path = parsed
        .server
        .normalized_base_path()
        .map_err(|e| anyhow::anyhow!("サイト {:?} の [server].base_path: {}", site.name, e))?;

    let body = toml::to_string_pretty(&table).context("サイトの設定を書き出せません")?;
    let config = format!(
        "# サイト {:?} の設定（{}）\n# 項目の説明は docs/configuration.md\n\n{}",
        site.name, GENERATED_NOTE, body
    );
    Ok((config, base_path))
}

/// インターネットから見せないパス（Prometheus などは Docker のネットワークの中から
/// `http://<name>:8080/metrics` を直接読む）。サーバーはベースパスの有無のどちらでも受け付けるので両方
fn internal_paths(site: &SiteSpec, base_path: &str) -> Vec<String> {
    let mut paths = vec!["/metrics"];
    if !site.public_health {
        paths.extend(["/health", "/api/v1/health"]);
    }
    let mut out: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
    if !base_path.is_empty() {
        out.extend(paths.iter().map(|p| format!("{}{}", base_path, p)));
    }
    out
}

/// `base_paths` はサイトの順の `[server].base_path`
fn caddyfile(sites: &SitesFile, base_paths: &[String]) -> String {
    let deploy = &sites.deploy;
    let mut out = format!("# {}\n", GENERATED_NOTE);
    if !deploy.acme_email.trim().is_empty() || deploy.acme_staging {
        out.push_str("\n{\n");
        if !deploy.acme_email.trim().is_empty() {
            let _ = writeln!(out, "\temail {}", deploy.acme_email.trim());
        }
        if deploy.acme_staging {
            let _ = writeln!(out, "\tacme_ca {}", LETS_ENCRYPT_STAGING);
        }
        out.push_str("}\n");
    }
    for (site, base_path) in sites.sites.iter().zip(base_paths) {
        let primary = &site.domains[0];
        let _ = write!(
            out,
            "\n# {}\n{} {{\n\tencode zstd gzip\n",
            site.name, primary
        );
        if !is_local_domain(primary) {
            // サブドメインに別のテナントがいることがあるので includeSubDomains は付けない
            out.push_str("\theader Strict-Transport-Security \"max-age=31536000\"\n");
        }
        // 存在を知らせないよう 403 ではなく 404
        let _ = writeln!(
            out,
            "\t@internal path {}\n\trespond @internal 404",
            internal_paths(site, base_path).join(" ")
        );
        let _ = writeln!(out, "\treverse_proxy {}:8080\n}}", site.name);
        for alias in &site.domains[1..] {
            let _ = writeln!(
                out,
                "\n{} {{\n\tredir https://{}{{uri}} permanent\n}}",
                alias, primary
            );
        }
    }
    out
}

/// `config_hashes` はサイトの順の `aegis.toml` のハッシュ
fn compose(
    sites: &SitesFile,
    dir: &Path,
    custom_themes: &[String],
    custom_plugins: &[String],
    config_hashes: &[String],
) -> String {
    let deploy = &sites.deploy;
    let mut out = format!("# {}\n", GENERATED_NOTE);
    let _ = writeln!(out, "name: {}\n\nservices:", yaml_str(&deploy.project));

    // Caddy: 80 / 443 を受け、ドメインでサイトに振り分ける。全サイトのネットワークに入る
    let _ = writeln!(out, "  caddy:");
    let _ = writeln!(out, "    image: {}", yaml_str(&deploy.caddy_image));
    out.push_str("    restart: unless-stopped\n    ports:\n      - \"80:80\"\n      - \"443:443\"\n      - \"443:443/udp\"\n");
    out.push_str("    volumes:\n      - ./Caddyfile:/etc/caddy/Caddyfile:ro\n      - caddy-data:/data\n      - caddy-config:/config\n");
    out.push_str("    networks:\n");
    for site in &sites.sites {
        let _ = writeln!(out, "      - site-{}", site.name);
    }
    out.push_str("    depends_on:\n");
    for site in &sites.sites {
        let _ = writeln!(out, "      - {}", site.name);
    }

    for (site, hash) in sites.sites.iter().zip(config_hashes) {
        let _ = writeln!(out, "\n  {}:", site.name);
        out.push_str("    image: ${AEGIS_IMAGE:-aegiscms:latest}\n    restart: unless-stopped\n");
        // 設定（bind mount）が変わったら作り直させる
        let _ = writeln!(
            out,
            "    labels:\n      aegiscms.config-hash: {}",
            yaml_str(hash)
        );
        out.push_str("    environment:\n      AEGIS_CONFIG: /app/config/site.toml\n");
        let secrets = format!("secrets/{}.env", site.name);
        if dir.join(&secrets).is_file() {
            let _ = writeln!(
                out,
                "    env_file:\n      - {}",
                yaml_str(&format!("./{}", secrets))
            );
        }
        out.push_str("    volumes:\n");
        let _ = writeln!(
            out,
            "      - ./sites/{}/aegis.toml:/app/config/site.toml:ro",
            site.name
        );
        let _ = writeln!(out, "      - {}-data:/app/data", site.name);
        for theme in custom_themes {
            let _ = writeln!(
                out,
                "      - {}",
                yaml_str(&format!("./themes/{0}:/app/themes/{0}:ro", theme))
            );
        }
        for plugin in custom_plugins {
            let _ = writeln!(
                out,
                "      - {}",
                yaml_str(&format!("./plugins/{0}:/app/plugins/{0}:ro", plugin))
            );
        }
        if let Some(memory) = &site.memory {
            let _ = writeln!(out, "    mem_limit: {}", memory);
        }
        out.push_str("    expose:\n      - \"8080\"\n");
        let _ = writeln!(out, "    networks:\n      - site-{}", site.name);
    }

    out.push_str("\nnetworks:\n");
    for site in &sites.sites {
        let _ = writeln!(out, "  site-{}: {{}}", site.name);
    }
    out.push_str("\nvolumes:\n  caddy-data: {}\n  caddy-config: {}\n");
    for site in &sites.sites {
        let _ = writeln!(out, "  {}-data: {{}}", site.name);
    }
    out
}

/// 生成して `dir` に書き込む。作ったファイルを表示する
pub fn run_generate(dir: &Path, builtin: &Path) -> anyhow::Result<()> {
    let generated = generate(dir, builtin)?;
    for warning in &generated.warnings {
        eprintln!("warning: {}", warning);
    }
    for (relative, content) in &generated.files {
        let path = dir.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("{} を作れません", parent.display()))?;
        }
        std::fs::write(&path, content)
            .with_context(|| format!("{} に書けません", path.display()))?;
        println!("generated {}", relative.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> SitesFile {
        toml::from_str(text).unwrap()
    }

    /// 組み込みのテーマ・プラグインがあるディレクトリ（リポジトリのルート）
    fn repo() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// テスト用の空のディレクトリ（終わったら消す）
    struct TempDir(PathBuf);

    impl TempDir {
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn empty_dir() -> TempDir {
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("aegis-deploy-test-{}-{}", std::process::id(), n));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn file<'a>(generated: &'a Generated, name: &str) -> &'a str {
        &generated
            .files
            .iter()
            .find(|(p, _)| p == Path::new(name))
            .unwrap_or_else(|| panic!("{} was not generated", name))
            .1
    }

    const TWO_SITES: &str = r#"
        [deploy]
        acme_email = "ops@example.com"

        [[sites]]
        name = "main"
        domains = ["example.com", "www.example.com"]
        site_title = "Example"
        theme = "roastery"
        plugins = ["official-blocks"]
        memory = "512m"

        [sites.config.commerce]
        enabled = true

        [[sites]]
        name = "blog"
        domains = ["blog.example.com"]
    "#;

    #[test]
    fn test_generates_caddy_compose_and_site_configs() {
        let dir = empty_dir();
        std::fs::create_dir_all(dir.path().join("themes/my-theme")).unwrap();
        std::fs::create_dir_all(dir.path().join("secrets")).unwrap();
        std::fs::write(
            dir.path().join("secrets/main.env"),
            "AEGIS_SMTP_PASSWORD=x\n",
        )
        .unwrap();
        let generated = generate_from(&parse(TWO_SITES), dir.path(), &repo()).unwrap();
        assert!(generated.warnings.is_empty(), "{:?}", generated.warnings);

        let caddy = file(&generated, "Caddyfile");
        assert!(
            caddy.contains("\n{\n\temail ops@example.com\n}\n"),
            "{}",
            caddy
        );
        assert!(caddy.contains("\nexample.com {\n\tencode zstd gzip\n\theader Strict-Transport-Security \"max-age=31536000\"\n\t@internal path /metrics /health /api/v1/health\n\trespond @internal 404\n\treverse_proxy main:8080\n}\n"), "{}", caddy);
        assert!(
            caddy.contains("\nwww.example.com {\n\tredir https://example.com{uri} permanent\n}\n")
        );
        assert!(
            caddy.contains("\nblog.example.com {\n") && caddy.contains("reverse_proxy blog:8080")
        );
        assert!(!caddy.contains("acme_ca"));

        let compose = file(&generated, "compose.yaml");
        assert!(compose.contains("name: \"aegiscms\""));
        assert!(
            compose.contains("      - site-main\n      - site-blog\n"),
            "{}",
            compose
        );
        assert!(compose.contains("  main:\n    image: ${AEGIS_IMAGE:-aegiscms:latest}"));
        assert!(compose.contains("    env_file:\n      - \"./secrets/main.env\""));
        assert_eq!(compose.matches("env_file").count(), 1);
        assert!(compose.contains("      - \"./themes/my-theme:/app/themes/my-theme:ro\""));
        assert!(compose.contains("    mem_limit: 512m"));
        assert!(compose.contains(&format!(
            "      aegiscms.config-hash: \"{}\"",
            fnv1a(file(&generated, "sites/main/aegis.toml"))
        )));
        assert!(compose.contains("  main-data: {}") && compose.contains("  blog-data: {}"));
        // サイトの外にポートを開けない
        assert_eq!(compose.matches("ports:").count(), 1);

        let main: AegisConfig = toml::from_str(file(&generated, "sites/main/aegis.toml")).unwrap();
        assert_eq!(main.theme.active, "roastery");
        assert_eq!(main.theme.site_title, "Example");
        assert_eq!(main.plugins.enabled, vec!["official-blocks".to_string()]);
        assert!(main.commerce.enabled);
        assert!(main.audit.trust_forwarded_for);
        assert_eq!(main.database.url, "sqlite:///app/data/aegis.db?mode=rwc");
        let blog: AegisConfig = toml::from_str(file(&generated, "sites/blog/aegis.toml")).unwrap();
        assert!(!blog.commerce.enabled && blog.plugins.enabled.is_empty());
    }

    #[test]
    fn test_internal_paths_are_hidden_from_the_internet() {
        let sites = parse(
            r#"
            [[sites]]
            name = "main"
            domains = ["example.com"]

            [[sites]]
            name = "status"
            domains = ["status.example.com"]
            public_health = true

            [sites.config.server]
            base_path = "/cms"
            "#,
        );
        let generated = generate_from(&sites, empty_dir().path(), &repo()).unwrap();
        let caddy = file(&generated, "Caddyfile");
        assert!(
            caddy.contains("\n\t@internal path /metrics /health /api/v1/health\n\trespond @internal 404\n\treverse_proxy main:8080\n"),
            "{}",
            caddy
        );
        assert!(
            caddy.contains("\n\t@internal path /metrics /cms/metrics\n\trespond @internal 404\n\treverse_proxy status:8080\n"),
            "{}",
            caddy
        );
    }

    #[test]
    fn test_staging_and_local_domains() {
        let generated = generate_from(
            &parse(
                r#"
                [deploy]
                acme_staging = true
                [[sites]]
                name = "dev"
                domains = ["dev.localhost"]
                "#,
            ),
            empty_dir().path(),
            &repo(),
        )
        .unwrap();
        let caddy = file(&generated, "Caddyfile");
        assert!(caddy.contains(&format!("\tacme_ca {}", LETS_ENCRYPT_STAGING)));
        // ローカルのドメインには HSTS を付けない
        assert!(!caddy.contains("Strict-Transport-Security"), "{}", caddy);
    }

    #[test]
    fn test_rejects_invalid_sites() {
        let dir = empty_dir();
        let site = |body: &str| format!("[[sites]]\n{}", body);
        for (text, expected) in [
            (String::new(), "[[sites]]"),
            (site("name = \"Main\"\ndomains = [\"a.com\"]"), "サイト名"),
            (site("name = \"caddy\"\ndomains = [\"a.com\"]"), "サイト名"),
            (site("name = \"a\"\ndomains = []"), "domains"),
            (site("name = \"a\"\ndomains = [\"https://a.com\"]"), "ドメイン"),
            (site("name = \"a\"\ndomains = [\"*.a.com\"]"), "ドメイン"),
            (site("name = \"a\"\ndomains = [\"10.0.0.1\"]"), "ドメイン"),
            (site("name = \"a\"\ndomains = [\"A.com\"]"), "ドメイン"),
            (format!("{}\n{}", site("name = \"a\"\ndomains = [\"a.com\"]"), site("name = \"a\"\ndomains = [\"b.com\"]")), "2 回"),
            (format!("{}\n{}", site("name = \"a\"\ndomains = [\"a.com\"]"), site("name = \"b\"\ndomains = [\"a.com\"]")), "複数のサイト"),
            (site("name = \"a\"\ndomains = [\"a.com\"]\ntheme = \"nope\""), "テーマ"),
            (site("name = \"a\"\ndomains = [\"a.com\"]\nplugins = [\"nope\"]"), "プラグイン"),
            (site("name = \"a\"\ndomains = [\"a.com\"]\nmemory = \"lots\""), "memory"),
            (site("name = \"a\"\ndomains = [\"a.com\"]\n[sites.config.theme]\nfront_page = \"bogus\""), "設定"),
            (site("name = \"a\"\ndomains = [\"a.com\"]\n[sites.config.server]\nport = \"x\""), "config"),
        ] {
            let err = match toml::from_str::<SitesFile>(&text) {
                Ok(sites) => generate_from(&sites, dir.path(), &repo())
                    .err()
                    .map(|e| format!("{:#}", e))
                    .unwrap_or_default(),
                Err(e) => e.to_string(),
            };
            assert!(err.contains(expected), "{:?} -> {:?}", text, err);
        }
        // 知らないキーは書き間違いとして断る
        assert!(
            toml::from_str::<SitesFile>("[[sites]]\nname = \"a\"\ndomain = [\"a.com\"]").is_err()
        );
    }

    #[test]
    fn test_without_the_builtin_directory_names_are_not_checked() {
        let generated = generate_from(
            &parse("[[sites]]\nname = \"a\"\ndomains = [\"a.com\"]\ntheme = \"custom\""),
            empty_dir().path(),
            Path::new("/nonexistent"),
        )
        .unwrap();
        assert_eq!(generated.warnings.len(), 1);
    }

    #[test]
    fn test_the_example_file_generates() {
        let example = std::fs::read_to_string(repo().join("deploy/sites.example.toml")).unwrap();
        let generated = generate_from(&parse(&example), empty_dir().path(), &repo()).unwrap();
        assert!(generated.warnings.is_empty(), "{:?}", generated.warnings);
    }

    #[test]
    fn test_config_hash_changes_only_for_the_changed_site() {
        let hashes = |text: &str| {
            let generated = generate_from(&parse(text), empty_dir().path(), &repo()).unwrap();
            let compose = file(&generated, "compose.yaml").to_string();
            compose
                .lines()
                .filter(|l| l.contains("aegiscms.config-hash"))
                .map(str::to_string)
                .collect::<Vec<_>>()
        };
        let before = hashes("[[sites]]\nname = \"a\"\ndomains = [\"a.com\"]\n[[sites]]\nname = \"b\"\ndomains = [\"b.com\"]");
        let after = hashes("[[sites]]\nname = \"a\"\ndomains = [\"a.com\"]\nsite_title = \"A\"\n[[sites]]\nname = \"b\"\ndomains = [\"b.com\"]");
        assert_ne!(before[0], after[0]);
        assert_eq!(before[1], after[1]);
    }
}
