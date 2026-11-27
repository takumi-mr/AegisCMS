//! 公式プラグイン `official-embeds`: 外部のサービスのページを `<iframe>` で本文に埋め込むブロック
//! （`render-blocks`）。
//!
//! - `official-embeds/video`: `url`（YouTube・Vimeo）・`title`・`aspect`（`16x9` / `4x3` / `9x16`）
//! - `official-embeds/openstreetmap`: `latitude`・`longitude`・`zoom`・`title`。住所からの検索はしない
//! - `official-embeds/codepen`: `url`（Pen の URL）・`tab`（最初に開くタブ）・`height`・`click_to_load`・`title`
//! - `official-embeds/google-map`: `embed`（Google マップの「地図を埋め込む」の HTML か、その `src` の URL）・
//!   `aspect`・`title`
//! - `official-embeds/google-calendar`: `calendar`（埋め込みの HTML・URL か、カレンダー ID）・`mode`・
//!   `timezone`・`height`・`title`
//! - `official-embeds/google-form`: `url`（フォームの URL か、埋め込みの HTML）・`height`・`title`
//!
//! 外部のサイトに依存するブロックは、すべてこのプラグインに置く（`official-blocks` には置かない）。
//! プラグインを有効にしたときだけ、埋め込み先のホストが CSP の `frame-src` に入るようにするため。
//!
//! どのブロックも、利用者が入力した URL や HTML をそのまま出さない。ID や必要な値だけを取り出して検証し、
//! iframe の `src` はこのプラグインが組み立てる。使うホストはマニフェストの
//! `capabilities.embed.frame_domains` にあるものだけ（テストで突き合わせる）。
//!
//! 見た目は最初のブロックの前に 1 回だけ入れる `<style>`。`:where()` で詳細度 0（テーマの CSS が優先）

use aegis_plugin_sdk::prelude::*;

#[derive(Default)]
pub struct EmbedsPlugin;

const STYLES: &str = "<style>\
:where(.oe-frame){width:100%;margin:1.5rem 0;border-radius:10px;overflow:hidden}\
:where(.oe-frame) iframe{display:block;width:100%;border:0}\
:where(.oe-ratio) iframe{height:100%}\
:where(.oe-video){background:#000;aspect-ratio:16/9}\
:where(.oe-video-4x3){aspect-ratio:4/3}\
:where(.oe-video-9x16){aspect-ratio:9/16;max-width:360px;margin-left:auto;margin-right:auto}\
:where(.oe-map){margin:1.5rem 0}\
:where(.oe-map) .oe-frame{margin:0;aspect-ratio:16/10;background:rgba(127,127,127,.1)}\
:where(.oe-map-4x3) .oe-frame{aspect-ratio:4/3}\
:where(.oe-map-1x1) .oe-frame{aspect-ratio:1/1}\
:where(.oe-map) figcaption{font-size:.85em;opacity:.75;margin-top:.4rem}\
:where(.oe-form){background:#fff}\
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

/// 数値の入力欄の値（数値でも、文字列で保存されていても読む）
fn number(block: &BlockNode, name: &str) -> Option<f64> {
    let value = block.attributes.get(name)?;
    let n = match value {
        serde_json::Value::Number(n) => n.as_f64()?,
        serde_json::Value::String(s) => s.trim().parse().ok()?,
        _ => return None,
    };
    n.is_finite().then_some(n)
}

/// 真偽値の入力欄の値（`true` / `"true"`）
fn flag(block: &BlockNode, name: &str) -> bool {
    match block.attributes.get(name) {
        Some(serde_json::Value::Bool(b)) => *b,
        Some(serde_json::Value::String(s)) => s.trim() == "true",
        _ => false,
    }
}

/// iframe の高さ（px）。入力が無い・数値でなければ `default`、範囲外は `min`〜`max` に収める
fn height(block: &BlockNode, default: u32, min: u32, max: u32) -> u32 {
    number(block, "height")
        .map(|h| h.round().clamp(f64::from(min), f64::from(max)) as u32)
        .unwrap_or(default)
}

/// 読み上げ用のタイトル（空ならサービス名）
fn title_or<'a>(block: &'a BlockNode, fallback: &'a str) -> &'a str {
    match attr(block, "title") {
        "" => fallback,
        title => title,
    }
}

/// 埋め込みの HTML（`<iframe src="...">`）が貼られたら `src` の値を、そうでなければ入力全体を返す。
/// 属性の中の `&amp;` は `&` に戻す
fn extract_src(input: &str) -> String {
    let input = input.trim();
    let lower = input.to_ascii_lowercase();
    let value = match lower.find("src=") {
        Some(i) if lower.contains("<iframe") => {
            let rest = &input[i + 4..];
            match rest.chars().next() {
                Some(q @ ('"' | '\'')) => rest[1..].split(q).next().unwrap_or(""),
                _ => rest
                    .split(|c: char| c.is_whitespace() || c == '>')
                    .next()
                    .unwrap_or(""),
            }
        }
        _ => input,
    };
    value.trim().replace("&amp;", "&")
}

/// `http(s)://` の URL を、英小文字にしたホスト・パスの区切り・クエリの組に分ける
struct Url<'a> {
    host: String,
    segments: Vec<&'a str>,
    query: &'a str,
}

impl<'a> Url<'a> {
    fn parse(url: &'a str) -> Option<Self> {
        let rest = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))?;
        if url
            .chars()
            .any(|c| c.is_whitespace() || c == '"' || c == '<')
        {
            return None;
        }
        let (host, path_and_query) = match rest.find(['/', '?', '#']) {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, ""),
        };
        let (path, query) = match path_and_query.split_once('?') {
            Some((p, q)) => (p, q.split('#').next().unwrap_or("")),
            None => (path_and_query.split('#').next().unwrap_or(""), ""),
        };
        Some(Url {
            host: host.to_ascii_lowercase(),
            segments: path.split('/').filter(|s| !s.is_empty()).collect(),
            query,
        })
    }

    /// クエリの値（最初のもの。デコードはしない）
    fn param(&self, name: &str) -> Option<&'a str> {
        self.params(name).next()
    }

    /// 同じ名前のクエリの値をすべて（デコードはしない）
    fn params<'b>(&self, name: &'b str) -> impl Iterator<Item = &'a str> + 'b
    where
        'a: 'b,
    {
        self.query
            .split('&')
            .filter_map(|kv| kv.split_once('='))
            .filter(move |(k, _)| *k == name)
            .map(|(_, v)| v)
    }
}

/// `%XX` を戻す（`+` は空白にする）。壊れた `%` や UTF-8 でないものは `None`
fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hex = value.get(i + 1..i + 3)?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// クエリの値に入れるため、英数字と `-._~` 以外を `%XX` にする
fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

fn is_id(value: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

// ---------------------------------------------------------------------------
// 動画（YouTube・Vimeo）

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
    let url = Url::parse(url.trim())?;
    match url.host.as_str() {
        "youtube.com"
        | "www.youtube.com"
        | "m.youtube.com"
        | "youtube-nocookie.com"
        | "www.youtube-nocookie.com"
        | "youtu.be" => {
            let id = if url.host == "youtu.be" {
                url.segments.first().copied()
            } else {
                match url.segments.as_slice() {
                    ["watch"] => url.param("v"),
                    ["shorts" | "embed" | "live", id, ..] => Some(*id),
                    _ => None,
                }
            }?;
            if !is_id(id, 11, 11) {
                return None;
            }
            let start = url
                .param("t")
                .or_else(|| url.param("start"))
                .and_then(parse_start);
            Some(Video::YouTube {
                id: id.to_string(),
                start,
            })
        }
        "vimeo.com" | "www.vimeo.com" | "player.vimeo.com" => {
            let (id, hash) = match url.segments.as_slice() {
                ["video", id, ..] => (*id, url.param("h")),
                [id, hash, ..] => (*id, Some(*hash)),
                [id] => (*id, url.param("h")),
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
    Some(format!(
        r#"<div class="oe-frame oe-ratio oe-video oe-video-{aspect}"><iframe src="{}" title="{}" loading="lazy" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share; fullscreen" referrerpolicy="strict-origin-when-cross-origin" allowfullscreen></iframe></div>"#,
        escape(&video.player_url()),
        escape(title_or(block, video.provider()))
    ))
}

// ---------------------------------------------------------------------------
// 地図（OpenStreetMap）

/// 地図の中心から、埋め込み（OpenStreetMap）に渡す表示範囲 `(左, 下, 右, 上)` を求める。
/// `zoom` が大きいほど狭い範囲になる（幅は約 2 タイル、高さは幅の 6 割に緯度の補正をかけたもの）
fn map_bbox(lat: f64, lon: f64, zoom: u32) -> (f64, f64, f64, f64) {
    let lon_span = 360.0 / f64::from(1u32 << zoom) * 2.0;
    let lat_span = lon_span * 0.6 * lat.to_radians().cos();
    (
        (lon - lon_span / 2.0).max(-180.0),
        (lat - lat_span / 2.0).max(-85.0),
        (lon + lon_span / 2.0).min(180.0),
        (lat + lat_span / 2.0).min(85.0),
    )
}

fn render_openstreetmap(block: &BlockNode) -> Option<String> {
    let lat = number(block, "latitude").filter(|v| (-85.0..=85.0).contains(v))?;
    let lon = number(block, "longitude").filter(|v| (-180.0..=180.0).contains(v))?;
    let zoom = number(block, "zoom")
        .map(|z| z.round().clamp(1.0, 19.0) as u32)
        .unwrap_or(15);
    let (west, south, east, north) = map_bbox(lat, lon, zoom);
    let title = title_or(block, "Map");
    let embed = format!(
        "https://www.openstreetmap.org/export/embed.html?bbox={west:.5},{south:.5},{east:.5},{north:.5}&layer=mapnik&marker={lat:.5},{lon:.5}"
    );
    let large = format!(
        "https://www.openstreetmap.org/?mlat={lat:.5}&mlon={lon:.5}#map={zoom}/{lat:.5}/{lon:.5}"
    );
    Some(format!(
        r#"<figure class="oe-map"><div class="oe-frame oe-ratio"><iframe src="{}" title="{}" loading="lazy" referrerpolicy="strict-origin-when-cross-origin"></iframe></div><figcaption><a href="{}" rel="noopener" target="_blank">{}</a> &copy; OpenStreetMap</figcaption></figure>"#,
        escape(&embed),
        escape(title),
        escape(&large),
        escape(title)
    ))
}

// ---------------------------------------------------------------------------
// CodePen

/// Pen の作者と ID
#[derive(Debug, PartialEq)]
struct Pen {
    user: String,
    id: String,
}

/// `codepen.io/<user>/pen/<id>`（`embed`・`full`・`details`・`debug`・`embed/preview` も）を読む
fn parse_codepen(input: &str) -> Option<Pen> {
    let src = extract_src(input);
    let url = Url::parse(&src)?;
    if !matches!(url.host.as_str(), "codepen.io" | "www.codepen.io") {
        return None;
    }
    let (user, id) = match url.segments.as_slice() {
        [user, "embed", "preview", id, ..] => (*user, *id),
        [user, "pen" | "embed" | "full" | "details" | "debug", id, ..] => (*user, *id),
        _ => return None,
    };
    if !is_id(user, 1, 64) || !is_id(id, 1, 32) || id.contains(['-', '_']) {
        return None;
    }
    Some(Pen {
        user: user.to_string(),
        id: id.to_string(),
    })
}

fn render_codepen(block: &BlockNode) -> Option<String> {
    let pen = parse_codepen(attr(block, "url"))?;
    let tab = match attr(block, "tab") {
        "html" => "html%2Cresult",
        "css" => "css%2Cresult",
        "js" => "js%2Cresult",
        _ => "result",
    };
    let preview = if flag(block, "click_to_load") {
        "preview/"
    } else {
        ""
    };
    let src = format!(
        "https://codepen.io/{}/embed/{preview}{}?default-tab={tab}",
        pen.user, pen.id
    );
    Some(format!(
        r#"<div class="oe-frame oe-codepen"><iframe src="{}" title="{}" height="{}" loading="lazy" allow="clipboard-write" allowfullscreen></iframe></div>"#,
        escape(&src),
        escape(title_or(block, "CodePen")),
        height(block, 400, 200, 1200)
    ))
}

// ---------------------------------------------------------------------------
// Google マップ

/// 「地図を埋め込む」の URL（`www.google.com/maps/embed?pb=...`）から `pb` を取り出す
fn parse_google_map(input: &str) -> Option<String> {
    let src = extract_src(input);
    let url = Url::parse(&src)?;
    if !matches!(url.host.as_str(), "www.google.com" | "google.com")
        || url.segments.as_slice() != ["maps", "embed"]
    {
        return None;
    }
    let pb = url.param("pb")?;
    let ok = (1..=4096).contains(&pb.len())
        && pb.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b'!' | b'.' | b'_' | b'-' | b'~' | b'%' | b'*' | b'+' | b':' | b','
                )
        });
    ok.then(|| pb.to_string())
}

fn render_google_map(block: &BlockNode) -> Option<String> {
    let pb = parse_google_map(attr(block, "embed"))?;
    let aspect = match attr(block, "aspect") {
        "4x3" => " oe-map-4x3",
        "1x1" => " oe-map-1x1",
        _ => "",
    };
    let title = attr(block, "title");
    let caption = if title.is_empty() {
        String::new()
    } else {
        format!("<figcaption>{}</figcaption>", escape(title))
    };
    Some(format!(
        r#"<figure class="oe-map{aspect}"><div class="oe-frame oe-ratio"><iframe src="https://www.google.com/maps/embed?pb={}" title="{}" loading="lazy" referrerpolicy="no-referrer-when-downgrade" allowfullscreen></iframe></div>{caption}</figure>"#,
        escape(&pb),
        escape(title_or(block, "Google Maps"))
    ))
}

// ---------------------------------------------------------------------------
// Google カレンダー

/// 1 つの埋め込みに並べられるカレンダーの数
const MAX_CALENDARS: usize = 10;

/// カレンダー ID（`xxx@gmail.com`・`xxx@group.calendar.google.com`・`ja.japanese#holiday@...`）か
fn is_calendar_id(id: &str) -> bool {
    (3..=254).contains(&id.len())
        && id.matches('@').count() == 1
        && !id.starts_with('@')
        && !id.ends_with('@')
        && id.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'%' | b'+' | b'-' | b'@' | b'#')
        })
}

/// タイムゾーン（`Asia/Tokyo` など）か
fn is_timezone(tz: &str) -> bool {
    (1..=64).contains(&tz.len())
        && tz
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'_' | b'+' | b'-'))
}

/// 埋め込むカレンダー
#[derive(Debug, PartialEq)]
struct Calendar {
    /// カレンダー ID（デコード済み）
    ids: Vec<String>,
    /// 貼られた URL にあったタイムゾーン
    timezone: Option<String>,
}

/// 埋め込みの HTML・URL（`calendar.google.com/calendar/embed?src=...`）か、カレンダー ID を読む
fn parse_google_calendar(input: &str) -> Option<Calendar> {
    let src = extract_src(input);
    if !src.contains("://") {
        return is_calendar_id(&src).then(|| Calendar {
            ids: vec![src],
            timezone: None,
        });
    }
    let url = Url::parse(&src)?;
    if url.host != "calendar.google.com" {
        return None;
    }
    // `/calendar/embed` と、Google Workspace の `/calendar/b/0/embed`・`/calendar/u/0/embed`
    let embed = match url.segments.as_slice() {
        ["calendar", "embed"] => true,
        ["calendar", "b" | "u", n, "embed"] => n.bytes().all(|b| b.is_ascii_digit()),
        _ => false,
    };
    if !embed {
        return None;
    }
    let ids: Vec<String> = url
        .params("src")
        .filter_map(percent_decode)
        .filter(|id| is_calendar_id(id))
        .take(MAX_CALENDARS)
        .collect();
    if ids.is_empty() {
        return None;
    }
    let timezone = url
        .param("ctz")
        .and_then(percent_decode)
        .filter(|tz| is_timezone(tz));
    Some(Calendar { ids, timezone })
}

fn render_google_calendar(block: &BlockNode) -> Option<String> {
    let calendar = parse_google_calendar(attr(block, "calendar"))?;
    let mut src = String::from("https://calendar.google.com/calendar/embed?");
    let sources: Vec<String> = calendar
        .ids
        .iter()
        .map(|id| format!("src={}", percent_encode(id)))
        .collect();
    src.push_str(&sources.join("&"));
    let timezone = Some(attr(block, "timezone"))
        .filter(|tz| is_timezone(tz))
        .map(str::to_string)
        .or(calendar.timezone);
    if let Some(tz) = timezone {
        src.push_str(&format!("&ctz={}", percent_encode(&tz)));
    }
    match attr(block, "mode") {
        "week" => src.push_str("&mode=WEEK"),
        "agenda" => src.push_str("&mode=AGENDA"),
        _ => {}
    }
    Some(format!(
        r#"<div class="oe-frame oe-calendar"><iframe src="{}" title="{}" height="{}" loading="lazy"></iframe></div>"#,
        escape(&src),
        escape(title_or(block, "Google Calendar")),
        height(block, 600, 300, 1200)
    ))
}

// ---------------------------------------------------------------------------
// Google フォーム

/// フォームの ID。`d/e/<id>`（公開用の ID）か `d/<id>`（編集用の ID）
#[derive(Debug, PartialEq)]
enum Form {
    Published(String),
    Document(String),
}

/// `docs.google.com/forms/d/e/<id>/viewform`（`/forms/u/0/...`・`/edit` も）か、埋め込みの HTML を読む。
/// 短縮 URL（`forms.gle`）は取得しないと ID が分からないので読まない
fn parse_google_form(input: &str) -> Option<Form> {
    let src = extract_src(input);
    let url = Url::parse(&src)?;
    if url.host != "docs.google.com" {
        return None;
    }
    let segments = match url.segments.as_slice() {
        ["forms", "u", n, rest @ ..] if n.bytes().all(|b| b.is_ascii_digit()) => rest,
        ["forms", rest @ ..] => rest,
        _ => return None,
    };
    let form = match segments {
        ["d", "e", id, ..] => Form::Published(id.to_string()),
        ["d", id, ..] => Form::Document(id.to_string()),
        _ => return None,
    };
    let (Form::Published(id) | Form::Document(id)) = &form;
    is_id(id, 10, 200).then_some(form)
}

fn render_google_form(block: &BlockNode) -> Option<String> {
    let src = match parse_google_form(attr(block, "url"))? {
        Form::Published(id) => {
            format!("https://docs.google.com/forms/d/e/{id}/viewform?embedded=true")
        }
        Form::Document(id) => {
            format!("https://docs.google.com/forms/d/{id}/viewform?embedded=true")
        }
    };
    Some(format!(
        r#"<div class="oe-frame oe-form"><iframe src="{}" title="{}" height="{}" loading="lazy"></iframe></div>"#,
        escape(&src),
        escape(title_or(block, "Google Forms")),
        height(block, 800, 300, 3000)
    ))
}

impl Plugin for EmbedsPlugin {
    fn render_blocks(&mut self, blocks: Vec<BlockNode>) -> Result<Vec<Option<String>>> {
        let mut styled = false;
        Ok(blocks
            .iter()
            .map(|block| {
                let html = match block.name.as_str() {
                    "official-embeds/video" => render_video(block),
                    "official-embeds/openstreetmap" => render_openstreetmap(block),
                    "official-embeds/codepen" => render_codepen(block),
                    "official-embeds/google-map" => render_google_map(block),
                    "official-embeds/google-calendar" => render_google_calendar(block),
                    "official-embeds/google-form" => render_google_form(block),
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

aegis_plugin_sdk::register_plugin!(EmbedsPlugin);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    fn block(name: &str, attrs: &[(&str, serde_json::Value)]) -> BlockNode {
        BlockNode {
            name: format!("official-embeds/{}", name),
            attributes: attrs
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect::<HashMap<_, _>>(),
            text: None,
            children: Vec::new(),
        }
    }

    fn render(blocks: Vec<BlockNode>) -> Vec<Option<String>> {
        EmbedsPlugin.render_blocks(blocks).unwrap()
    }

    /// HTML の中の iframe の `src` をすべて取り出す
    fn frame_srcs(html: &str) -> Vec<String> {
        html.split("<iframe src=\"")
            .skip(1)
            .map(|rest| rest.split('"').next().unwrap().replace("&amp;", "&"))
            .collect()
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
    fn renders_videos_with_the_styles_once() {
        let html = render(vec![
            block(
                "video",
                &[
                    ("url", json!("https://youtu.be/dQw4w9WgXcQ?t=42")),
                    ("title", json!("Demo \"1\"")),
                ],
            ),
            block(
                "video",
                &[
                    ("url", json!("https://vimeo.com/76979871")),
                    ("aspect", json!("9x16")),
                ],
            ),
            block("video", &[("url", json!("https://example.com/video.mp4"))]),
        ]);
        let first = html[0].as_deref().unwrap();
        assert!(first.starts_with("<style>"));
        assert!(first.contains(r#"<div class="oe-frame oe-ratio oe-video oe-video-16x9"><iframe src="https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ?start=42" title="Demo &quot;1&quot;" loading="lazy""#), "{}", first);
        let second = html[1].as_deref().unwrap();
        assert!(!second.contains("<style>"));
        assert!(second.contains(r#"oe-video-9x16"><iframe src="https://player.vimeo.com/video/76979871?dnt=1" title="Vimeo""#), "{}", second);
        assert_eq!(html[2], None);
    }

    #[test]
    fn renders_openstreetmap_with_a_bounding_box_around_the_marker() {
        let map = |lat: serde_json::Value, lon: serde_json::Value, zoom: serde_json::Value| {
            block(
                "openstreetmap",
                &[
                    ("latitude", lat),
                    ("longitude", lon),
                    ("zoom", zoom),
                    ("title", json!("Tokyo & co")),
                ],
            )
        };
        let html = render(vec![
            map(json!(35.68), json!("139.76"), json!(15)),
            map(json!(95.0), json!(0), json!(15)),
            map(json!(0), json!(200), json!(15)),
            block("openstreetmap", &[]),
        ]);
        let ok = html[0].as_deref().unwrap();
        assert!(ok.contains("https://www.openstreetmap.org/export/embed.html?bbox=139."));
        assert!(ok.contains("&amp;marker=35.68000,139.76000"));
        assert!(ok.contains("title=\"Tokyo &amp; co\""));
        assert!(ok.contains("#map=15/35.68000/139.76000"));
        assert_eq!(&html[1..], &[None, None, None]);
        // 拡大するほど範囲は狭い
        let (w1, _, e1, _) = map_bbox(35.0, 139.0, 10);
        let (w2, _, e2, _) = map_bbox(35.0, 139.0, 16);
        assert!(e1 - w1 > e2 - w2);
        // 範囲は地図の限界を超えない
        let (_, s, _, n) = map_bbox(84.9, 0.0, 1);
        assert!(s >= -85.0 && n <= 85.0);
    }

    #[test]
    fn reads_codepen_urls_and_embed_codes() {
        let pen = |user: &str, id: &str| {
            Some(Pen {
                user: user.to_string(),
                id: id.to_string(),
            })
        };
        assert_eq!(
            parse_codepen("https://codepen.io/team-a/pen/abcXYZ1"),
            pen("team-a", "abcXYZ1")
        );
        assert_eq!(
            parse_codepen("https://codepen.io/alice/full/QWErty?editors=1100#x"),
            pen("alice", "QWErty")
        );
        assert_eq!(
            parse_codepen("https://codepen.io/alice/embed/preview/QWErty"),
            pen("alice", "QWErty")
        );
        // CodePen の埋め込みのコード（iframe の形）
        assert_eq!(
            parse_codepen(
                r#"<iframe height="300" style="width: 100%;" scrolling="no" title="t" src="https://codepen.io/alice/embed/QWErty?default-tab=html%2Cresult" frameborder="no" loading="lazy" allowtransparency="true" allowfullscreen="true"></iframe>"#
            ),
            pen("alice", "QWErty")
        );
        for bad in [
            "https://codepen.io/alice",
            "https://codepen.io/alice/pen/",
            "https://codepen.io/alice/pen/a-b",
            "https://codepen.io/al%22ice/pen/abc",
            "https://codepen.io.evil.example/alice/pen/abc",
            "https://evil.example/codepen.io/alice/pen/abc",
            "javascript:alert(1)",
        ] {
            assert_eq!(parse_codepen(bad), None, "{}", bad);
        }
    }

    #[test]
    fn renders_codepen_with_the_chosen_tab_and_height() {
        let html = render(vec![
            block(
                "codepen",
                &[
                    ("url", json!("https://codepen.io/alice/pen/QWErty")),
                    ("tab", json!("css")),
                    ("height", json!(5000)),
                    ("click_to_load", json!(true)),
                ],
            ),
            block(
                "codepen",
                &[("url", json!("https://codepen.io/alice/pen/QWErty"))],
            ),
        ]);
        let first = html[0].as_deref().unwrap();
        assert!(first.contains(r#"<iframe src="https://codepen.io/alice/embed/preview/QWErty?default-tab=css%2Cresult" title="CodePen" height="1200""#), "{}", first);
        let second = html[1].as_deref().unwrap();
        assert!(second.contains(r#"src="https://codepen.io/alice/embed/QWErty?default-tab=result" title="CodePen" height="400""#), "{}", second);
    }

    #[test]
    fn reads_google_map_embed_codes() {
        let pb = "!1m18!1m12!1m3!1d3240.8!2d139.76!3d35.68!2m3!1f0!2f0!3f0!3m2!1i1024!2i768!4f13.1!3m3!1m2!1s0x60188bfbd89f700b%3A0x277c49ba34ed38!2z5p2x5Lqs6aeF!5e0!3m2!1sja!2sjp!4v1700000000000!5m2!1sja!2sjp";
        let code = format!(
            r#"<iframe src="https://www.google.com/maps/embed?pb={pb}" width="600" height="450" style="border:0;" allowfullscreen="" loading="lazy" referrerpolicy="no-referrer-when-downgrade"></iframe>"#
        );
        assert_eq!(parse_google_map(&code).as_deref(), Some(pb));
        assert_eq!(
            parse_google_map(&format!("https://www.google.com/maps/embed?pb={pb}")).as_deref(),
            Some(pb)
        );
        for bad in [
            "https://www.google.com/maps/place/Tokyo+Station".to_string(),
            "https://www.google.com/maps/embed?q=tokyo".to_string(),
            format!("https://www.google.com/search/embed?pb={pb}"),
            format!("https://www.google.com.evil.example/maps/embed?pb={pb}"),
            "https://www.google.com/maps/embed?pb=!1m2\"onload=x".to_string(),
            "https://www.google.com/maps/embed?pb=<x>".to_string(),
        ] {
            assert_eq!(parse_google_map(&bad), None, "{}", bad);
        }
        let html = render(vec![block(
            "google-map",
            &[
                ("embed", json!(code)),
                ("aspect", json!("1x1")),
                ("title", json!("Tokyo <Station>")),
            ],
        )]);
        let html = html[0].as_deref().unwrap();
        assert!(html.contains(r#"<figure class="oe-map oe-map-1x1">"#));
        assert!(html.contains(&format!(
            r#"<iframe src="https://www.google.com/maps/embed?pb={pb}" title="Tokyo &lt;Station&gt;""#
        )));
        assert!(html.contains("<figcaption>Tokyo &lt;Station&gt;</figcaption>"));
    }

    #[test]
    fn reads_google_calendars_from_codes_urls_and_ids() {
        let code = r#"<iframe src="https://calendar.google.com/calendar/embed?src=alice%40example.com&amp;src=ja.japanese%23holiday%40group.v.calendar.google.com&amp;ctz=Asia%2FTokyo" style="border: 0" width="800" height="600" frameborder="0" scrolling="no"></iframe>"#;
        assert_eq!(
            parse_google_calendar(code),
            Some(Calendar {
                ids: vec![
                    "alice@example.com".to_string(),
                    "ja.japanese#holiday@group.v.calendar.google.com".to_string()
                ],
                timezone: Some("Asia/Tokyo".to_string()),
            })
        );
        assert_eq!(
            parse_google_calendar("c_abc123@group.calendar.google.com"),
            Some(Calendar {
                ids: vec!["c_abc123@group.calendar.google.com".to_string()],
                timezone: None,
            })
        );
        assert!(parse_google_calendar(
            "https://calendar.google.com/calendar/b/1/embed?src=bob%40example.com"
        )
        .is_some());
        for bad in [
            "alice",
            "a@b@c",
            "alice@example.com\" onload=\"x",
            "https://calendar.google.com/calendar/u/0/r",
            "https://calendar.google.com/calendar/embed?src=%3Cx%3E%40a",
            "https://calendar.google.com.evil.example/calendar/embed?src=a%40b.c",
            "https://evil.example/calendar/embed?src=a%40b.c",
        ] {
            assert_eq!(parse_google_calendar(bad), None, "{}", bad);
        }

        let html = render(vec![
            block(
                "google-calendar",
                &[
                    ("calendar", json!(code)),
                    ("mode", json!("agenda")),
                    ("height", json!("450")),
                ],
            ),
            block(
                "google-calendar",
                &[
                    ("calendar", json!("alice@example.com")),
                    ("timezone", json!("Europe/London")),
                ],
            ),
        ]);
        let srcs: Vec<String> = html
            .iter()
            .flat_map(|h| frame_srcs(h.as_deref().unwrap()))
            .collect();
        assert_eq!(
            srcs,
            [
                "https://calendar.google.com/calendar/embed?src=alice%40example.com&src=ja.japanese%23holiday%40group.v.calendar.google.com&ctz=Asia%2FTokyo&mode=AGENDA",
                "https://calendar.google.com/calendar/embed?src=alice%40example.com&ctz=Europe%2FLondon",
            ]
        );
        assert!(html[0].as_deref().unwrap().contains(r#"height="450""#));
        assert!(html[1].as_deref().unwrap().contains(r#"height="600""#));
    }

    #[test]
    fn reads_google_form_urls() {
        let id = "1FAIpQLSf_abcdefghijklmnop-1234567890";
        assert_eq!(
            parse_google_form(&format!(
                "https://docs.google.com/forms/d/e/{id}/viewform?usp=sf_link"
            )),
            Some(Form::Published(id.to_string()))
        );
        assert_eq!(
            parse_google_form(&format!(
                r#"<iframe src="https://docs.google.com/forms/d/e/{id}/viewform?embedded=true" width="640" height="800" frameborder="0" marginheight="0" marginwidth="0">読み込んでいます…</iframe>"#
            )),
            Some(Form::Published(id.to_string()))
        );
        assert_eq!(
            parse_google_form("https://docs.google.com/forms/u/0/d/1AbCdEfGhIjKlMnOp/edit"),
            Some(Form::Document("1AbCdEfGhIjKlMnOp".to_string()))
        );
        for bad in [
            "https://forms.gle/abcdef12345",
            "https://docs.google.com/document/d/1AbCdEfGhIjKlMnOp/edit",
            "https://docs.google.com/forms/d/e/short/viewform",
            "https://docs.google.com/forms/d/e/1AbCdEfGhIjK%22lMnOp/viewform",
            "https://docs.google.com.evil.example/forms/d/e/1AbCdEfGhIjKlMnOp/viewform",
        ] {
            assert_eq!(parse_google_form(bad), None, "{}", bad);
        }
        let html = render(vec![block(
            "google-form",
            &[(
                "url",
                json!(format!("https://docs.google.com/forms/d/e/{id}/viewform")),
            )],
        )]);
        let html = html[0].as_deref().unwrap();
        assert_eq!(
            frame_srcs(html),
            [format!(
                "https://docs.google.com/forms/d/e/{id}/viewform?embedded=true"
            )]
        );
        assert!(html.contains(r#"title="Google Forms" height="800""#));
    }

    #[test]
    fn skips_unknown_or_incomplete_blocks() {
        let html = render(vec![
            block("unknown", &[]),
            block("video", &[]),
            block("codepen", &[("url", json!(""))]),
            block("google-map", &[("embed", json!("<iframe src=\"x\">"))]),
            block("google-calendar", &[("calendar", json!(42))]),
            block("google-form", &[]),
        ]);
        assert!(html.iter().all(Option::is_none));
    }

    /// iframe のホストは、すべてマニフェストの `embed.frame_domains` にあること（無いとサニタイズで消える）
    #[test]
    fn frames_only_use_the_declared_hosts() {
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../plugin.manifest.json")).unwrap();
        let declared: Vec<String> = manifest["capabilities"]["embed"]["frame_domains"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d.as_str().unwrap().to_string())
            .collect();
        let html = render(vec![
            block("video", &[("url", json!("https://youtu.be/dQw4w9WgXcQ"))]),
            block("video", &[("url", json!("https://vimeo.com/76979871"))]),
            block(
                "openstreetmap",
                &[("latitude", json!(35)), ("longitude", json!(139))],
            ),
            block(
                "codepen",
                &[("url", json!("https://codepen.io/alice/pen/QWErty"))],
            ),
            block(
                "google-map",
                &[("embed", json!("https://www.google.com/maps/embed?pb=!1m2"))],
            ),
            block("google-calendar", &[("calendar", json!("a@example.com"))]),
            block(
                "google-form",
                &[(
                    "url",
                    json!("https://docs.google.com/forms/d/e/1AbCdEfGhIjKlMnOp/viewform"),
                )],
            ),
        ]);
        let mut used = Vec::new();
        for h in &html {
            for src in frame_srcs(h.as_deref().unwrap()) {
                let host = src["https://".len()..]
                    .split('/')
                    .next()
                    .unwrap()
                    .to_string();
                assert!(declared.contains(&host), "{} is not declared", host);
                used.push(host);
            }
        }
        // 宣言したホストは、どれかのブロックが使っていること（使わないホストを CSP に足さない）
        for host in &declared {
            assert!(used.contains(host), "{} is declared but unused", host);
        }
        // マニフェストのブロックと、このプラグインが描画するブロックが一致していること
        let names: Vec<&str> = manifest["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "video",
                "openstreetmap",
                "codepen",
                "google-map",
                "google-calendar",
                "google-form"
            ]
        );
    }
}
