//! `aegis-server theme import-wordpress <テーマのフォルダー>` — WordPress のテーマから、AegisCMS のテーマを作る。
//!
//! PHP のテンプレートは変換できない。代わりに、WordPress と同じクラス名で出力するスターターテーマ
//! （`themes/wordpress-classic`）を土台にして、元のテーマの `style.css` と画像・フォントを
//! `assets/wp/` に写し、スターターのあとに読み込む。クラス名が合っていれば、見た目はほぼそのまま出る。
//! 手で直す箇所（ヘッダー・フッターのマークアップ、ウィジェット、メニューなど）は一覧で報告する。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use aegis_theme::{ThemeAuthor, ThemeCsp, ThemeManifest};
use clap::Args;
use walkdir::WalkDir;

/// スターターテーマの ID
const STARTER_ID: &str = "wordpress-classic";
/// スターターの `base.html` の、元のテーマの CSS を読み込む位置の目印
const STYLE_MARKER: &str = "<!-- wordpress-theme-style -->";
/// 元のテーマのファイルを写す先（`assets/` の下）
const COPY_DIR: &str = "wp";
/// 1 ファイルの大きさの上限（これを超えるものは写さない）
const MAX_FILE_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Args, Debug)]
pub struct ImportThemeArgs {
    /// WordPress のテーマのフォルダー（`style.css` がある場所。`wp-content/themes/<名前>`）
    pub source: PathBuf,

    /// 作るテーマの ID（小文字・数字・ハイフン）。省略すると、元のテーマの名前から付ける
    #[arg(long)]
    pub id: Option<String>,

    /// テーマを置くフォルダー（`[paths].themes_dir`）
    #[arg(long, default_value = "themes")]
    pub themes_dir: PathBuf,

    /// 土台にするスターターテーマのフォルダー（既定は `<themes-dir>/wordpress-classic`）
    #[arg(long)]
    pub starter: Option<PathBuf>,

    /// 同じ ID のテーマがあっても上書きする
    #[arg(long)]
    pub force: bool,
}

/// `style.css` の先頭のコメントのヘッダー（`Theme Name: …`）。キーは小文字
pub fn parse_header(css: &str) -> BTreeMap<String, String> {
    let mut header = BTreeMap::new();
    let Some(start) = css.find("/*") else {
        return header;
    };
    let body = &css[start + 2..];
    let body = body.split("*/").next().unwrap_or(body);
    for line in body.lines() {
        let line = line.trim().trim_start_matches('*').trim();
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim();
            // `Theme URI: https://…` のように値に `:` を含んでもよいが、キーは英字と空白だけ
            if !key.is_empty()
                && key.chars().all(|c| c.is_ascii_alphabetic() || c == ' ')
                && !value.is_empty()
                && !header.contains_key(&key)
            {
                header.insert(key, value.to_string());
            }
        }
    }
    header
}

/// テーマの ID: 指定があればそれ、なければテキストドメイン・フォルダー名・テーマ名の順。
/// 小文字・数字・ハイフンだけにする
pub fn theme_id(
    given: Option<&str>,
    header: &BTreeMap<String, String>,
    source: &Path,
) -> Result<String, String> {
    let sanitize = |raw: &str| -> String {
        let mut out = String::new();
        for c in raw.chars() {
            if c.is_ascii_alphanumeric() {
                out.push(c.to_ascii_lowercase());
            } else if !out.ends_with('-') {
                out.push('-');
            }
        }
        out.trim_matches('-').to_string()
    };
    if let Some(given) = given {
        let id = sanitize(given);
        if id.is_empty() || id != given {
            return Err(format!(
                "--id must be lowercase letters, digits and hyphens (for example '{}')",
                if id.is_empty() { "my-theme" } else { &id }
            ));
        }
        return Ok(id);
    }
    let from_folder = source
        .file_name()
        .and_then(|n| n.to_str())
        .map(sanitize)
        .unwrap_or_default();
    let candidates = [
        header.get("text domain").map(|s| sanitize(s)),
        Some(from_folder),
        header.get("theme name").map(|s| sanitize(s)),
    ];
    let id = candidates
        .into_iter()
        .flatten()
        .find(|id| !id.is_empty())
        .unwrap_or_else(|| "wordpress-theme".to_string());
    // スターターと同じ ID にはしない（コピーが自分自身を上書きしてしまう）
    Ok(if id == STARTER_ID {
        format!("{id}-imported")
    } else {
        id
    })
}

/// CSS が読み込む Google Fonts のホスト（`theme.json` の `csp` に宣言する）。
/// 戻り値は（スタイルのホスト、フォントのホスト）
pub fn css_hosts(css: &str) -> (Vec<String>, Vec<String>) {
    let mut styles = Vec::new();
    let mut fonts = Vec::new();
    if css.contains("fonts.googleapis.com") {
        styles.push("fonts.googleapis.com".to_string());
        fonts.push("fonts.gstatic.com".to_string());
    }
    if css.contains("fonts.gstatic.com") && fonts.is_empty() {
        fonts.push("fonts.gstatic.com".to_string());
    }
    (styles, fonts)
}

/// CSS のコメント（`/* … */`）を除く
fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start + 2..].find("*/") {
            Some(end) => rest = &rest[start + 2 + end + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// CSS の中の、ほかのホストの URL（宣言できないので、報告に出す）
fn other_hosts(css: &str) -> Vec<String> {
    let mut hosts = Vec::new();
    let stripped = strip_comments(css);
    let mut rest = stripped.as_str();
    while let Some(i) = rest.find("://") {
        let after = &rest[i + 3..];
        let host: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
            .collect();
        if !host.is_empty()
            && !matches!(host.as_str(), "fonts.googleapis.com" | "fonts.gstatic.com")
            && !hosts.contains(&host)
        {
            hosts.push(host);
        }
        rest = after;
    }
    hosts
}

/// 写さないもの: PHP・翻訳・開発用のフォルダー・隠しファイル
fn skip(relative: &Path) -> bool {
    let name = relative
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if name.starts_with('.') {
        return true;
    }
    let ext = relative
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(
        ext.as_str(),
        "php" | "po" | "mo" | "pot" | "scss" | "sass" | "less" | "map" | "log" | "zip"
    ) {
        return true;
    }
    relative.components().any(|c| {
        matches!(
            c.as_os_str().to_str(),
            Some("node_modules" | "languages" | "vendor" | "tests" | "bin")
        ) || c.as_os_str().to_str().is_some_and(|s| s.starts_with('.'))
    })
}

/// 写した結果
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CopyStats {
    pub copied: usize,
    pub scripts: usize,
    pub too_large: Vec<String>,
}

/// 元のテーマの CSS・画像・フォントなどを、`assets/wp/` に（フォルダーの構造のまま）写す。
/// 相対パスの `url(images/a.png)` がそのまま効くように、構造を保つ
pub fn copy_assets(source: &Path, target: &Path) -> std::io::Result<CopyStats> {
    let mut stats = CopyStats::default();
    for entry in WalkDir::new(source).follow_links(false) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(source) else {
            continue;
        };
        if skip(relative) {
            continue;
        }
        if entry.metadata()?.len() > MAX_FILE_BYTES {
            stats
                .too_large
                .push(relative.to_string_lossy().replace('\\', "/"));
            continue;
        }
        let destination = target.join(relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(entry.path(), &destination)?;
        stats.copied += 1;
        if relative.extension().is_some_and(|e| e == "js") {
            stats.scripts += 1;
        }
    }
    Ok(stats)
}

/// WordPress のテンプレート → このテーマのテンプレート（`None` は、対応するものが無い）
const TEMPLATE_MAP: &[(&str, Option<&str>, &str)] = &[
    ("index.php", Some("index.html"), "post list"),
    ("home.php", Some("index.html"), "post list"),
    (
        "front-page.php",
        Some("front.html"),
        "top page (set [theme].front_page = \"template\" to use front.html)",
    ),
    ("single.php", Some("article.html"), "single post"),
    ("single-post.php", Some("article.html"), "single post"),
    ("page.php", Some("page.html"), "static page"),
    (
        "archive.php",
        Some("category.html"),
        "archive (category.html and tag.html)",
    ),
    ("category.php", Some("category.html"), "category archive"),
    ("tag.php", Some("tag.html"), "tag archive"),
    ("search.php", Some("search.html"), "search results"),
    ("404.php", Some("404.html"), "not found"),
    ("comments.php", Some("comments.html"), "comment section"),
    (
        "header.php",
        Some("base.html"),
        "header, merged into base.html",
    ),
    (
        "footer.php",
        Some("base.html"),
        "footer, merged into base.html",
    ),
    (
        "sidebar.php",
        None,
        "widgets are not supported: put fixed content in base.html",
    ),
    ("searchform.php", None, "the search form is built in"),
    (
        "functions.php",
        None,
        "menus, widgets and Customizer settings are not imported",
    ),
];

/// 元のテーマにある PHP のテンプレートと、対応の説明
pub fn template_report(source: &Path) -> Vec<String> {
    let mut lines = Vec::new();
    let mut found: Vec<String> = WalkDir::new(source)
        .max_depth(2)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().is_some_and(|x| x == "php"))
        .filter_map(|e| {
            e.path()
                .strip_prefix(source)
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
        })
        .collect();
    found.sort();
    for file in found {
        let line = match TEMPLATE_MAP.iter().find(|(name, _, _)| *name == file) {
            Some((_, Some(target), what)) => {
                format!("{file} -> templates/{target} ({what}): check the markup")
            }
            Some((_, None, what)) => format!("{file}: not converted ({what})"),
            None if file.starts_with("template-parts/") => {
                format!("{file}: not converted (copy its markup into the template that used it)")
            }
            None if file.starts_with("page-") || file.starts_with("single-") => format!(
                "{file}: not converted (a template for one page or type; use the layout field of a page, or a theme template)"
            ),
            None => format!("{file}: not converted (PHP cannot run in a theme)"),
        };
        lines.push(line);
    }
    lines
}

/// 作ったテーマの内容（テスト・表示用）
#[derive(Debug)]
pub struct Imported {
    pub id: String,
    pub dir: PathBuf,
    pub stats: CopyStats,
    pub notes: Vec<String>,
}

/// テーマを作る
pub fn import(args: &ImportThemeArgs) -> Result<Imported, String> {
    let style_path = args.source.join("style.css");
    let css = std::fs::read_to_string(&style_path).map_err(|e| {
        format!(
            "{} is not a WordPress theme folder: cannot read style.css ({e})",
            args.source.display()
        )
    })?;
    let header = parse_header(&css);
    if !header.contains_key("theme name") {
        return Err(format!(
            "{} has no 'Theme Name:' header: this does not look like a WordPress theme",
            style_path.display()
        ));
    }
    let id = theme_id(args.id.as_deref(), &header, &args.source)?;
    let starter = args
        .starter
        .clone()
        .unwrap_or_else(|| args.themes_dir.join(STARTER_ID));
    if !starter.join("templates").join("base.html").is_file() {
        return Err(format!(
            "The starter theme was not found at {} (use --starter, or --themes-dir where '{STARTER_ID}' is)",
            starter.display()
        ));
    }
    let target = args.themes_dir.join(&id);
    if target.exists() {
        if !args.force {
            return Err(format!(
                "{} already exists: choose another --id, or use --force to overwrite it",
                target.display()
            ));
        }
        std::fs::remove_dir_all(&target).map_err(|e| e.to_string())?;
    }

    // 1. スターターをそのまま写す
    for entry in WalkDir::new(&starter).follow_links(false) {
        let entry = entry.map_err(|e| e.to_string())?;
        let relative = entry
            .path()
            .strip_prefix(&starter)
            .map_err(|e| e.to_string())?;
        let destination = target.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
        } else if entry.file_type().is_file() {
            std::fs::copy(entry.path(), &destination).map_err(|e| e.to_string())?;
        }
    }

    // 2. 元のテーマのファイルを assets/wp/ に写す
    let assets = target.join("assets").join(COPY_DIR);
    let stats = copy_assets(&args.source, &assets).map_err(|e| e.to_string())?;

    // 3. base.html に、元の style.css を読み込む link を入れる
    let base_path = target.join("templates").join("base.html");
    let base = std::fs::read_to_string(&base_path).map_err(|e| e.to_string())?;
    if !base.contains(STYLE_MARKER) {
        return Err(format!(
            "The starter's base.html has no '{STYLE_MARKER}' marker: use the wordpress-classic theme that comes with this version"
        ));
    }
    let link = format!(
        "<link rel=\"stylesheet\" href=\"{{{{ base_path }}}}/theme/assets/{COPY_DIR}/style.css\">"
    );
    std::fs::write(&base_path, base.replace(STYLE_MARKER, &link)).map_err(|e| e.to_string())?;

    // 4. 元のテーマのスクリーンショットをプレビューに
    let mut manifest: ThemeManifest = serde_json::from_str(
        &std::fs::read_to_string(target.join("theme.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("The starter's theme.json is not valid: {e}"))?;
    for name in [
        "screenshot.png",
        "screenshot.jpg",
        "screenshot.jpeg",
        "screenshot.webp",
    ] {
        let screenshot = args.source.join(name);
        if screenshot.is_file() {
            let extension = name.rsplit('.').next().unwrap_or("png");
            let preview = format!("preview.{extension}");
            if std::fs::copy(&screenshot, target.join(&preview)).is_ok() {
                let _ = std::fs::remove_file(target.join("preview.svg"));
                manifest.preview_image = Some(preview);
            }
            break;
        }
    }

    // 5. theme.json
    manifest.id = id.clone();
    manifest.name = header
        .get("theme name")
        .cloned()
        .unwrap_or_else(|| id.clone());
    if let Some(version) = header.get("version") {
        manifest.version = version.clone();
    }
    if let Some(description) = header.get("description") {
        manifest.description = description.clone();
    }
    manifest.author = match header.get("author") {
        Some(name) => ThemeAuthor::Detailed {
            name: name.clone(),
            email: None,
            url: header.get("author uri").cloned(),
        },
        None => manifest.author,
    };
    if let Some(license) = header.get("license") {
        manifest.license = Some(license.clone());
    }
    manifest.homepage = header.get("theme uri").cloned();
    manifest.repository = None;
    manifest.tags = vec!["wordpress".to_string(), "imported".to_string()];
    let (style_domains, font_domains) = css_hosts(&css);
    manifest.csp = (!style_domains.is_empty() || !font_domains.is_empty()).then(|| ThemeCsp {
        style_domains,
        font_domains,
        img_domains: Vec::new(),
    });
    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(target.join("theme.json"), format!("{json}\n")).map_err(|e| e.to_string())?;

    // 6. 手で直す箇所
    let mut notes = template_report(&args.source);
    let hosts = other_hosts(&css);
    if !hosts.is_empty() {
        notes.push(format!(
            "style.css loads from other hosts ({}): they are blocked by the CSP; download the files into the theme, or declare the hosts in theme.json csp",
            hosts.join(", ")
        ));
    }
    if stats.scripts > 0 {
        notes.push(format!(
            "{} JavaScript file(s) were copied to assets/{COPY_DIR}/ but are not loaded: theme scripts must be added by hand (inline scripts are not allowed)",
            stats.scripts
        ));
    }
    for file in &stats.too_large {
        notes.push(format!("{file}: not copied (over 20 MB)"));
    }
    Ok(Imported {
        id,
        dir: target,
        stats,
        notes,
    })
}

pub fn run(args: ImportThemeArgs) -> anyhow::Result<()> {
    let imported = import(&args).map_err(|e| anyhow::anyhow!(e))?;
    println!(
        "Created theme '{}' at {} ({} files copied to assets/{COPY_DIR}/)",
        imported.id,
        imported.dir.display(),
        imported.stats.copied
    );
    if !imported.notes.is_empty() {
        println!("\nNeeds attention:");
        for note in &imported.notes {
            println!("  - {note}");
        }
    }
    println!(
        "\nNext: check it with `aegis-server theme dev {}`, then set [theme].active = \"{}\".",
        imported.dir.display(),
        imported.id
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const STYLE: &str = "/*\nTheme Name: Twenty Test\nTheme URI: https://example.com/twenty-test\nAuthor: the WordPress team\nAuthor URI: https://wordpress.org/\nDescription: A test: with a colon\nVersion: 2.1\nLicense: GNU General Public License v2 or later\nText Domain: twentytest\n*/\nbody { background: url(images/bg.png); }\n@import url('https://fonts.googleapis.com/css?family=Lato');\n";

    #[test]
    fn the_header_is_read() {
        let header = parse_header(STYLE);
        assert_eq!(header["theme name"], "Twenty Test");
        assert_eq!(header["theme uri"], "https://example.com/twenty-test");
        assert_eq!(header["description"], "A test: with a colon");
        assert_eq!(header["text domain"], "twentytest");
        assert!(parse_header("body { color: red }").is_empty());
    }

    #[test]
    fn ids_are_made_valid() {
        let header = parse_header(STYLE);
        assert_eq!(
            theme_id(None, &header, Path::new("/x/twentytwenty")).unwrap(),
            "twentytest"
        );
        let none = BTreeMap::new();
        assert_eq!(
            theme_id(None, &none, Path::new("/x/My Theme_2")).unwrap(),
            "my-theme-2"
        );
        assert_eq!(
            theme_id(Some("custom"), &header, Path::new("/x")).unwrap(),
            "custom"
        );
        assert!(theme_id(Some("Bad Id"), &header, Path::new("/x")).is_err());
        assert_eq!(
            theme_id(None, &none, Path::new("/x/wordpress-classic")).unwrap(),
            "wordpress-classic-imported"
        );
    }

    #[test]
    fn google_fonts_hosts_are_found() {
        let (styles, fonts) = css_hosts(STYLE);
        assert_eq!(styles, ["fonts.googleapis.com"]);
        assert_eq!(fonts, ["fonts.gstatic.com"]);
        assert!(other_hosts(STYLE).is_empty());
        assert_eq!(
            other_hosts("a { background: url(https://cdn.example.com/a.png) }"),
            ["cdn.example.com"]
        );
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn wp_theme(dir: &Path) {
        write(&dir.join("style.css"), STYLE);
        write(&dir.join("images/bg.png"), "png");
        write(&dir.join("fonts/a.woff2"), "font");
        write(&dir.join("js/nav.js"), "x");
        write(&dir.join("index.php"), "<?php");
        write(&dir.join("single.php"), "<?php");
        write(&dir.join("sidebar.php"), "<?php");
        write(&dir.join("functions.php"), "<?php");
        write(&dir.join("template-parts/content.php"), "<?php");
        write(&dir.join("languages/ja.po"), "x");
        write(&dir.join("node_modules/x/a.css"), "x");
        write(&dir.join(".git/config"), "x");
        write(&dir.join("screenshot.png"), "shot");
    }

    fn starter(themes: &Path) {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../themes/wordpress-classic");
        for entry in WalkDir::new(&repo).into_iter().flatten() {
            if entry.file_type().is_file() {
                let relative = entry.path().strip_prefix(&repo).unwrap();
                let to = themes.join(STARTER_ID).join(relative);
                std::fs::create_dir_all(to.parent().unwrap()).unwrap();
                std::fs::copy(entry.path(), to).unwrap();
            }
        }
    }

    #[test]
    fn a_wordpress_theme_becomes_a_theme() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("twentytest");
        wp_theme(&source);
        let themes = temp.path().join("themes");
        starter(&themes);

        let imported = import(&ImportThemeArgs {
            source,
            id: None,
            themes_dir: themes.clone(),
            starter: None,
            force: false,
        })
        .unwrap();
        assert_eq!(imported.id, "twentytest");
        let dir = &imported.dir;

        // The starter's templates and the copied assets
        assert!(dir.join("templates/article.html").is_file());
        assert!(dir.join("assets/wp/images/bg.png").is_file());
        assert!(dir.join("assets/wp/fonts/a.woff2").is_file());
        assert!(dir.join("assets/wp/style.css").is_file());
        // PHP, translations, dev folders and hidden files are left behind
        assert!(!dir.join("assets/wp/index.php").exists());
        assert!(!dir.join("assets/wp/languages").exists());
        assert!(!dir.join("assets/wp/node_modules").exists());
        assert!(!dir.join("assets/wp/.git").exists());
        // The old style.css is loaded after the starter's
        let base = std::fs::read_to_string(dir.join("templates/base.html")).unwrap();
        assert!(base.contains("/theme/assets/wp/style.css"));
        assert!(!base.contains(STYLE_MARKER));
        // The manifest
        let manifest: ThemeManifest =
            serde_json::from_str(&std::fs::read_to_string(dir.join("theme.json")).unwrap())
                .unwrap();
        assert_eq!(manifest.id, "twentytest");
        assert_eq!(manifest.name, "Twenty Test");
        assert_eq!(manifest.version, "2.1");
        assert_eq!(manifest.preview_image.as_deref(), Some("preview.png"));
        assert_eq!(
            manifest.csp.unwrap().style_domains,
            ["fonts.googleapis.com"]
        );
        assert!(!dir.join("preview.svg").exists());
        // The report names what has to be done by hand
        let notes = imported.notes.join("\n");
        assert!(
            notes.contains("single.php -> templates/article.html"),
            "{notes}"
        );
        assert!(notes.contains("sidebar.php: not converted"), "{notes}");
        assert!(notes.contains("functions.php: not converted"), "{notes}");
        assert!(
            notes.contains("template-parts/content.php: not converted"),
            "{notes}"
        );
        assert!(notes.contains("1 JavaScript file"), "{notes}");

        // A second import needs --force
        let again = ImportThemeArgs {
            source: temp.path().join("twentytest"),
            id: None,
            themes_dir: themes,
            starter: None,
            force: false,
        };
        assert!(import(&again).unwrap_err().contains("already exists"));
        assert!(import(&ImportThemeArgs {
            force: true,
            ..again
        })
        .is_ok());
    }

    #[test]
    fn a_folder_that_is_not_a_wordpress_theme_is_refused() {
        let temp = tempfile::tempdir().unwrap();
        let empty = temp.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        let args = |source: PathBuf| ImportThemeArgs {
            source,
            id: None,
            themes_dir: temp.path().join("themes"),
            starter: None,
            force: false,
        };
        assert!(import(&args(empty.clone()))
            .unwrap_err()
            .contains("style.css"));
        write(&empty.join("style.css"), "body {}");
        assert!(import(&args(empty)).unwrap_err().contains("Theme Name"));
    }
}
