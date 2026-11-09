//! 公開ページの Strict CSP（`[security].strict_csp`）のための、インラインのスクリプトのハッシュの収集。docs/csp.md の 3。
//!
//! インラインのスクリプトを**信頼できる出どころが出すとき**（プラグインの `scripts`・テンプレートの
//! `inline_script()`）に、本文の SHA-256 をここへ記録する。リクエストを包むミドルウェアが
//! [`collect_script_hashes`] で受け口を作り、応答の `Content-Security-Policy` の `script-src` に足す。
//! 最終の HTML の `<script>` を全部ハッシュするのではない（攻撃者が差し込んだものまで許可してしまう）。
//!
//! ハッシュは**ページの内容で決まる**ので、CDN・リバースプロキシが応答をキャッシュしても（ヘッダーと本文が
//! 同じ応答に入る）一致し続ける。受け口の無いところ（リクエストの外）で記録しても、何も起きない。

use std::collections::BTreeSet;
use std::future::Future;
use std::sync::{Arc, Mutex};

use base64::Engine;
use sha2::{Digest, Sha256};

/// 受け口。リクエストの間だけ有効（`tokio::task_local`）
struct Collector {
    /// ハッシュを取る前に本文へ施す変換（応答が返る前に本文を書き換えるもの。例: 名前空間の置き換え）。
    /// ブラウザーが実行する本文と、ハッシュを取る本文を一致させる
    normalize: Box<dyn Fn(&str) -> String + Send + Sync>,
    hashes: Mutex<BTreeSet<String>>,
    /// インラインのスタイル（`<style>` の本文）のハッシュ
    style_hashes: Mutex<BTreeSet<String>>,
    /// `normalize` を掛ける前の本文のハッシュ（スクリプト・スタイル）。名前空間の置き換えを受けない応答
    /// （プレビューのページなど）に使う
    raw_hashes: Mutex<BTreeSet<String>>,
    raw_style_hashes: Mutex<BTreeSet<String>>,
}

/// リクエストの間に集めたハッシュ（`sha256-<base64>`。引用符なし）
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CollectedHashes {
    /// インラインのスクリプト（`script-src` に足す）
    pub scripts: Vec<String>,
    /// インラインのスタイルの `<style>`（`style-src` に足す）
    pub styles: Vec<String>,
}

tokio::task_local! {
    static COLLECTOR: Arc<Collector>;
}

/// CSP の `script-src` に書く、インラインのスクリプトのハッシュ（`'sha256-<base64>'` の中身。引用符なし）
pub fn script_hash(body: &str) -> String {
    let digest = Sha256::digest(body.as_bytes());
    format!(
        "sha256-{}",
        base64::engine::general_purpose::STANDARD.encode(digest)
    )
}

/// `fut` の間だけ受け口を作り、`fut` が記録したハッシュ（重複なし・順序は文字列順）を返す。
/// `normalize` は、記録する本文に施す変換（応答の本文を書き換える処理と同じものを渡す）
pub async fn collect_hashes<F: Future>(
    normalize: impl Fn(&str) -> String + Send + Sync + 'static,
    fut: F,
) -> (F::Output, CollectedHashes) {
    let collector = Arc::new(Collector {
        normalize: Box::new(normalize),
        hashes: Mutex::new(BTreeSet::new()),
        style_hashes: Mutex::new(BTreeSet::new()),
        raw_hashes: Mutex::new(BTreeSet::new()),
        raw_style_hashes: Mutex::new(BTreeSet::new()),
    });
    let output = COLLECTOR.scope(collector.clone(), fut).await;
    let list = |set: &Mutex<BTreeSet<String>>| -> Vec<String> {
        set.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .cloned()
            .collect()
    };
    let collected = CollectedHashes {
        scripts: list(&collector.hashes),
        styles: list(&collector.style_hashes),
    };
    (output, collected)
}

/// [`collect_hashes`] のうち、スクリプトのハッシュだけ返すもの
pub async fn collect_script_hashes<F: Future>(
    normalize: impl Fn(&str) -> String + Send + Sync + 'static,
    fut: F,
) -> (F::Output, Vec<String>) {
    let (output, collected) = collect_hashes(normalize, fut).await;
    (output, collected.scripts)
}

/// インラインのスタイル（`<style>` の中身そのもの）を記録する。受け口が無ければ何もしない
pub fn record_inline_style(body: &str) {
    let _ = COLLECTOR.try_with(|collector| {
        let text = (collector.normalize)(body);
        collector
            .style_hashes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(script_hash(&text));
        collector
            .raw_style_hashes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(script_hash(body));
    });
}

/// [`record_inline_style`] で記録したスタイルの、置き換えを掛けない本文のハッシュ
pub fn recorded_raw_style_hashes() -> Vec<String> {
    COLLECTOR
        .try_with(|collector| {
            collector
                .raw_style_hashes
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .iter()
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// [`record_inline_script`] で記録したスクリプトの、置き換えを掛けない本文のハッシュ
pub fn recorded_raw_script_hashes() -> Vec<String> {
    COLLECTOR
        .try_with(|collector| {
            collector
                .raw_hashes
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .iter()
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// 取っておいたスタイルのハッシュをそのまま記録する
pub fn record_style_hash(hash: &str) {
    let _ = COLLECTOR.try_with(|collector| {
        collector
            .style_hashes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(hash.to_string());
    });
}

/// いま（このリクエストで）記録したスタイルのハッシュ
pub fn recorded_style_hashes() -> Vec<String> {
    COLLECTOR
        .try_with(|collector| {
            collector
                .style_hashes
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .iter()
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// `html` の中の `<tag …>本文</tag>` を、（属性の文字列, 本文）で列挙する（`<tags>` のような別のタグは含めない）
fn elements<'a>(html: &'a str, tag: &str) -> Vec<(&'a str, &'a str)> {
    let lower = html.to_ascii_lowercase();
    let open_tag = format!("<{tag}");
    let close_tag = format!("</{tag}");
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(open) = lower[from..].find(&open_tag) {
        let after = from + open + open_tag.len();
        match lower.as_bytes().get(after) {
            Some(b'>') | Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') => {}
            _ => {
                from = after;
                continue;
            }
        }
        let Some(tag_end) = lower[after..].find('>') else {
            break;
        };
        let body_start = after + tag_end + 1;
        let Some(close) = lower[body_start..].find(&close_tag) else {
            break;
        };
        found.push((
            &html[after..after + tag_end],
            &html[body_start..body_start + close],
        ));
        from = body_start + close;
    }
    found
}

/// 信頼できる出どころの HTML の断片（プラグインのブロックや `extra_head`）の中の `<style>` 要素を全部記録する。
/// ユーザーが書いた本文には使わない
pub fn record_styles_in(html: &str) {
    for (_, body) in elements(html, "style") {
        record_inline_style(body);
    }
}

/// 信頼できる出どころの HTML の断片の中の、`src` の無い `<script>` を全部記録する
/// （管理者が書いた `core/html` のサンドボックスの中身など。外部のスクリプトの `src` はホストの許可で決まる）
pub fn record_inline_scripts_in(html: &str) {
    for (attrs, body) in elements(html, "script") {
        if !attrs.to_ascii_lowercase().contains("src") {
            record_inline_script(body);
        }
    }
}

/// インラインのスタイルの本文を、`<style>` の中に置ける形にする（`</style` を CSS の escape にする）
pub fn escape_inline_style(css: &str) -> String {
    let lower = css.to_ascii_lowercase();
    let mut out = String::with_capacity(css.len());
    let mut i = 0;
    while i < css.len() {
        if lower[i..].starts_with("</style") {
            out.push_str("<\\/style");
            i += "</style".len();
        } else {
            let ch = css[i..].chars().next().map_or(1, char::len_utf8);
            out.push_str(&css[i..i + ch]);
            i += ch;
        }
    }
    out
}

/// インラインのスタイルの `<style>` を作り、ハッシュを記録する（テンプレートの `inline_style()` など）
pub fn inline_style_tag(css: &str) -> String {
    let body = escape_inline_style(css);
    record_inline_style(&body);
    format!("<style>{body}</style>")
}

/// インラインのスクリプトの本文（`<script>` の中身そのもの）を記録する。受け口が無ければ何もしない
pub fn record_inline_script(body: &str) {
    let _ = COLLECTOR.try_with(|collector| {
        let text = (collector.normalize)(body);
        collector
            .hashes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(script_hash(&text));
        collector
            .raw_hashes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(script_hash(body));
    });
}

/// 取っておいたハッシュ（`script_hash` の値）をそのまま記録する（プレビューのページを置いておいて、
/// あとで配信するときなど）
pub fn record_script_hash(hash: &str) {
    let _ = COLLECTOR.try_with(|collector| {
        collector
            .hashes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(hash.to_string());
    });
}

/// いま（このリクエストで）記録したハッシュ
pub fn recorded_script_hashes() -> Vec<String> {
    COLLECTOR
        .try_with(|collector| {
            collector
                .hashes
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .iter()
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// インラインのスクリプトの本文を、`<script>` の中に置ける形にする。`</script` と `<!--` は、
/// タグや HTML のコメントとして読まれるので、JavaScript の文字列として同じ意味になる形にする
pub fn escape_inline_script(code: &str) -> String {
    let mut out = String::with_capacity(code.len());
    let lower = code.to_ascii_lowercase();
    let (bytes, lower_bytes) = (code.as_bytes(), lower.as_bytes());
    let mut i = 0;
    while i < bytes.len() {
        if lower_bytes[i..].starts_with(b"</script") || lower_bytes[i..].starts_with(b"<!--") {
            out.push_str("<\\");
            i += 1;
        } else {
            let ch_len = code[i..].chars().next().map_or(1, char::len_utf8);
            out.push_str(&code[i..i + ch_len]);
            i += ch_len;
        }
    }
    out
}

/// インラインのスクリプトのタグを作り、ハッシュを記録する（テンプレートの `inline_script()` など）。
/// `consent` があれば、同意のあとに実行する形（`type="text/plain" data-aegis-consent`）で出す
pub fn inline_script_tag(code: &str, consent: Option<&str>) -> String {
    let body = escape_inline_script(code);
    record_inline_script(&body);
    match consent {
        Some(category) => {
            format!(r#"<script type="text/plain" data-aegis-consent="{category}">{body}</script>"#)
        }
        None => format!("<script>{body}</script>"),
    }
}

/// 公開ページの既定の CSP（`headers::DEFAULT_CONTENT_SECURITY_POLICY`。`theme dev` も同じ値を使う）
pub const DEFAULT_PUBLIC_CSP: &str = "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; font-src 'self' data: https:; connect-src 'self'; frame-ancestors 'self'; object-src 'none'; base-uri 'self'; form-action 'self';";

/// Strict CSP（`[security].strict_csp`）の値を作る。`csp` の `script-src` から `'unsafe-inline'`・`'unsafe-eval'` を外し、
/// 信頼できる出どころが出したインラインのスクリプトのハッシュ（`hashes`。`sha256-<base64>`）を足す。
/// `script-src` が無ければ（`default-src` が効いている）、`'self'` とハッシュだけの `script-src` を足す。
/// 許可するホスト（プラグインのもの）は残す。`'strict-dynamic'` は使わない（ブラウザーがホストの指定を
/// 無視してしまい、マニフェストで宣言した `script_domains` の強制が無くなるため。docs/csp.md の 3.1）
pub fn strict_script_csp(csp: &str, hashes: &[String]) -> String {
    let hash_tokens: Vec<String> = hashes.iter().map(|h| format!("'{h}'")).collect();
    let mut found = false;
    let mut directives: Vec<String> = csp
        .split(';')
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(|directive| {
            let mut parts = directive.split_whitespace();
            if parts.next() != Some("script-src") {
                return directive.to_string();
            }
            found = true;
            let mut kept: Vec<&str> = parts
                .filter(|t| !matches!(*t, "'unsafe-inline'" | "'unsafe-eval'"))
                .collect();
            if !kept.contains(&"'self'") {
                kept.insert(0, "'self'");
            }
            let mut out = String::from("script-src");
            for token in kept
                .iter()
                .map(|t| t.to_string())
                .chain(hash_tokens.iter().cloned())
            {
                out.push(' ');
                out.push_str(&token);
            }
            out
        })
        .collect();
    if !found {
        let mut out = String::from("script-src 'self'");
        for token in &hash_tokens {
            out.push(' ');
            out.push_str(token);
        }
        directives.push(out);
    }
    format!("{};", directives.join("; "))
}

/// 公開ページの Strict CSP の全体: [`strict_script_csp`] と [`strict_style_csp`]
pub fn strict_csp(csp: &str, hashes: &CollectedHashes) -> String {
    strict_style_csp(&strict_script_csp(csp, &hashes.scripts), &hashes.styles)
}

/// Strict CSP のスタイル側。`style-src` から `'unsafe-inline'` を外し、信頼できる出どころの `<style>` の
/// ハッシュ（`hashes`）を足す（`style-src` が無ければ `'self'` とハッシュだけの `style-src` を足す）。
/// `style=""` 属性は許さない（`style-src` が `style-src-attr` にも効く。組み込みのテンプレートとプラグインは
/// クラスを使う。docs/csp.md の 6.1）
pub fn strict_style_csp(csp: &str, hashes: &[String]) -> String {
    let hash_tokens: Vec<String> = hashes.iter().map(|h| format!("'{h}'")).collect();
    let mut found = false;
    let mut directives: Vec<String> = csp
        .split(';')
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(|directive| {
            let mut parts = directive.split_whitespace();
            match parts.next() {
                Some("style-src") => {
                    found = true;
                    let mut kept: Vec<&str> = parts.filter(|t| *t != "'unsafe-inline'").collect();
                    if !kept.contains(&"'self'") {
                        kept.insert(0, "'self'");
                    }
                    let mut out = String::from("style-src");
                    for token in kept
                        .iter()
                        .map(|t| t.to_string())
                        .chain(hash_tokens.iter().cloned())
                    {
                        out.push(' ');
                        out.push_str(&token);
                    }
                    out
                }
                _ => directive.to_string(),
            }
        })
        .collect();
    if !found {
        let mut out = String::from("style-src 'self'");
        for token in &hash_tokens {
            out.push(' ');
            out.push_str(token);
        }
        directives.push(out);
    }
    format!("{};", directives.join("; "))
}

/// `directive` に効いている値（無ければ `fallbacks` の最初に見つかったもの）に、`https://<host>` を足す。
/// `directive` が無いときは、足した値で `directive` を加える。値に `*` か `'none'` を含む（制限していない・
/// 管理者が明示的に拒否している）とき、どれも無い（制限していない）ときは、そのまま
pub fn with_directive_sources(
    csp: &str,
    directive: &str,
    fallbacks: &[&str],
    hosts: &[String],
) -> String {
    if hosts.is_empty() {
        return csp.to_string();
    }
    let directives: Vec<&str> = csp
        .split(';')
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .collect();
    let find = |name: &str| {
        directives.iter().position(|d| {
            d.split_whitespace()
                .next()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
    };
    let sources_of = |i: usize| -> Vec<String> {
        directives[i]
            .split_whitespace()
            .skip(1)
            .map(str::to_string)
            .collect()
    };
    let add_hosts = |mut sources: Vec<String>| -> Option<Vec<String>> {
        if sources
            .iter()
            .any(|s| s == "*" || s.eq_ignore_ascii_case("'none'"))
        {
            return None;
        }
        for host in hosts {
            let source = format!("https://{}", host);
            if !sources.iter().any(|s| s.eq_ignore_ascii_case(&source)) {
                sources.push(source);
            }
        }
        Some(sources)
    };

    let mut out: Vec<String> = directives.iter().map(|d| d.to_string()).collect();
    if let Some(i) = find(directive) {
        match add_hosts(sources_of(i)) {
            Some(sources) => out[i] = format!("{} {}", directive, sources.join(" ")),
            None => return csp.to_string(),
        }
    } else if let Some(i) = fallbacks.iter().find_map(|name| find(name)) {
        match add_hosts(sources_of(i)) {
            Some(sources) => out.push(format!("{} {}", directive, sources.join(" "))),
            None => return csp.to_string(),
        }
    } else {
        return csp.to_string();
    }
    format!("{};", out.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_is_the_csp_sha256_of_the_body() {
        // `echo -n "alert(1)" | openssl dgst -sha256 -binary | base64`
        assert_eq!(
            script_hash("alert(1)"),
            "sha256-bhHHL3z2vDgxUt0W3dWQOrprscmda2Y5pLsLg4GF+pI="
        );
    }

    #[tokio::test]
    async fn hashes_are_collected_per_request_and_normalized() {
        let ((), hashes) = collect_script_hashes(|body| body.replace("aegis", "cms"), async {
            record_inline_script("a();");
            record_inline_script("a();");
            record_inline_script("window.aegis = 1;");
            assert_eq!(recorded_script_hashes().len(), 2);
        })
        .await;
        // 同じ本文は 1 つ。名前空間のように書き換わる本文は、書き換えたあとのハッシュ
        assert_eq!(hashes, {
            let mut want = vec![script_hash("a();"), script_hash("window.cms = 1;")];
            want.sort();
            want
        });
        // 別のリクエストには混ざらず、受け口が無ければ何も起きない
        record_inline_script("outside();");
        let ((), second) = collect_script_hashes(|b| b.to_string(), async {}).await;
        assert!(second.is_empty());
        assert!(recorded_script_hashes().is_empty());
    }

    #[tokio::test]
    async fn styles_are_collected_apart_from_scripts() {
        let ((), got) = collect_hashes(|b| b.replace("aegis", "cms"), async {
            record_inline_script("a();");
            inline_style_tag(".aegis-x{color:red}");
            record_styles_in(
                "<p>x</p><style>.a{}</style><STYLE>.b{}</STYLE><style media=\"print\">.c{}</style><styles>.d{}</styles>",
            );
        })
        .await;
        assert_eq!(got.scripts, vec![script_hash("a();")]);
        let mut want = vec![
            script_hash(".cms-x{color:red}"),
            script_hash(".a{}"),
            script_hash(".b{}"),
            script_hash(".c{}"),
        ];
        want.sort();
        assert_eq!(got.styles, want);
    }

    #[test]
    fn the_style_tag_cannot_be_closed_early() {
        let tag = inline_style_tag("a{}</style><script>x()</script>");
        assert_eq!(tag.matches("</style>").count(), 1, "{tag}");
    }

    #[test]
    fn the_tag_escapes_the_body_and_is_hashed_as_emitted() {
        let tag = inline_script_tag("a='</script><script>x()';", None);
        assert_eq!(tag.matches("</script>").count(), 1, "{tag}");
        let tag = inline_script_tag("y()", Some("analytics"));
        assert_eq!(
            tag,
            r#"<script type="text/plain" data-aegis-consent="analytics">y()</script>"#
        );
    }
}
