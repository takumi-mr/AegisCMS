//! 公式プラグイン `official-blocks`: 本文に置けるボタンとリンクカードのブロック（`render-blocks`）。
//! カスタムブロックを作るときのお手本でもある。
//!
//! - `official-blocks/button`: `label`・`url`・`style`（`primary` / `secondary`）・`align`（`left` / `center`）
//! - `official-blocks/link-card`: `url`・`title`・`description`・`image`
//! - `official-blocks/video`: `url`（YouTube・Vimeo）・`title`・`aspect`（`16x9` / `4x3` / `9x16`）。
//!   プレーヤーの URL はこのプラグインが動画の ID から組み立て（`www.youtube-nocookie.com`・
//!   `player.vimeo.com`、マニフェストの `capabilities.embed.frame_domains`）、利用者の HTML は使わない
//!
//! 見た目は最初のブロックの前に 1 回だけ入れる `<style>`。配置・余白は `:where()` で詳細度 0
//! （テーマの CSS が優先）。リンクの色と下線だけは、テーマの本文のリンクの指定（`.prose a` など）に
//! 負けないようクラス 2 つの詳細度にする（負けると塗りつぶしのボタンの文字が背景と同じ色になる）。
//! 色はテーマの `--accent`（無ければ紫）と、塗りつぶしのボタンの文字の `--accent-contrast`（無ければ白）、
//! 文字色の濃淡だけを使う

use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct BlocksPlugin;

const STYLES: &str = "<style>\
:where(.ob-button-row){margin:1.5rem 0}\
:where(.ob-button-row.ob-center){text-align:center}\
:where(.ob-button){display:inline-block;padding:.65rem 1.3rem;border-radius:8px;font-weight:600;border:1.5px solid var(--accent,#6366f1)}\
.ob-button-row .ob-button{text-decoration:none}\
.ob-button-row .ob-button-primary{background:var(--accent,#6366f1);color:var(--accent-contrast,#fff)}\
.ob-button-row .ob-button-secondary{color:var(--accent,#6366f1);background:transparent}\
:where(.ob-card){display:flex;gap:1rem;align-items:stretch;margin:1.5rem 0;border:1px solid rgba(127,127,127,.3);border-radius:10px;overflow:hidden}\
.aegis-plugin-block .ob-card{color:inherit;text-decoration:none}\
:where(.ob-card):hover{border-color:var(--accent,#6366f1)}\
:where(.ob-card-body){flex:1;min-width:0;padding:.9rem 1.1rem}\
:where(.ob-card-title){display:block;font-weight:700;margin-bottom:.25rem}\
:where(.ob-card-description){display:block;opacity:.75;font-size:.9em}\
:where(.ob-card-site){display:block;opacity:.6;font-size:.8em;margin-top:.4rem;overflow-wrap:anywhere}\
:where(.ob-card-image){flex:0 0 30%;max-width:200px;background:rgba(127,127,127,.1)}\
:where(.ob-card-image) img{width:100%;height:100%;object-fit:cover;display:block}\
:where(.ob-video){width:100%;margin:1.5rem 0;border-radius:10px;overflow:hidden;background:#000;aspect-ratio:16/9}\
:where(.ob-video-4x3){aspect-ratio:4/3}\
:where(.ob-video-9x16){aspect-ratio:9/16;max-width:360px;margin-left:auto;margin-right:auto}\
:where(.ob-video) iframe{display:block;width:100%;height:100%;border:0}\
</style>";

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(c),
        }
    }
    out
}

fn attr<'a>(block: &'a BlockNode, name: &str) -> &'a str {
    block
        .attributes
        .get(name)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("")
}

/// リンクとして出してよい URL（http(s)・mailto とスキームの無い相対 URL）
fn safe_url(url: &str) -> Option<&str> {
    let before_path = url.split(['/', '?', '#']).next().unwrap_or("");
    let allowed = match before_path.split_once(':') {
        None => true,
        Some((scheme, _)) => matches!(
            scheme.to_ascii_lowercase().as_str(),
            "http" | "https" | "mailto"
        ),
    };
    (!url.is_empty() && allowed && !url.chars().any(char::is_whitespace)).then_some(url)
}

/// 外部のサイトか（`http(s)://` で始まる）
fn is_external(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// リンクカードに出すサイト名（URL のホスト）
fn host_of(url: &str) -> Option<&str> {
    let rest = url.split_once("://")?.1;
    let host = rest.split(['/', '?', '#']).next()?;
    (!host.is_empty()).then_some(host)
}

fn render_button(block: &BlockNode) -> Option<String> {
    let label = attr(block, "label");
    let url = safe_url(attr(block, "url"))?;
    if label.is_empty() {
        return None;
    }
    let style = match attr(block, "style") {
        "secondary" => "secondary",
        _ => "primary",
    };
    let align = if attr(block, "align") == "center" {
        " ob-center"
    } else {
        ""
    };
    Some(format!(
        r#"<p class="ob-button-row{align}"><a class="ob-button ob-button-{style}" href="{}">{}</a></p>"#,
        escape(url),
        escape(label)
    ))
}

fn render_link_card(block: &BlockNode) -> Option<String> {
    let url = safe_url(attr(block, "url"))?;
    let title = attr(block, "title");
    if title.is_empty() {
        return None;
    }
    let description = attr(block, "description");
    let description = if description.is_empty() {
        String::new()
    } else {
        format!(
            r#"<span class="ob-card-description">{}</span>"#,
            escape(description)
        )
    };
    let site = if is_external(url) {
        host_of(url)
            .map(|h| format!(r#"<span class="ob-card-site">{}</span>"#, escape(h)))
            .unwrap_or_default()
    } else {
        String::new()
    };
    let image = safe_url(attr(block, "image"))
        .map(|src| {
            format!(
                r#"<span class="ob-card-image"><img src="{}" alt=""></span>"#,
                escape(src)
            )
        })
        .unwrap_or_default();
    Some(format!(
        r#"<a class="ob-card" href="{}"><span class="ob-card-body"><span class="ob-card-title">{}</span>{description}{site}</span>{image}</a>"#,
        escape(url),
        escape(title)
    ))
}

/// 埋め込む動画
#[derive(Debug, PartialEq)]
enum Video {
    /// 動画 ID と再生開始位置（秒）
    YouTube { id: String, start: Option<u32> },
    /// 動画 ID と限定公開の動画のハッシュ
    Vimeo { id: String, hash: Option<String> },
}

impl Video {
    /// プレーヤーの URL（`embed.frame_domains` のホストだけを使う）
    fn player_url(&self) -> String {
        match self {
            Video::YouTube { id, start } => match start {
                Some(s) if *s > 0 => {
                    format!("https://www.youtube-nocookie.com/embed/{}?start={}", id, s)
                }
                _ => format!("https://www.youtube-nocookie.com/embed/{}", id),
            },
            Video::Vimeo { id, hash } => match hash {
                Some(h) => format!("https://player.vimeo.com/video/{}?h={}&dnt=1", id, h),
                None => format!("https://player.vimeo.com/video/{}?dnt=1", id),
            },
        }
    }

    fn provider(&self) -> &'static str {
        match self {
            Video::YouTube { .. } => "YouTube",
            Video::Vimeo { .. } => "Vimeo",
        }
    }
}

fn is_youtube_id(id: &str) -> bool {
    id.len() == 11
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// `90`、`90s`、`1m30s`、`1h2m3s` を秒にする
fn parse_start(value: &str) -> Option<u32> {
    if let Ok(seconds) = value.parse::<u32>() {
        return Some(seconds);
    }
    let mut total: u32 = 0;
    let mut number = String::new();
    for c in value.chars() {
        if c.is_ascii_digit() {
            number.push(c);
            continue;
        }
        let n: u32 = number.parse().ok()?;
        number.clear();
        total = total.checked_add(match c {
            'h' => n.checked_mul(3600)?,
            'm' => n.checked_mul(60)?,
            's' => n,
            _ => return None,
        })?;
    }
    number.is_empty().then_some(total)
}

/// YouTube（`watch?v=`・`youtu.be/`・`shorts/`・`embed/`・`live/`）か Vimeo の URL を読む
fn parse_video(url: &str) -> Option<Video> {
    let url = url.trim();
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let (host, path_and_query) = match rest.find(['/', '?', '#']) {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let host = host.to_ascii_lowercase();
    let (path, query) = match path_and_query.split_once('?') {
        Some((p, q)) => (p, q.split('#').next().unwrap_or("")),
        None => (path_and_query.split('#').next().unwrap_or(""), ""),
    };
    let param = |name: &str| {
        query
            .split('&')
            .filter_map(|kv| kv.split_once('='))
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v)
    };
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    match host.as_str() {
        "youtube.com"
        | "www.youtube.com"
        | "m.youtube.com"
        | "youtube-nocookie.com"
        | "www.youtube-nocookie.com"
        | "youtu.be" => {
            let id = if host == "youtu.be" {
                segments.first().copied()
            } else {
                match segments.as_slice() {
                    ["watch"] => param("v"),
                    ["shorts" | "embed" | "live", id, ..] => Some(*id),
                    _ => None,
                }
            }?;
            if !is_youtube_id(id) {
                return None;
            }
            let start = param("t").or_else(|| param("start")).and_then(parse_start);
            Some(Video::YouTube {
                id: id.to_string(),
                start,
            })
        }
        "vimeo.com" | "www.vimeo.com" | "player.vimeo.com" => {
            let (id, hash) = match segments.as_slice() {
                ["video", id, ..] => (*id, param("h")),
                [id, hash, ..] => (*id, Some(*hash)),
                [id] => (*id, param("h")),
                _ => return None,
            };
            if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let hash = hash.filter(|h| !h.is_empty() && h.bytes().all(|b| b.is_ascii_hexdigit()));
            Some(Video::Vimeo {
                id: id.to_string(),
                hash: hash.map(str::to_string),
            })
        }
        _ => None,
    }
}

fn render_video(block: &BlockNode) -> Option<String> {
    let video = parse_video(attr(block, "url"))?;
    let aspect = match attr(block, "aspect") {
        "4x3" => "4x3",
        "9x16" => "9x16",
        _ => "16x9",
    };
    let title = match attr(block, "title") {
        "" => video.provider(),
        title => title,
    };
    Some(format!(
        r#"<div class="ob-video ob-video-{aspect}"><iframe src="{}" title="{}" loading="lazy" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share; fullscreen" referrerpolicy="strict-origin-when-cross-origin" allowfullscreen></iframe></div>"#,
        escape(&video.player_url()),
        escape(title)
    ))
}

impl Plugin for BlocksPlugin {
    fn render_blocks(&mut self, blocks: Vec<BlockNode>) -> Result<Vec<Option<String>>> {
        let mut styled = false;
        Ok(blocks
            .iter()
            .map(|block| {
                let html = match block.name.as_str() {
                    "official-blocks/button" => render_button(block),
                    "official-blocks/link-card" => render_link_card(block),
                    "official-blocks/video" => render_video(block),
                    _ => None,
                }?;
                // スタイルは最初に表示するブロックにだけ付ける
                if styled {
                    Some(html)
                } else {
                    styled = true;
                    Some(format!("{}{}", STYLES, html))
                }
            })
            .collect())
    }
}

aegis_plugin_sdk::register_plugin!(BlocksPlugin);

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn block(name: &str, attrs: &[(&str, &str)]) -> BlockNode {
        BlockNode {
            name: format!("official-blocks/{}", name),
            attributes: attrs
                .iter()
                .map(|(k, v)| (k.to_string(), serde_json::json!(v)))
                .collect::<HashMap<_, _>>(),
            text: None,
            children: Vec::new(),
        }
    }

    #[test]
    fn renders_buttons_and_cards_with_the_styles_once() {
        let html = BlocksPlugin
            .render_blocks(vec![
                block(
                    "button",
                    &[
                        ("label", "Start <now>"),
                        ("url", "/start"),
                        ("align", "center"),
                    ],
                ),
                block(
                    "link-card",
                    &[
                        ("url", "https://example.com/docs?a=1"),
                        ("title", "Docs"),
                        ("description", "Read \"this\""),
                        ("image", "/uploads/a.png"),
                    ],
                ),
            ])
            .unwrap();
        assert_eq!(html.len(), 2);
        let button = html[0].as_deref().unwrap();
        assert!(button.starts_with("<style>"));
        assert!(button.ends_with(r#"<p class="ob-button-row ob-center"><a class="ob-button ob-button-primary" href="/start">Start &lt;now&gt;</a></p>"#));
        let card = html[1].as_deref().unwrap();
        assert!(!card.contains("<style>"));
        assert!(card.contains(r#"href="https://example.com/docs?a=1""#));
        assert!(card.contains("Read &quot;this&quot;"));
        assert!(card.contains(r#"<span class="ob-card-site">example.com</span>"#));
        assert!(card.contains(r#"<img src="/uploads/a.png" alt="">"#));
    }

    #[test]
    fn skips_incomplete_or_unsafe_blocks() {
        let html = BlocksPlugin
            .render_blocks(vec![
                block("button", &[("label", "x"), ("url", "javascript:alert(1)")]),
                block("button", &[("label", ""), ("url", "/a")]),
                block("link-card", &[("url", "https://a.example")]),
                block("unknown", &[]),
                block(
                    "button",
                    &[
                        ("label", "ok"),
                        ("url", "mailto:a@example.com"),
                        ("style", "secondary"),
                    ],
                ),
            ])
            .unwrap();
        assert_eq!(&html[..4], &[None, None, None, None]);
        // スタイルは実際に表示する最初のブロックに付く
        let last = html[4].as_deref().unwrap();
        assert!(last.starts_with("<style>") && last.contains("ob-button-secondary"));
    }

    #[test]
    fn reads_youtube_and_vimeo_urls() {
        let yt = |id: &str, start: Option<u32>| {
            Some(Video::YouTube {
                id: id.to_string(),
                start,
            })
        };
        assert_eq!(
            parse_video("https://www.youtube.com/watch?v=dQw4w9WgXcQ"),
            yt("dQw4w9WgXcQ", None)
        );
        assert_eq!(
            parse_video("https://youtube.com/watch?feature=x&v=dQw4w9WgXcQ&t=1m30s"),
            yt("dQw4w9WgXcQ", Some(90))
        );
        assert_eq!(
            parse_video("https://youtu.be/dQw4w9WgXcQ?t=42"),
            yt("dQw4w9WgXcQ", Some(42))
        );
        assert_eq!(
            parse_video("https://www.youtube.com/shorts/dQw4w9WgXcQ"),
            yt("dQw4w9WgXcQ", None)
        );
        assert_eq!(
            parse_video("https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ?start=5"),
            yt("dQw4w9WgXcQ", Some(5))
        );
        assert_eq!(
            parse_video("http://m.youtube.com/watch?v=dQw4w9WgXcQ#x"),
            yt("dQw4w9WgXcQ", None)
        );
        assert_eq!(
            parse_video("https://vimeo.com/76979871"),
            Some(Video::Vimeo {
                id: "76979871".to_string(),
                hash: None
            })
        );
        assert_eq!(
            parse_video("https://vimeo.com/76979871/abc123ff"),
            Some(Video::Vimeo {
                id: "76979871".to_string(),
                hash: Some("abc123ff".to_string())
            })
        );
        assert_eq!(
            parse_video("https://player.vimeo.com/video/76979871?h=abc1"),
            Some(Video::Vimeo {
                id: "76979871".to_string(),
                hash: Some("abc1".to_string())
            })
        );
        for bad in [
            "https://www.youtube.com/watch?v=short",
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ\"x",
            "https://youtube.com.evil.example/watch?v=dQw4w9WgXcQ",
            "https://evil.example/youtu.be/dQw4w9WgXcQ",
            "javascript:alert(1)",
            "https://vimeo.com/abc",
            "https://vimeo.com/channels/staffpicks",
            "dQw4w9WgXcQ",
        ] {
            assert_eq!(parse_video(bad), None, "{}", bad);
        }
        assert_eq!(parse_start("1h2m3s"), Some(3723));
        assert_eq!(parse_start("x"), None);
        assert_eq!(parse_start("10m5"), None);
    }

    #[test]
    fn renders_videos_from_the_declared_players_only() {
        let html = BlocksPlugin
            .render_blocks(vec![
                block(
                    "video",
                    &[
                        ("url", "https://youtu.be/dQw4w9WgXcQ?t=42"),
                        ("title", "Demo \"1\""),
                    ],
                ),
                block(
                    "video",
                    &[("url", "https://vimeo.com/76979871"), ("aspect", "9x16")],
                ),
                block("video", &[("url", "https://example.com/video.mp4")]),
            ])
            .unwrap();
        let first = html[0].as_deref().unwrap();
        assert!(first.contains(r#"<div class="ob-video ob-video-16x9"><iframe src="https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ?start=42" title="Demo &quot;1&quot;" loading="lazy""#), "{}", first);
        let second = html[1].as_deref().unwrap();
        assert!(second.contains(r#"class="ob-video ob-video-9x16"><iframe src="https://player.vimeo.com/video/76979871?dnt=1" title="Vimeo""#), "{}", second);
        assert_eq!(html[2], None);

        // プレーヤーのホストはマニフェストの embed.frame_domains と一致していること
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../plugin.manifest.json")).unwrap();
        let domains = manifest["capabilities"]["embed"]["frame_domains"].clone();
        for host in ["www.youtube-nocookie.com", "player.vimeo.com"] {
            assert!(
                domains.as_array().unwrap().iter().any(|d| d == host),
                "{}",
                host
            );
        }
    }
}
