//! 公式プラグイン `official-blocks`: 本文に置けるボタンとリンクカードのブロック（`render-blocks`）。
//! カスタムブロックを作るときのお手本でもある。
//!
//! - `official-blocks/button`: `label`・`url`・`style`（`primary` / `secondary`）・`align`（`left` / `center`）
//! - `official-blocks/link-card`: `url`・`title`・`description`・`image`
//! - `official-blocks/video`: `url`（YouTube・Vimeo）・`title`・`aspect`（`16x9` / `4x3` / `9x16`）。
//!   プレーヤーの URL はこのプラグインが動画の ID から組み立て（`www.youtube-nocookie.com`・
//!   `player.vimeo.com`、マニフェストの `capabilities.embed.frame_domains`）、利用者の HTML は使わない
//!
//! - `official-blocks/audio`: `url`・`title`。ブラウザ標準のプレーヤー（読み込みは再生するまで待つ）
//! - `official-blocks/file`: `url`・`title`・`description`・`size`（表示用の文字列）。ダウンロード用のリンク
//! - `official-blocks/map`: `latitude`・`longitude`・`zoom`・`title`。OpenStreetMap の埋め込み
//!   （`www.openstreetmap.org`、マニフェストの `capabilities.embed.frame_domains`）。住所からの検索はしない
//! - `official-blocks/gallery`: `images`（`list`: `image`・`caption`。24 枚まで）・`columns`（2〜4）
//! - `official-blocks/faq`: `items`（`list`: `question`・`answer`）。`<details>` で開閉し、schema.org の
//!   FAQPage を microdata で付ける（`<script>` の JSON-LD はサニタイズで消える）
//! - `official-blocks/steps`: `items`（`list`: `title`・`description`）。番号つきの手順
//! - `official-blocks/columns`: `items`（`list`: `image`・`title`・`text`。4 列まで）。同じ幅の列を並べ、
//!   狭い画面では縦に積む
//!
//! gallery・faq・steps は、複数行の文字列で保存した古い形（1 行に 1 枚 / 空行で区切った項目）も読む
//! - `official-blocks/media-text`: `image`・`alt`・`title`・`text`・`side`（左 / 右）。画像と文章を並べる。
//!   狭い画面では縦に積む
//! - `official-blocks/figure`: `image`・`alt`・`caption`。画像と、その下のキャプション（`<figcaption>`）。
//!   core の画像（GitHub の記法の範囲）にはキャプションが無いため、このプラグインで足す
//! - `official-blocks/timeline`: `items`（`list`: `date`・`title`・`description`。50 件まで）。縦線でつなぐ年表
//! - `official-blocks/chat`: `items`（`list`: `name`・`avatar`・`side`・`text`。50 件まで）。吹き出しの会話
//! - `official-blocks/tabs`: `items`（`list`: `label`・`content`・`format`（文章 / コード）。8 つまで）。
//!   ラジオボタンと CSS だけで切り替える（JavaScript なし）。印刷では全部のタブを順に出す
//! - `official-blocks/stats`: `items`（`list`: `value`・`unit`・`label`。6 つまで）。大きな数字を並べる
//! - `official-blocks/definition-list`: `items`（`list`: `term`・`description`。100 件まで）。`<dl>`
//! - `official-blocks/rating`: `name`・`item_type`・`rating`（1〜5、0.5 刻み）・`summary`・`author`・
//!   `scores`（`list`: `label`・`score`。10 件まで）。星の評価で、schema.org の Review を microdata で付ける
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
:where(.ob-audio){margin:1.5rem 0}\
:where(.ob-audio) figcaption{font-size:.9em;opacity:.75;margin-bottom:.4rem}\
:where(.ob-audio) audio{display:block;width:100%}\
:where(.ob-file){display:flex;gap:.9rem;align-items:center;margin:1.5rem 0;padding:.8rem 1rem;border:1px solid rgba(127,127,127,.3);border-radius:10px}\
.aegis-plugin-block .ob-file{color:inherit;text-decoration:none}\
:where(.ob-file):hover{border-color:var(--accent,#6366f1)}\
:where(.ob-file-icon){flex:0 0 auto;font-size:1.4em;color:var(--accent,#6366f1)}\
:where(.ob-file-body){flex:1;min-width:0}\
:where(.ob-file-title){display:block;font-weight:700;overflow-wrap:anywhere}\
:where(.ob-file-meta){display:block;opacity:.7;font-size:.85em}\
:where(.ob-map){margin:1.5rem 0}\
:where(.ob-map-frame){width:100%;aspect-ratio:16/10;border-radius:10px;overflow:hidden;background:rgba(127,127,127,.1)}\
:where(.ob-map-frame) iframe{display:block;width:100%;height:100%;border:0}\
:where(.ob-map) figcaption{font-size:.85em;opacity:.75;margin-top:.4rem}\
:where(.ob-gallery){display:grid;gap:.6rem;margin:1.5rem 0;grid-template-columns:repeat(var(--ob-cols,3),minmax(0,1fr))}\
:where(.ob-gallery-2){--ob-cols:2}:where(.ob-gallery-4){--ob-cols:4}\
@media(max-width:640px){:where(.ob-gallery){--ob-cols:2}}\
:where(.ob-gallery) figure{margin:0}\
:where(.ob-gallery) img{display:block;width:100%;aspect-ratio:4/3;object-fit:cover;border-radius:8px;background:rgba(127,127,127,.1)}\
:where(.ob-gallery) figcaption{font-size:.8em;opacity:.75;margin-top:.3rem}\
:where(.ob-faq){margin:1.5rem 0;border-top:1px solid rgba(127,127,127,.3)}\
:where(.ob-faq) details{border-bottom:1px solid rgba(127,127,127,.3);padding:.75rem .25rem}\
:where(.ob-faq) summary{cursor:pointer;font-weight:600}\
:where(.ob-faq-answer){margin-top:.6rem;opacity:.9}\
:where(.ob-faq-answer) p{margin:.4rem 0}\
:where(.ob-steps){list-style:none;counter-reset:ob-step;margin:1.5rem 0;padding:0}\
:where(.ob-steps) li{counter-increment:ob-step;position:relative;margin:0;padding:0 0 1.25rem 3rem}\
:where(.ob-steps) li::before{content:counter(ob-step);position:absolute;left:0;top:-.1rem;width:2.1rem;height:2.1rem;border-radius:50%;display:flex;align-items:center;justify-content:center;font-weight:700;background:var(--accent,#6366f1);color:var(--accent-contrast,#fff)}\
:where(.ob-steps) li::after{content:'';position:absolute;left:1.05rem;top:2.2rem;bottom:.15rem;width:2px;background:rgba(127,127,127,.3)}\
:where(.ob-steps) li:last-child::after{display:none}\
:where(.ob-step-title){display:block;font-weight:700}\
:where(.ob-step-body){display:block;opacity:.85;margin-top:.2rem}\
:where(.ob-columns){display:grid;gap:1.5rem;margin:1.5rem 0;grid-template-columns:repeat(auto-fit,minmax(220px,1fr))}\
:where(.ob-column){min-width:0}\
:where(.ob-column) img{display:block;width:100%;border-radius:10px;margin-bottom:.6rem}\
:where(.ob-column-title){display:block;font-weight:700;font-size:1.05em;margin-bottom:.25rem}\
:where(.ob-column) p{margin:.4rem 0}\
:where(.ob-media-text){display:flex;flex-wrap:wrap;gap:1.25rem;align-items:center;margin:1.5rem 0}\
:where(.ob-media-text-media){flex:1 1 240px;min-width:0}\
:where(.ob-media-text-media) img{display:block;width:100%;border-radius:10px}\
:where(.ob-media-text-body){flex:2 1 280px;min-width:0}\
:where(.ob-media-text-title){display:block;font-weight:700;font-size:1.15em;margin-bottom:.3rem}\
:where(.ob-media-text-body) p{margin:.4rem 0}\
@media(min-width:641px){:where(.ob-media-text-right) .ob-media-text-media{order:2}}\
:where(.ob-figure){margin:1.5rem 0}\
:where(.ob-figure) img{display:block;max-width:100%;height:auto;margin:0 auto;border-radius:8px}\
:where(.ob-figure) figcaption{font-size:.85em;opacity:.75;margin-top:.5rem;text-align:center}\
:where(.ob-timeline){list-style:none;margin:1.5rem 0;padding:0 0 0 1.5rem;border-left:2px solid rgba(127,127,127,.3)}\
:where(.ob-timeline) li{position:relative;margin:0;padding:0 0 1.25rem}\
:where(.ob-timeline) li::before{content:'';position:absolute;left:calc(-1.5rem - 6px);top:.35rem;width:10px;height:10px;border-radius:50%;background:var(--accent,#6366f1)}\
:where(.ob-tl-date){display:block;font-size:.85em;font-weight:600;color:var(--accent,#6366f1)}\
:where(.ob-tl-title){display:block;font-weight:700}\
:where(.ob-tl-body){display:block;opacity:.85;margin-top:.2rem}\
:where(.ob-chat){margin:1.5rem 0;display:flex;flex-direction:column;gap:1rem}\
:where(.ob-chat-item){display:flex;gap:.75rem;align-items:flex-start}\
:where(.ob-chat-right){flex-direction:row-reverse}\
:where(.ob-chat-who){flex:0 0 4.5rem;text-align:center;font-size:.75em;overflow-wrap:anywhere}\
:where(.ob-chat-who) img{display:block;width:3rem;height:3rem;margin:0 auto .2rem;border-radius:50%;object-fit:cover}\
:where(.ob-chat-bubble){position:relative;max-width:min(32rem,calc(100% - 5.25rem));padding:.7rem 1rem;border-radius:14px;background:rgba(127,127,127,.14)}\
:where(.ob-chat-bubble) p{margin:.3rem 0}\
:where(.ob-chat-right) .ob-chat-bubble{background:color-mix(in srgb,var(--accent,#6366f1) 18%,transparent)}\
:where(.ob-tabs){position:relative;margin:1.5rem 0}\
:where(.ob-tabs)>input{position:absolute;opacity:0;width:1px;height:1px;margin:0;pointer-events:none}\
:where(.ob-tabs-bar){display:flex;flex-wrap:wrap;gap:.25rem;border-bottom:1px solid rgba(127,127,127,.3)}\
:where(.ob-tabs-bar) label{cursor:pointer;padding:.5rem .9rem;border-bottom:3px solid transparent;margin-bottom:-1px;opacity:.7}\
:where(.ob-tab-panel){display:none;padding:.9rem .25rem}\
:where(.ob-tab-panel) p{margin:.4rem 0}\
:where(.ob-tab-panel) pre{margin:0;overflow-x:auto}\
:where(.ob-tab-title){display:none;font-weight:700;margin:0}\
:where(.ob-tabs)>input:nth-of-type(1):checked~.ob-tabs-bar label:nth-child(1){border-bottom-color:var(--accent,#6366f1);font-weight:700;opacity:1}\
:where(.ob-tabs)>input:nth-of-type(1):checked~.ob-tabs-panels .ob-tab-panel:nth-child(1){display:block}\
:where(.ob-tabs)>input:nth-of-type(1):focus-visible~.ob-tabs-bar label:nth-child(1){outline:2px solid var(--accent,#6366f1)}\
:where(.ob-tabs)>input:nth-of-type(2):checked~.ob-tabs-bar label:nth-child(2){border-bottom-color:var(--accent,#6366f1);font-weight:700;opacity:1}\
:where(.ob-tabs)>input:nth-of-type(2):checked~.ob-tabs-panels .ob-tab-panel:nth-child(2){display:block}\
:where(.ob-tabs)>input:nth-of-type(2):focus-visible~.ob-tabs-bar label:nth-child(2){outline:2px solid var(--accent,#6366f1)}\
:where(.ob-tabs)>input:nth-of-type(3):checked~.ob-tabs-bar label:nth-child(3){border-bottom-color:var(--accent,#6366f1);font-weight:700;opacity:1}\
:where(.ob-tabs)>input:nth-of-type(3):checked~.ob-tabs-panels .ob-tab-panel:nth-child(3){display:block}\
:where(.ob-tabs)>input:nth-of-type(3):focus-visible~.ob-tabs-bar label:nth-child(3){outline:2px solid var(--accent,#6366f1)}\
:where(.ob-tabs)>input:nth-of-type(4):checked~.ob-tabs-bar label:nth-child(4){border-bottom-color:var(--accent,#6366f1);font-weight:700;opacity:1}\
:where(.ob-tabs)>input:nth-of-type(4):checked~.ob-tabs-panels .ob-tab-panel:nth-child(4){display:block}\
:where(.ob-tabs)>input:nth-of-type(4):focus-visible~.ob-tabs-bar label:nth-child(4){outline:2px solid var(--accent,#6366f1)}\
:where(.ob-tabs)>input:nth-of-type(5):checked~.ob-tabs-bar label:nth-child(5){border-bottom-color:var(--accent,#6366f1);font-weight:700;opacity:1}\
:where(.ob-tabs)>input:nth-of-type(5):checked~.ob-tabs-panels .ob-tab-panel:nth-child(5){display:block}\
:where(.ob-tabs)>input:nth-of-type(5):focus-visible~.ob-tabs-bar label:nth-child(5){outline:2px solid var(--accent,#6366f1)}\
:where(.ob-tabs)>input:nth-of-type(6):checked~.ob-tabs-bar label:nth-child(6){border-bottom-color:var(--accent,#6366f1);font-weight:700;opacity:1}\
:where(.ob-tabs)>input:nth-of-type(6):checked~.ob-tabs-panels .ob-tab-panel:nth-child(6){display:block}\
:where(.ob-tabs)>input:nth-of-type(6):focus-visible~.ob-tabs-bar label:nth-child(6){outline:2px solid var(--accent,#6366f1)}\
:where(.ob-tabs)>input:nth-of-type(7):checked~.ob-tabs-bar label:nth-child(7){border-bottom-color:var(--accent,#6366f1);font-weight:700;opacity:1}\
:where(.ob-tabs)>input:nth-of-type(7):checked~.ob-tabs-panels .ob-tab-panel:nth-child(7){display:block}\
:where(.ob-tabs)>input:nth-of-type(7):focus-visible~.ob-tabs-bar label:nth-child(7){outline:2px solid var(--accent,#6366f1)}\
:where(.ob-tabs)>input:nth-of-type(8):checked~.ob-tabs-bar label:nth-child(8){border-bottom-color:var(--accent,#6366f1);font-weight:700;opacity:1}\
:where(.ob-tabs)>input:nth-of-type(8):checked~.ob-tabs-panels .ob-tab-panel:nth-child(8){display:block}\
:where(.ob-tabs)>input:nth-of-type(8):focus-visible~.ob-tabs-bar label:nth-child(8){outline:2px solid var(--accent,#6366f1)}\
@media print{.ob-tabs .ob-tab-panel{display:block}.ob-tabs .ob-tabs-bar{display:none}.ob-tabs .ob-tab-title{display:block}}\
:where(.ob-stats){display:grid;gap:1rem;margin:1.5rem 0;grid-template-columns:repeat(auto-fit,minmax(150px,1fr))}\
:where(.ob-stat){padding:1rem;border:1px solid rgba(127,127,127,.3);border-radius:10px;text-align:center}\
:where(.ob-stat-value){display:block;font-size:2em;font-weight:800;line-height:1.2;color:var(--accent,#6366f1)}\
:where(.ob-stat-unit){font-size:.5em;font-weight:600;margin-left:.15em}\
:where(.ob-stat-label){display:block;opacity:.75;font-size:.9em;margin-top:.2rem}\
:where(.ob-dl){margin:1.5rem 0;border-top:1px solid rgba(127,127,127,.3)}\
:where(.ob-dl-row){display:grid;grid-template-columns:minmax(0,1fr);gap:.2rem;padding:.7rem .25rem;border-bottom:1px solid rgba(127,127,127,.3)}\
:where(.ob-dl) dt{font-weight:700;overflow-wrap:anywhere}\
:where(.ob-dl) dd{margin:0;opacity:.9}\
:where(.ob-dl) dd p{margin:.2rem 0}\
:where(.ob-rating){margin:1.5rem 0;padding:1rem 1.25rem;border:1px solid rgba(127,127,127,.3);border-radius:10px}\
:where(.ob-rating-head){display:flex;flex-wrap:wrap;gap:.4rem 1rem;align-items:center;justify-content:space-between}\
:where(.ob-rating-name){font-weight:700;font-size:1.1em;overflow-wrap:anywhere}\
:where(.ob-rating-score){display:inline-flex;align-items:center;gap:.5rem}\
:where(.ob-stars){position:relative;display:inline-block;white-space:nowrap;line-height:1;color:rgba(127,127,127,.35)}\
:where(.ob-stars)::before{content:'★★★★★'}\
:where(.ob-stars-fill){position:absolute;left:0;top:0;overflow:hidden;white-space:nowrap;color:#f5a623}\
:where(.ob-r0) .ob-stars-fill{width:0%}\
:where(.ob-r1) .ob-stars-fill{width:10%}\
:where(.ob-r2) .ob-stars-fill{width:20%}\
:where(.ob-r3) .ob-stars-fill{width:30%}\
:where(.ob-r4) .ob-stars-fill{width:40%}\
:where(.ob-r5) .ob-stars-fill{width:50%}\
:where(.ob-r6) .ob-stars-fill{width:60%}\
:where(.ob-r7) .ob-stars-fill{width:70%}\
:where(.ob-r8) .ob-stars-fill{width:80%}\
:where(.ob-r9) .ob-stars-fill{width:90%}\
:where(.ob-r10) .ob-stars-fill{width:100%}\
:where(.ob-rating-number){font-weight:700}\
:where(.ob-rating-body){margin-top:.6rem}\
:where(.ob-rating-body) p{margin:.4rem 0}\
:where(.ob-rating-author){margin:.4rem 0 0;opacity:.7;font-size:.9em}\
:where(.ob-rating-scores){list-style:none;margin:.8rem 0 0;padding:0;display:grid;gap:.35rem}\
:where(.ob-rating-scores) li{display:flex;align-items:center;gap:.6rem}\
:where(.ob-rating-label){flex:1 1 auto;min-width:0;overflow-wrap:anywhere}\
:where(.ob-rating-value){font-variant-numeric:tabular-nums;opacity:.8;min-width:2em;text-align:right}\
@media(min-width:641px){:where(.ob-dl-row){grid-template-columns:minmax(8rem,30%) minmax(0,1fr);gap:1rem}}\
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

fn render_audio(block: &BlockNode) -> Option<String> {
    let url = safe_url(attr(block, "url"))?;
    let title = attr(block, "title");
    let caption = if title.is_empty() {
        String::new()
    } else {
        format!("<figcaption>{}</figcaption>", escape(title))
    };
    let label = if title.is_empty() {
        String::new()
    } else {
        format!(r#" aria-label="{}""#, escape(title))
    };
    Some(format!(
        r#"<figure class="ob-audio">{caption}<audio controls preload="none"{label} src="{}"></audio></figure>"#,
        escape(url)
    ))
}

fn render_file(block: &BlockNode) -> Option<String> {
    let url = safe_url(attr(block, "url"))?;
    let title = attr(block, "title");
    if title.is_empty() {
        return None;
    }
    let meta: Vec<&str> = [attr(block, "description"), attr(block, "size")]
        .into_iter()
        .filter(|v| !v.is_empty())
        .collect();
    let meta = if meta.is_empty() {
        String::new()
    } else {
        format!(
            r#"<span class="ob-file-meta">{}</span>"#,
            escape(&meta.join(" · "))
        )
    };
    Some(format!(
        r#"<a class="ob-file" href="{}" download><span class="ob-file-icon" aria-hidden="true">&#x2B07;</span><span class="ob-file-body"><span class="ob-file-title">{}</span>{meta}</span></a>"#,
        escape(url),
        escape(title)
    ))
}

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

fn render_map(block: &BlockNode) -> Option<String> {
    let lat = number(block, "latitude").filter(|v| (-85.0..=85.0).contains(v))?;
    let lon = number(block, "longitude").filter(|v| (-180.0..=180.0).contains(v))?;
    let zoom = number(block, "zoom")
        .map(|z| z.round().clamp(1.0, 19.0) as u32)
        .unwrap_or(15);
    let (west, south, east, north) = map_bbox(lat, lon, zoom);
    let title = match attr(block, "title") {
        "" => "Map",
        title => title,
    };
    let embed = format!(
        "https://www.openstreetmap.org/export/embed.html?bbox={west:.5},{south:.5},{east:.5},{north:.5}&layer=mapnik&marker={lat:.5},{lon:.5}"
    );
    let large = format!(
        "https://www.openstreetmap.org/?mlat={lat:.5}&mlon={lon:.5}#map={zoom}/{lat:.5}/{lon:.5}"
    );
    Some(format!(
        r#"<figure class="ob-map"><div class="ob-map-frame"><iframe src="{}" title="{}" loading="lazy" referrerpolicy="strict-origin-when-cross-origin"></iframe></div><figcaption><a href="{}" rel="noopener" target="_blank">{}</a> &copy; OpenStreetMap</figcaption></figure>"#,
        escape(&embed),
        escape(title),
        escape(&large),
        escape(title)
    ))
}

/// 空行で区切った項目に分ける。各項目は「1 行目（見出し）」と「残りの行（本文）」
fn entries(text: &str) -> Vec<(String, Vec<String>)> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in normalized.split('\n').chain(std::iter::once("")) {
        if line.trim().is_empty() {
            if let Some((head, rest)) = current.split_first() {
                out.push((
                    head.trim().to_string(),
                    rest.iter().map(|l| l.trim().to_string()).collect(),
                ));
            }
            current.clear();
        } else {
            current.push(line);
        }
    }
    out
}

/// 本文の行を段落にする（HTML はエスケープする）
fn paragraphs(lines: &[String]) -> String {
    lines
        .iter()
        .map(|l| format!("<p>{}</p>", escape(l)))
        .collect()
}

/// `list` の入力欄の値（項目はオブジェクト。オブジェクト以外の要素は捨てる）
fn list_of<'a>(
    block: &'a BlockNode,
    name: &str,
) -> Vec<&'a serde_json::Map<String, serde_json::Value>> {
    block
        .attributes
        .get(name)
        .and_then(|v| v.as_array())
        .map(|items| items.iter().filter_map(|i| i.as_object()).collect())
        .unwrap_or_default()
}

/// 項目の文字列の値（前後の空白は除く）
fn item_str<'a>(item: &'a serde_json::Map<String, serde_json::Value>, key: &str) -> &'a str {
    item.get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("")
}

/// 複数行の文字列を、空でない行にする
fn text_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// 「見出し + 本文の行」の項目の並び。`list`（`head_key`・`body_key`）の値か、古い形（空行で区切った文字列）
fn head_body_items(
    block: &BlockNode,
    head_key: &str,
    body_key: &str,
) -> Vec<(String, Vec<String>)> {
    match block.attributes.get("items") {
        Some(serde_json::Value::Array(_)) => list_of(block, "items")
            .into_iter()
            .map(|item| {
                (
                    item_str(item, head_key).to_string(),
                    text_lines(item_str(item, body_key)),
                )
            })
            .collect(),
        Some(serde_json::Value::String(text)) => entries(text),
        _ => Vec::new(),
    }
}

/// ギャラリーに置ける画像の最大枚数
const MAX_GALLERY_IMAGES: usize = 24;

/// ギャラリーの `(画像の URL, 説明)`。`list`（`image`・`caption`）の値か、古い形（1 行に `URL | 説明`）
fn gallery_items(block: &BlockNode) -> Vec<(String, String)> {
    match block.attributes.get("images") {
        Some(serde_json::Value::Array(_)) => list_of(block, "images")
            .into_iter()
            .map(|item| {
                (
                    item_str(item, "image").to_string(),
                    item_str(item, "caption").to_string(),
                )
            })
            .collect(),
        Some(serde_json::Value::String(text)) => text
            .lines()
            .map(|line| match line.split_once('|') {
                Some((src, caption)) => (src.trim().to_string(), caption.trim().to_string()),
                None => (line.trim().to_string(), String::new()),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn render_gallery(block: &BlockNode) -> Option<String> {
    let mut figures = String::new();
    let mut count = 0;
    for (src, caption) in gallery_items(block) {
        let Some(src) = safe_url(&src) else { continue };
        count += 1;
        if count > MAX_GALLERY_IMAGES {
            break;
        }
        let caption_html = if caption.is_empty() {
            String::new()
        } else {
            format!("<figcaption>{}</figcaption>", escape(&caption))
        };
        figures.push_str(&format!(
            r#"<figure><img src="{}" alt="{}" loading="lazy">{caption_html}</figure>"#,
            escape(src),
            escape(&caption)
        ));
    }
    if figures.is_empty() {
        return None;
    }
    let columns = match attr(block, "columns") {
        "2" => "2",
        "4" => "4",
        _ => "3",
    };
    Some(format!(
        r#"<div class="ob-gallery ob-gallery-{columns}">{figures}</div>"#
    ))
}

fn render_faq(block: &BlockNode) -> Option<String> {
    let items: String = head_body_items(block, "question", "answer")
        .into_iter()
        .filter(|(question, answer)| !question.is_empty() && !answer.is_empty())
        .map(|(question, answer)| {
            format!(
                r#"<details itemscope itemprop="mainEntity" itemtype="https://schema.org/Question"><summary itemprop="name">{}</summary><div class="ob-faq-answer" itemscope itemprop="acceptedAnswer" itemtype="https://schema.org/Answer"><div itemprop="text">{}</div></div></details>"#,
                escape(&question),
                paragraphs(&answer)
            )
        })
        .collect();
    (!items.is_empty()).then(|| {
        format!(
            r#"<div class="ob-faq" itemscope itemtype="https://schema.org/FAQPage">{items}</div>"#
        )
    })
}

fn render_steps(block: &BlockNode) -> Option<String> {
    let items: String = head_body_items(block, "title", "description")
        .into_iter()
        .filter(|(title, _)| !title.is_empty())
        .map(|(title, body)| {
            let body = if body.is_empty() {
                String::new()
            } else {
                format!(
                    r#"<span class="ob-step-body">{}</span>"#,
                    escape(&body.join(" "))
                )
            };
            format!(
                r#"<li><span class="ob-step-title">{}</span>{body}</li>"#,
                escape(&title)
            )
        })
        .collect();
    (!items.is_empty()).then(|| format!(r#"<ol class="ob-steps">{items}</ol>"#))
}

/// カラムの最大数
const MAX_COLUMNS: usize = 4;

fn render_columns(block: &BlockNode) -> Option<String> {
    let columns: String = list_of(block, "items")
        .into_iter()
        .take(MAX_COLUMNS)
        .filter_map(|item| {
            let title = item_str(item, "title");
            let lines = text_lines(item_str(item, "text"));
            let image = safe_url(item_str(item, "image"));
            if title.is_empty() && lines.is_empty() && image.is_none() {
                return None;
            }
            let image = image
                .map(|src| {
                    format!(
                        r#"<img src="{}" alt="{}" loading="lazy">"#,
                        escape(src),
                        escape(title)
                    )
                })
                .unwrap_or_default();
            let title = if title.is_empty() {
                String::new()
            } else {
                format!(r#"<span class="ob-column-title">{}</span>"#, escape(title))
            };
            Some(format!(
                r#"<div class="ob-column">{image}{title}{}</div>"#,
                paragraphs(&lines)
            ))
        })
        .collect();
    (!columns.is_empty()).then(|| format!(r#"<div class="ob-columns">{columns}</div>"#))
}

fn render_media_text(block: &BlockNode) -> Option<String> {
    let image = safe_url(attr(block, "image"))?;
    let title = attr(block, "title");
    let body = block
        .attributes
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let lines = text_lines(body);
    if title.is_empty() && lines.is_empty() {
        return None;
    }
    let title_html = if title.is_empty() {
        String::new()
    } else {
        format!(
            r#"<span class="ob-media-text-title">{}</span>"#,
            escape(title)
        )
    };
    let side = if attr(block, "side") == "right" {
        " ob-media-text-right"
    } else {
        ""
    };
    Some(format!(
        r#"<div class="ob-media-text{side}"><div class="ob-media-text-media"><img src="{}" alt="{}" loading="lazy"></div><div class="ob-media-text-body">{title_html}{}</div></div>"#,
        escape(image),
        escape(attr(block, "alt")),
        paragraphs(&lines)
    ))
}

fn render_figure(block: &BlockNode) -> Option<String> {
    let image = safe_url(attr(block, "image"))?;
    let caption = attr(block, "caption");
    let caption_html = if caption.is_empty() {
        String::new()
    } else {
        format!("<figcaption>{}</figcaption>", escape(caption))
    };
    Some(format!(
        r#"<figure class="ob-figure"><img src="{}" alt="{}" loading="lazy">{caption_html}</figure>"#,
        escape(image),
        escape(attr(block, "alt"))
    ))
}

/// タイムラインの最大項目数
const MAX_TIMELINE: usize = 50;

fn render_timeline(block: &BlockNode) -> Option<String> {
    let items: String = list_of(block, "items")
        .into_iter()
        .filter(|item| !item_str(item, "date").is_empty() || !item_str(item, "title").is_empty())
        .take(MAX_TIMELINE)
        .map(|item| {
            let part = |class: &str, text: &str| {
                if text.is_empty() {
                    String::new()
                } else {
                    format!(r#"<span class="{class}">{}</span>"#, escape(text))
                }
            };
            let body = text_lines(item_str(item, "description")).join(" ");
            format!(
                "<li>{}{}{}</li>",
                part("ob-tl-date", item_str(item, "date")),
                part("ob-tl-title", item_str(item, "title")),
                part("ob-tl-body", &body)
            )
        })
        .collect();
    (!items.is_empty()).then(|| format!(r#"<ol class="ob-timeline">{items}</ol>"#))
}

/// 会話の最大項目数
const MAX_CHAT: usize = 50;

fn render_chat(block: &BlockNode) -> Option<String> {
    let items: String = list_of(block, "items")
        .into_iter()
        .filter_map(|item| {
            let lines = text_lines(item_str(item, "text"));
            if lines.is_empty() {
                return None;
            }
            let name = item_str(item, "name");
            let avatar = safe_url(item_str(item, "avatar"))
                .map(|src| format!(r#"<img src="{}" alt="" loading="lazy">"#, escape(src)))
                .unwrap_or_default();
            let who = if avatar.is_empty() && name.is_empty() {
                String::new()
            } else {
                format!(r#"<div class="ob-chat-who">{avatar}{}</div>"#, escape(name))
            };
            let side = if item_str(item, "side") == "right" {
                " ob-chat-right"
            } else {
                ""
            };
            Some(format!(
                r#"<div class="ob-chat-item{side}">{who}<div class="ob-chat-bubble">{}</div></div>"#,
                paragraphs(&lines)
            ))
        })
        .take(MAX_CHAT)
        .collect();
    (!items.is_empty()).then(|| format!(r#"<div class="ob-chat">{items}</div>"#))
}

/// タブの最大数（CSS が対応する数。`STYLES` の `nth-of-type` と合わせる）
const MAX_TABS: usize = 8;

/// 別のブロックと id が重ならないようにするための簡易ハッシュ（FNV-1a）
fn fnv(text: &str) -> u32 {
    text.bytes().fold(0x811c_9dc5u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    })
}

/// タブ。ラジオボタンと CSS だけで切り替える（JavaScript なし。矢印キーはラジオボタンの標準の操作）。
/// `index` は同じ呼び出しの中のブロックの位置（id を重ならなくする）
fn render_tabs(block: &BlockNode, index: usize) -> Option<String> {
    let tabs: Vec<(&str, String)> = list_of(block, "items")
        .into_iter()
        .filter_map(|item| {
            let label = item_str(item, "label");
            let content = item_str(item, "content");
            if label.is_empty() || content.is_empty() {
                return None;
            }
            let body = if item_str(item, "format") == "code" {
                format!("<pre><code>{}</code></pre>", escape(content))
            } else {
                paragraphs(&text_lines(content))
            };
            Some((label, body))
        })
        .take(MAX_TABS)
        .collect();
    if tabs.is_empty() {
        return None;
    }
    let seed: String = tabs
        .iter()
        .map(|(l, b)| format!("{l}\u{1}{b}\u{2}"))
        .collect();
    let group = format!("ob-tabs-{index}-{:08x}", fnv(&seed));
    let (mut inputs, mut labels, mut panels) = (String::new(), String::new(), String::new());
    for (i, (label, body)) in tabs.iter().enumerate() {
        let checked = if i == 0 { " checked" } else { "" };
        inputs.push_str(&format!(
            r#"<input type="radio" name="{group}" id="{group}-{i}" aria-label="{}"{checked}>"#,
            escape(label)
        ));
        labels.push_str(&format!(
            r#"<label for="{group}-{i}">{}</label>"#,
            escape(label)
        ));
        panels.push_str(&format!(
            r#"<div class="ob-tab-panel"><p class="ob-tab-title">{}</p>{body}</div>"#,
            escape(label)
        ));
    }
    Some(format!(
        r#"<div class="ob-tabs">{inputs}<div class="ob-tabs-bar">{labels}</div><div class="ob-tabs-panels">{panels}</div></div>"#
    ))
}

/// 数値の最大数
const MAX_STATS: usize = 6;

fn render_stats(block: &BlockNode) -> Option<String> {
    let items: String = list_of(block, "items")
        .into_iter()
        .filter(|item| !item_str(item, "value").is_empty())
        .take(MAX_STATS)
        .map(|item| {
            let unit = item_str(item, "unit");
            let unit = if unit.is_empty() {
                String::new()
            } else {
                format!(r#"<span class="ob-stat-unit">{}</span>"#, escape(unit))
            };
            let label = item_str(item, "label");
            let label = if label.is_empty() {
                String::new()
            } else {
                format!(r#"<span class="ob-stat-label">{}</span>"#, escape(label))
            };
            format!(
                r#"<div class="ob-stat"><span class="ob-stat-value">{}{unit}</span>{label}</div>"#,
                escape(item_str(item, "value"))
            )
        })
        .collect();
    (!items.is_empty()).then(|| format!(r#"<div class="ob-stats">{items}</div>"#))
}

/// 定義リストの最大項目数
const MAX_DEFINITIONS: usize = 100;

fn render_definition_list(block: &BlockNode) -> Option<String> {
    let items: String = list_of(block, "items")
        .into_iter()
        .filter_map(|item| {
            let term = item_str(item, "term");
            let lines = text_lines(item_str(item, "description"));
            if term.is_empty() || lines.is_empty() {
                return None;
            }
            Some(format!(
                r#"<div class="ob-dl-row"><dt>{}</dt><dd>{}</dd></div>"#,
                escape(term),
                paragraphs(&lines)
            ))
        })
        .take(MAX_DEFINITIONS)
        .collect();
    (!items.is_empty()).then(|| format!(r#"<dl class="ob-dl">{items}</dl>"#))
}

/// 項目別スコアの最大数
const MAX_RATING_SCORES: usize = 10;

/// 範囲内の数を 0.5 刻みに丸める。範囲外・数値でないときは `None`
fn half_step(value: Option<f64>, min: f64) -> Option<f64> {
    let v = value?;
    (min..=5.0).contains(&v).then(|| (v * 2.0).round() / 2.0)
}

/// 星（0.5 刻み）。読み上げには「4.5 / 5」を使い、星の文字は CSS で出す
fn stars(score: f64) -> String {
    format!(
        r#"<span class="ob-stars ob-r{}" role="img" aria-label="{score} / 5"><span class="ob-stars-fill">★★★★★</span></span>"#,
        (score * 2.0) as u32
    )
}

/// 評価（レビュー）。星と、schema.org の Review の microdata（`<script>` の JSON-LD はサニタイズで消える）
fn render_rating(block: &BlockNode) -> Option<String> {
    let name = attr(block, "name");
    if name.is_empty() {
        return None;
    }
    let score = half_step(number(block, "rating"), 1.0)?;
    let item_type = match attr(block, "item_type") {
        t @ ("Product" | "SoftwareApplication" | "Book" | "Movie") => t,
        _ => "Thing",
    };
    let body = text_lines(attr(block, "summary"));
    let body = if body.is_empty() {
        String::new()
    } else {
        format!(
            r#"<div class="ob-rating-body" itemprop="reviewBody">{}</div>"#,
            paragraphs(&body)
        )
    };
    let author = attr(block, "author");
    let author = if author.is_empty() {
        String::new()
    } else {
        format!(
            r#"<p class="ob-rating-author">— <span itemprop="author" itemscope itemtype="https://schema.org/Person"><span itemprop="name">{}</span></span></p>"#,
            escape(author)
        )
    };
    let scores: String = list_of(block, "scores")
        .into_iter()
        .filter_map(|item| {
            let label = item_str(item, "label");
            let value = item.get("score").and_then(|v| match v {
                serde_json::Value::Number(n) => n.as_f64(),
                serde_json::Value::String(s) => s.trim().parse().ok(),
                _ => None,
            });
            let value = half_step(value, 0.0)?;
            (!label.is_empty()).then(|| {
                format!(
                    r#"<li><span class="ob-rating-label">{}</span>{}<span class="ob-rating-value">{value}</span></li>"#,
                    escape(label),
                    stars(value)
                )
            })
        })
        .take(MAX_RATING_SCORES)
        .collect();
    let scores = if scores.is_empty() {
        String::new()
    } else {
        format!(r#"<ul class="ob-rating-scores">{scores}</ul>"#)
    };
    Some(format!(
        r#"<div class="ob-rating" itemscope itemtype="https://schema.org/Review"><div class="ob-rating-head"><span class="ob-rating-name" itemprop="itemReviewed" itemscope itemtype="https://schema.org/{item_type}"><span itemprop="name">{}</span></span><span class="ob-rating-score" itemprop="reviewRating" itemscope itemtype="https://schema.org/Rating">{}<span class="ob-rating-number"><span itemprop="ratingValue">{score}</span> / <span itemprop="bestRating">5</span></span></span></div>{body}{author}{scores}</div>"#,
        escape(name),
        stars(score)
    ))
}

impl Plugin for BlocksPlugin {
    fn render_blocks(&mut self, blocks: Vec<BlockNode>) -> Result<Vec<Option<String>>> {
        let mut styled = false;
        Ok(blocks
            .iter()
            .enumerate()
            .map(|(index, block)| {
                let html = match block.name.as_str() {
                    "official-blocks/button" => render_button(block),
                    "official-blocks/link-card" => render_link_card(block),
                    "official-blocks/video" => render_video(block),
                    "official-blocks/audio" => render_audio(block),
                    "official-blocks/file" => render_file(block),
                    "official-blocks/map" => render_map(block),
                    "official-blocks/gallery" => render_gallery(block),
                    "official-blocks/faq" => render_faq(block),
                    "official-blocks/steps" => render_steps(block),
                    "official-blocks/columns" => render_columns(block),
                    "official-blocks/media-text" => render_media_text(block),
                    "official-blocks/figure" => render_figure(block),
                    "official-blocks/timeline" => render_timeline(block),
                    "official-blocks/chat" => render_chat(block),
                    "official-blocks/tabs" => render_tabs(block, index),
                    "official-blocks/stats" => render_stats(block),
                    "official-blocks/definition-list" => render_definition_list(block),
                    "official-blocks/rating" => render_rating(block),
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

    fn json_block(name: &str, attrs: &[(&str, serde_json::Value)]) -> BlockNode {
        BlockNode {
            name: format!("official-blocks/{}", name),
            attributes: attrs
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect::<HashMap<_, _>>(),
            text: None,
            children: Vec::new(),
        }
    }

    #[test]
    fn renders_audio_and_file_blocks() {
        let html = BlocksPlugin
            .render_blocks(vec![
                block("audio", &[("url", "/uploads/a.mp3"), ("title", "Ep <1>")]),
                block("audio", &[("url", "javascript:alert(1)")]),
                block(
                    "file",
                    &[
                        ("url", "/uploads/guide.pdf"),
                        ("title", "Guide"),
                        ("description", "Setup"),
                        ("size", "2.4 MB"),
                    ],
                ),
                block("file", &[("url", "/uploads/x.zip"), ("title", "")]),
            ])
            .unwrap();
        let audio = html[0].as_deref().unwrap();
        assert!(audio.contains("<figcaption>Ep &lt;1&gt;</figcaption>"));
        assert!(audio.contains(
            r#"<audio controls preload="none" aria-label="Ep &lt;1&gt;" src="/uploads/a.mp3">"#
        ));
        assert_eq!(html[1], None);
        let file = html[2].as_deref().unwrap();
        assert!(file.contains(r#"href="/uploads/guide.pdf" download"#));
        assert!(file.contains("Setup · 2.4 MB"));
        assert_eq!(html[3], None);
    }

    #[test]
    fn renders_maps_with_a_bounding_box_around_the_marker() {
        use serde_json::json;
        let map = |lat: serde_json::Value, lon: serde_json::Value, zoom: serde_json::Value| {
            json_block(
                "map",
                &[
                    ("latitude", lat),
                    ("longitude", lon),
                    ("zoom", zoom),
                    ("title", json!("Tokyo & co")),
                ],
            )
        };
        let html = BlocksPlugin
            .render_blocks(vec![
                map(json!(35.68), json!("139.76"), json!(15)),
                map(json!(95.0), json!(0), json!(15)),
                map(json!(0), json!(200), json!(15)),
                json_block("map", &[]),
            ])
            .unwrap();
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
    fn splits_entries_on_blank_lines() {
        let got = entries("Q1\r\nA1 line1\r\nA1 line2\r\n\r\n\r\n  Q2  \nA2\n\n");
        assert_eq!(
            got,
            vec![
                (
                    "Q1".to_string(),
                    vec!["A1 line1".to_string(), "A1 line2".to_string()]
                ),
                ("Q2".to_string(), vec!["A2".to_string()]),
            ]
        );
        assert!(entries(" \n\n").is_empty());
    }

    #[test]
    fn renders_galleries_from_one_image_per_line() {
        let html = BlocksPlugin
            .render_blocks(vec![
                block(
                    "gallery",
                    &[
                        (
                            "images",
                            "/a.jpg | Front <door>\nhttps://example.com/b.jpg\njavascript:alert(1)\n\n/c.jpg",
                        ),
                        ("columns", "4"),
                    ],
                ),
                block("gallery", &[("images", "javascript:alert(1)\nnot a url")]),
                block("gallery", &[]),
            ])
            .unwrap();
        let g = html[0].as_deref().unwrap();
        assert!(g.contains(r#"class="ob-gallery ob-gallery-4""#));
        assert_eq!(g.matches("<figure>").count(), 3);
        assert!(g.contains(
            r#"<img src="/a.jpg" alt="Front &lt;door&gt;" loading="lazy"><figcaption>Front &lt;door&gt;</figcaption>"#
        ));
        assert!(!g.contains("javascript"));
        assert_eq!(&html[1..], &[None, None]);

        let many = (0..40)
            .map(|i| format!("/{i}.jpg"))
            .collect::<Vec<_>>()
            .join("\n");
        let capped = BlocksPlugin
            .render_blocks(vec![block("gallery", &[("images", many.as_str())])])
            .unwrap();
        assert_eq!(
            capped[0].as_deref().unwrap().matches("<figure>").count(),
            24
        );
    }

    #[test]
    fn renders_faq_and_steps() {
        let faq = "Is it <free>?\nYes.\nReally.\n\nNo answer here\n\nWhere?\nHere.";
        let html = BlocksPlugin
            .render_blocks(vec![
                block("faq", &[("items", faq)]),
                block("faq", &[("items", "only a question")]),
                block(
                    "steps",
                    &[("items", "Install\nDownload it.\nRun it.\n\nStart")],
                ),
                block("steps", &[("items", "")]),
            ])
            .unwrap();
        let faq = html[0].as_deref().unwrap();
        assert!(faq.contains(r#"itemtype="https://schema.org/FAQPage""#));
        assert_eq!(faq.matches("<details").count(), 2);
        assert!(faq.contains(r#"<summary itemprop="name">Is it &lt;free&gt;?</summary>"#));
        assert!(faq.contains("<p>Yes.</p><p>Really.</p>"));
        assert!(!faq.contains("No answer here"));
        assert_eq!(html[1], None);
        let steps = html[2].as_deref().unwrap();
        assert!(steps.contains(r#"<li><span class="ob-step-title">Install</span><span class="ob-step-body">Download it. Run it.</span></li>"#));
        assert!(steps.contains(r#"<li><span class="ob-step-title">Start</span></li>"#));
        assert_eq!(html[3], None);
    }

    #[test]
    fn renders_media_text_on_either_side() {
        let html = BlocksPlugin
            .render_blocks(vec![
                block(
                    "media-text",
                    &[
                        ("image", "/uploads/a.jpg"),
                        ("alt", "A \"cup\""),
                        ("title", "Fresh"),
                        ("text", "Line one\n\nLine <two>"),
                        ("side", "right"),
                    ],
                ),
                block("media-text", &[("image", "/a.jpg"), ("title", "T")]),
                block("media-text", &[("image", "/a.jpg")]),
                block("media-text", &[("image", "javascript:x"), ("title", "T")]),
            ])
            .unwrap();
        let right = html[0].as_deref().unwrap();
        assert!(right.contains(r#"class="ob-media-text ob-media-text-right""#));
        assert!(right.contains(r#"alt="A &quot;cup&quot;""#));
        assert!(right.contains("<p>Line one</p><p>Line &lt;two&gt;</p>"));
        assert!(html[1]
            .as_deref()
            .unwrap()
            .contains(r#"class="ob-media-text">"#));
        assert_eq!(&html[2..], &[None, None]);
    }

    fn items_block(name: &str, items: serde_json::Value) -> BlockNode {
        json_block(name, &[("items", items)])
    }

    #[test]
    fn renders_a_timeline_and_skips_empty_items() {
        use serde_json::json;
        let html = BlocksPlugin
            .render_blocks(vec![
                items_block(
                    "timeline",
                    json!([
                        {"date": "2025-04", "title": "v1.0 <out>", "description": "First\nrelease"},
                        {"date": "2025-10"},
                        {"description": "no date or title"},
                        "not an object"
                    ]),
                ),
                items_block("timeline", json!([])),
            ])
            .unwrap();
        let timeline = html[0].as_deref().unwrap();
        assert!(timeline.contains(r#"<li><span class="ob-tl-date">2025-04</span><span class="ob-tl-title">v1.0 &lt;out&gt;</span><span class="ob-tl-body">First release</span></li>"#));
        assert!(timeline.contains(r#"<li><span class="ob-tl-date">2025-10</span></li>"#));
        assert_eq!(timeline.matches("<li>").count(), 2);
        assert_eq!(html[1], None);
    }

    #[test]
    fn renders_a_chat_on_either_side() {
        use serde_json::json;
        let html = BlocksPlugin
            .render_blocks(vec![
                items_block(
                    "chat",
                    json!([
                        {"name": "Ann", "avatar": "/a.png", "side": "left", "text": "Hi <b>\n\nThere"},
                        {"name": "Bob", "avatar": "javascript:x", "side": "right", "text": "Yo"},
                        {"name": "Nobody", "text": ""},
                        {"text": "Anonymous"}
                    ]),
                ),
                items_block("chat", json!([{"name": "x", "text": ""}])),
            ])
            .unwrap();
        let chat = html[0].as_deref().unwrap();
        assert!(chat.contains(r#"<div class="ob-chat-item"><div class="ob-chat-who"><img src="/a.png" alt="" loading="lazy">Ann</div><div class="ob-chat-bubble"><p>Hi &lt;b&gt;</p><p>There</p></div></div>"#));
        assert!(chat.contains(
            r#"<div class="ob-chat-item ob-chat-right"><div class="ob-chat-who">Bob</div>"#
        ));
        assert!(!chat.contains("javascript"));
        assert!(chat
            .contains(r#"<div class="ob-chat-item"><div class="ob-chat-bubble"><p>Anonymous</p>"#));
        assert_eq!(chat.matches(r#"<div class="ob-chat-item"#).count(), 3);
        assert_eq!(html[1], None);
    }

    #[test]
    fn renders_tabs_with_unique_radio_groups() {
        use serde_json::json;
        let tabs = |label: &str| {
            items_block(
                "tabs",
                json!([
                    {"label": label, "content": "npm i <x>", "format": "code"},
                    {"label": "pnpm", "content": "Line one\nLine two"},
                    {"label": "", "content": "dropped"},
                    {"label": "empty", "content": ""}
                ]),
            )
        };
        let html = BlocksPlugin
            .render_blocks(vec![
                tabs("npm"),
                tabs("npm"),
                items_block("tabs", json!([])),
            ])
            .unwrap();
        let first = html[0].as_deref().unwrap();
        assert_eq!(first.matches(r#"type="radio""#).count(), 2);
        assert_eq!(first.matches(" checked>").count(), 1);
        assert!(first.contains("<pre><code>npm i &lt;x&gt;</code></pre>"));
        assert!(first.contains("<p>Line one</p><p>Line two</p>"));
        assert!(first.contains(r#"<label for="#));
        // 同じ内容のブロックでも、ラジオボタンの name は重ならない
        let name = |h: &str| {
            h.split(r#"name=""#)
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
                .to_string()
        };
        assert_ne!(name(first), name(html[1].as_deref().unwrap()));
        assert_eq!(html[2], None);
    }

    #[test]
    fn caps_the_number_of_tabs() {
        use serde_json::json;
        let items: Vec<_> = (0..12)
            .map(|i| json!({"label": format!("T{i}"), "content": "x"}))
            .collect();
        let html = BlocksPlugin
            .render_blocks(vec![items_block("tabs", json!(items))])
            .unwrap();
        assert_eq!(
            html[0]
                .as_deref()
                .unwrap()
                .matches(r#"type="radio""#)
                .count(),
            MAX_TABS
        );
    }

    #[test]
    fn renders_stats_and_a_definition_list() {
        use serde_json::json;
        let html = BlocksPlugin
            .render_blocks(vec![
                items_block(
                    "stats",
                    json!([
                        {"value": "99.9", "unit": "%", "label": "Uptime <SLA>"},
                        {"value": "15+"},
                        {"unit": "x", "label": "no value"}
                    ]),
                ),
                items_block(
                    "definition-list",
                    json!([
                        {"term": "CPU <x>", "description": "Fast\n\nQuiet"},
                        {"term": "No description"},
                        {"description": "No term"}
                    ]),
                ),
                items_block("stats", json!([{"label": "x"}])),
            ])
            .unwrap();
        let stats = html[0].as_deref().unwrap();
        assert!(stats.contains(r#"<div class="ob-stat"><span class="ob-stat-value">99.9<span class="ob-stat-unit">%</span></span><span class="ob-stat-label">Uptime &lt;SLA&gt;</span></div>"#));
        assert!(
            stats.contains(r#"<div class="ob-stat"><span class="ob-stat-value">15+</span></div>"#)
        );
        assert_eq!(stats.matches(r#"class="ob-stat""#).count(), 2);
        let dl = html[1].as_deref().unwrap();
        assert!(dl.contains(r#"<dl class="ob-dl"><div class="ob-dl-row"><dt>CPU &lt;x&gt;</dt><dd><p>Fast</p><p>Quiet</p></dd></div></dl>"#));
        assert_eq!(html[2], None);
    }

    #[test]
    fn renders_a_rating_with_review_microdata() {
        use serde_json::json;
        let html = BlocksPlugin
            .render_blocks(vec![
                json_block(
                    "rating",
                    &[
                        ("name", json!("Widget <Pro>")),
                        ("item_type", json!("Product")),
                        ("rating", json!(4.3)),
                        ("summary", json!("Solid.\n\nWould buy")),
                        ("author", json!("Ann")),
                        (
                            "scores",
                            json!([
                                {"label": "Battery", "score": 5},
                                {"label": "Price", "score": "3.5"},
                                {"label": "", "score": 4},
                                {"label": "Out of range", "score": 9}
                            ]),
                        ),
                    ],
                ),
                json_block("rating", &[("name", json!("X")), ("rating", json!(0))]),
                json_block("rating", &[("name", json!("X")), ("rating", json!("n/a"))]),
                json_block("rating", &[("name", json!("")), ("rating", json!(4))]),
                json_block(
                    "rating",
                    &[
                        ("name", json!("Y")),
                        ("rating", json!(5)),
                        ("item_type", json!("Evil\"Type")),
                    ],
                ),
            ])
            .unwrap();
        let rating = html[0].as_deref().unwrap();
        assert!(rating.contains(r#"itemtype="https://schema.org/Review""#));
        assert!(rating.contains(r#"itemtype="https://schema.org/Product"><span itemprop="name">Widget &lt;Pro&gt;</span>"#));
        // 4.3 は 0.5 刻みに丸めて 4.5
        assert!(rating.contains(
            r#"<span itemprop="ratingValue">4.5</span> / <span itemprop="bestRating">5</span>"#
        ));
        assert!(rating.contains(r#"class="ob-stars ob-r9" role="img" aria-label="4.5 / 5""#));
        assert!(rating.contains(r#"itemprop="reviewBody"><p>Solid.</p><p>Would buy</p>"#));
        assert!(rating.contains(r#"itemprop="author" itemscope itemtype="https://schema.org/Person"><span itemprop="name">Ann</span>"#));
        // 項目別スコア: 範囲外・名前の無い項目は出さない
        assert_eq!(
            rating
                .matches(r#"<li><span class="ob-rating-label">"#)
                .count(),
            2
        );
        assert!(rating.contains(r#"class="ob-rating-value">3.5</span>"#));
        assert_eq!(&html[1..4], &[None, None, None]);
        // 知らない種類は Thing
        assert!(html[4].as_deref().unwrap().contains(r#"schema.org/Thing""#));
    }

    #[test]
    fn renders_a_figure_with_an_optional_caption() {
        let html = BlocksPlugin
            .render_blocks(vec![
                block(
                    "figure",
                    &[
                        ("image", "/uploads/a.jpg"),
                        ("alt", "A \"cup\""),
                        ("caption", "Fig. 1 <cup>"),
                    ],
                ),
                block("figure", &[("image", "/a.jpg")]),
                block("figure", &[("image", "javascript:x"), ("caption", "C")]),
                block("figure", &[("caption", "no image")]),
            ])
            .unwrap();
        let first = html[0].as_deref().unwrap();
        assert!(first.ends_with(r#"<figure class="ob-figure"><img src="/uploads/a.jpg" alt="A &quot;cup&quot;" loading="lazy"><figcaption>Fig. 1 &lt;cup&gt;</figcaption></figure>"#));
        assert_eq!(
            html[1].as_deref().unwrap(),
            r#"<figure class="ob-figure"><img src="/a.jpg" alt="" loading="lazy"></figure>"#
        );
        assert_eq!(&html[2..], &[None, None]);
    }

    #[test]
    fn renders_list_fields_like_the_text_form() {
        use serde_json::json;
        let html = BlocksPlugin
            .render_blocks(vec![
                json_block(
                    "gallery",
                    &[
                        (
                            "images",
                            json!([
                                {"image": "/a.jpg", "caption": "Front <door>"},
                                {"image": "javascript:alert(1)", "caption": "x"},
                                {"image": "", "caption": "none"},
                                "not an object",
                                {"image": "/b.jpg"}
                            ]),
                        ),
                        ("columns", json!("2")),
                    ],
                ),
                json_block(
                    "faq",
                    &[(
                        "items",
                        json!([
                            {"question": "Is it <free>?", "answer": "Yes.\nReally."},
                            {"question": "No answer", "answer": " "},
                            {"question": "", "answer": "Orphan"}
                        ]),
                    )],
                ),
                json_block(
                    "steps",
                    &[(
                        "items",
                        json!([
                            {"title": "Install", "description": "Download it.\nRun it."},
                            {"title": "Start"},
                            {"description": "no title"}
                        ]),
                    )],
                ),
                json_block("gallery", &[("images", json!([]))]),
            ])
            .unwrap();
        let g = html[0].as_deref().unwrap();
        assert!(g.contains(r#"class="ob-gallery ob-gallery-2""#));
        assert_eq!(g.matches("<figure>").count(), 2);
        assert!(g.contains(r#"alt="Front &lt;door&gt;""#));
        assert!(!g.contains("javascript"));
        let faq = html[1].as_deref().unwrap();
        assert_eq!(faq.matches("<details").count(), 1);
        assert!(faq.contains("<p>Yes.</p><p>Really.</p>"));
        assert!(!faq.contains("Orphan") && !faq.contains("No answer"));
        let steps = html[2].as_deref().unwrap();
        assert_eq!(steps.matches("<li>").count(), 2);
        assert!(steps.contains("Download it. Run it."));
        assert_eq!(html[3], None);
    }

    #[test]
    fn renders_columns() {
        use serde_json::json;
        let html = BlocksPlugin
            .render_blocks(vec![
                json_block(
                    "columns",
                    &[(
                        "items",
                        json!([
                            {"image": "/a.jpg", "title": "Fast <1>", "text": "One\n\nTwo"},
                            {"title": "Safe"},
                            {"image": "javascript:x"},
                            {"text": "Only text"},
                            {"title": "Fifth is dropped"},
                            {"title": "Sixth"}
                        ]),
                    )],
                ),
                json_block(
                    "columns",
                    &[("items", json!([{}, {"image": "javascript:x"}]))],
                ),
                json_block("columns", &[]),
            ])
            .unwrap();
        let c = html[0].as_deref().unwrap();
        // 4 列まで。中身の無い項目は列にしない（先頭 4 項目のうち 3 つ）
        assert_eq!(c.matches(r#"class="ob-column""#).count(), 3);
        assert!(c.contains(r#"<img src="/a.jpg" alt="Fast &lt;1&gt;" loading="lazy">"#));
        assert!(c.contains("<p>One</p><p>Two</p>"));
        assert!(!c.contains("Fifth") && !c.contains("javascript"));
        assert_eq!(&html[1..], &[None, None]);
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
