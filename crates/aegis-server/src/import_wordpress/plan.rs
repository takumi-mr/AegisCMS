//! 取り込みの計画（WordPress の項目を、どの型・状態・slug で取り込むか）。ネットワークには触れない。

use aegis_wordpress::html::{parse_fragment, Node};
use aegis_wordpress::{WpItem, WpTerm};
use jiff::Timestamp;

/// 記事の `excerpt` の最大文字数（`schemas/articles.yaml`）
pub const MAX_EXCERPT_CHARS: usize = 500;

/// 取り込み先の型。`post` → `article`、`page` → `page`。それ以外（独自の投稿タイプなど）は取り込まない
pub fn target_type(post_type: &str) -> Option<&'static str> {
    match post_type {
        "post" => Some("article"),
        "page" => Some("page"),
        _ => None,
    }
}

/// 取り込み先での状態と、その理由（手直しの参考として報告する）
#[derive(Debug, PartialEq)]
pub struct Status {
    pub status: &'static str,
    /// 公開日時（公開済み・予約のとき）
    pub published_at: Option<Timestamp>,
    /// 元の状態のままではない理由
    pub note: Option<&'static str>,
}

/// 取り込まない理由
#[derive(Debug, PartialEq, Eq)]
pub enum Skip {
    /// 型が対応していない（独自の投稿タイプなど）
    UnsupportedType,
    /// ゴミ箱・自動保存・リビジョンなど
    NotContent,
}

/// 項目の状態を決める。公開は公開、下書きは下書き、予約は予約。
/// 非公開・パスワード保護は、勝手に公開しないよう下書きにして報告する
pub fn map_status(item: &WpItem) -> Result<Status, Skip> {
    let plain = |status, note| Status {
        status,
        published_at: item.published_at,
        note,
    };
    let status = match item.status.as_str() {
        "publish" => plain("published", None),
        "draft" => plain("draft", None),
        "pending" => plain("pending", None),
        "future" => match item.published_at {
            Some(_) => plain("scheduled", None),
            None => plain("draft", Some("scheduled without a date")),
        },
        "private" => plain("draft", Some("private")),
        _ => return Err(Skip::NotContent),
    };
    if item.password.is_some() {
        return Ok(Status {
            status: "draft",
            published_at: status.published_at,
            note: Some("password protected"),
        });
    }
    Ok(status)
}

/// 元の項目を取り込むか（型と状態を見る）
pub fn classify(item: &WpItem) -> Result<(&'static str, Status), Skip> {
    let ty = target_type(&item.post_type).ok_or(Skip::UnsupportedType)?;
    Ok((ty, map_status(item)?))
}

fn hex_digit(byte: u8) -> Option<u8> {
    (byte as char).to_digit(16).map(|d| d as u8)
}

/// URL の `%E3%81%82` を元の文字に戻す。UTF-8 として読めなければ元のまま返す
pub fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex_digit(bytes[i + 1]), hex_digit(bytes[i + 2])) {
                out.push(high * 16 + low);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| raw.to_string())
}

/// 取り込み先の slug。WordPress は日本語の slug を `%E3%81%82` の形で書き出すので戻す。
/// 空（下書きなど）なら `wp-<ID>`。`/` や空白は `-` にする
pub fn item_slug(item: &WpItem) -> String {
    let decoded = percent_decode(item.slug.trim());
    let cleaned: String = decoded
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c == '?' || c == '#' || c.is_whitespace() {
                '-'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches('-');
    if cleaned.is_empty() {
        format!("wp-{}", item.id)
    } else {
        cleaned.to_string()
    }
}

/// タグ・カテゴリの slug。半角英数字とハイフンだけが使えるので、そうでなければ `None`
/// （呼び出し側が名前から作るか、`wp-<種類>-<ID>` にする）
pub fn term_slug(term: &WpTerm) -> Option<String> {
    let decoded = percent_decode(&term.slug);
    aegis_tags::is_valid_slug(&decoded).then_some(decoded)
}

/// 記事のカテゴリ。複数あれば最初の 1 つ（「未分類」だけなら無し）。
/// 戻り値は（カテゴリの slug、選ばれなかった slug）
pub fn pick_category(item: &WpItem) -> (Option<&str>, Vec<&str>) {
    let mut slugs = item
        .term_slugs("category")
        .filter(|slug| *slug != "uncategorized");
    let first = slugs.next();
    (first, slugs.collect())
}

/// 抜粋（HTML のタグを除いた文字列、`MAX_EXCERPT_CHARS` まで）。空なら `None`
pub fn excerpt_text(html: &str) -> Option<String> {
    fn collect(nodes: &[Node], out: &mut String) {
        for node in nodes {
            match node {
                Node::Text(t) => out.push_str(t),
                Node::Element(e) => collect(&e.children, out),
                Node::Comment(_) => {}
            }
        }
    }
    let mut text = String::new();
    collect(&parse_fragment(html), &mut text);
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return None;
    }
    Some(text.chars().take(MAX_EXCERPT_CHARS).collect())
}

/// ユーザー名として使える形にする（半角英数字と `. _ - @`、64 文字まで）。足りなければ `wp-<ID>`
pub fn username(login: &str, id: u64) -> String {
    let cleaned: String = login
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '@') {
                c
            } else {
                '-'
            }
        })
        .take(64)
        .collect();
    if cleaned.trim_matches('-').is_empty() {
        format!("wp-{id}")
    } else {
        cleaned
    }
}

/// 本文の中の古い URL を、取り込み後の URL に置き換える。
///
/// `old` が `…/foo.jpg` のとき、WordPress が作る縮小版 `…/foo-300x200.jpg` も同じ画像として
/// 置き換える（本文には縮小版の URL が書かれていることが多い）。長い URL から先に当てるので、
/// ある URL が別の URL の前半になっていても取り違えない。
pub fn rewrite_urls(text: &str, map: &[(String, String)]) -> String {
    let mut pairs: Vec<&(String, String)> = map.iter().filter(|(old, _)| !old.is_empty()).collect();
    pairs.sort_by_key(|(old, _)| std::cmp::Reverse(old.len()));
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    'scan: while !rest.is_empty() {
        for (old, new) in &pairs {
            if let Some(tail) = rest.strip_prefix(old.as_str()) {
                out.push_str(new);
                rest = tail;
                continue 'scan;
            }
            // 縮小版: `<拡張子の前まで>-<幅>x<高さ><拡張子>`
            if let Some((stem, ext)) = split_extension(old) {
                if let Some(tail) = rest.strip_prefix(stem) {
                    if let Some(after) = strip_size_suffix(tail, ext) {
                        // 取り込み先に縮小版は無いので、元の画像に向ける
                        out.push_str(new);
                        rest = after;
                        continue 'scan;
                    }
                }
            }
        }
        let ch = rest.chars().next().unwrap_or_default();
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    out
}

/// `foo.jpg` → (`foo`, `.jpg`)。パスの最後の `.` で分ける（拡張子が無ければ `None`）
fn split_extension(url: &str) -> Option<(&str, &str)> {
    let name_start = url.rfind('/').map_or(0, |i| i + 1);
    let dot = url[name_start..].rfind('.')? + name_start;
    if dot + 1 >= url.len() {
        return None;
    }
    Some((&url[..dot], &url[dot..]))
}

/// `-300x200.jpg` で始まっていれば、その後ろを返す
fn strip_size_suffix<'a>(tail: &'a str, ext: &str) -> Option<&'a str> {
    let rest = tail.strip_prefix('-')?;
    let width = rest.bytes().take_while(u8::is_ascii_digit).count();
    let rest = rest[width..].strip_prefix('x')?;
    let height = rest.bytes().take_while(u8::is_ascii_digit).count();
    if width == 0 || height == 0 {
        return None;
    }
    rest[height..].strip_prefix(ext)
}

/// JSON の中の文字列すべてに `rewrite_urls` をかける
pub fn rewrite_json(value: &mut serde_json::Value, map: &[(String, String)]) {
    match value {
        serde_json::Value::String(s) => {
            if map.iter().any(|(old, _)| s.contains(split_stem(old))) {
                *s = rewrite_urls(s, map);
            }
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(|v| rewrite_json(v, map)),
        serde_json::Value::Object(object) => object.values_mut().for_each(|v| rewrite_json(v, map)),
        _ => {}
    }
}

/// 速く外れるための前判定に使う、縮小版も含む共通の前半
fn split_stem(old: &str) -> &str {
    split_extension(old).map_or(old, |(stem, _)| stem)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(post_type: &str, status: &str) -> WpItem {
        WpItem {
            id: 7,
            post_type: post_type.to_string(),
            status: status.to_string(),
            ..WpItem::default()
        }
    }

    #[test]
    fn maps_types_and_statuses() {
        assert_eq!(target_type("post"), Some("article"));
        assert_eq!(target_type("page"), Some("page"));
        assert_eq!(target_type("product"), None);

        let mut published = item("post", "publish");
        published.published_at = Some("2020-05-01T00:00:00Z".parse().unwrap());
        let (ty, status) = classify(&published).unwrap();
        assert_eq!(ty, "article");
        assert_eq!(status.status, "published");
        assert!(status.published_at.is_some());

        assert_eq!(classify(&item("page", "draft")).unwrap().1.status, "draft");
        assert_eq!(classify(&item("post", "trash")), Err(Skip::NotContent));
        assert_eq!(classify(&item("post", "auto-draft")), Err(Skip::NotContent));
        assert_eq!(
            classify(&item("product", "publish")),
            Err(Skip::UnsupportedType)
        );
    }

    #[test]
    fn private_and_password_posts_are_never_published() {
        let private = map_status(&item("post", "private")).unwrap();
        assert_eq!((private.status, private.note), ("draft", Some("private")));

        let mut locked = item("post", "publish");
        locked.password = Some("secret".to_string());
        let locked = map_status(&locked).unwrap();
        assert_eq!(locked.status, "draft");
        assert_eq!(locked.note, Some("password protected"));
    }

    #[test]
    fn a_future_post_without_a_date_becomes_a_draft() {
        let status = map_status(&item("post", "future")).unwrap();
        assert_eq!(status.status, "draft");
        let mut dated = item("post", "future");
        dated.published_at = Some("2099-01-01T00:00:00Z".parse().unwrap());
        assert_eq!(map_status(&dated).unwrap().status, "scheduled");
    }

    #[test]
    fn slugs_are_decoded_and_never_empty() {
        let mut it = item("post", "publish");
        it.slug = "%e3%81%93%e3%82%93%e3%81%ab%e3%81%a1%e3%81%af".to_string();
        assert_eq!(item_slug(&it), "こんにちは");
        it.slug = String::new();
        assert_eq!(item_slug(&it), "wp-7");
        it.slug = "a b/c".to_string();
        assert_eq!(item_slug(&it), "a-b-c");
        // 壊れた % はそのまま
        it.slug = "100%".to_string();
        assert_eq!(item_slug(&it), "100%");
        it.slug = "%zz-x".to_string();
        assert_eq!(item_slug(&it), "%zz-x");
    }

    #[test]
    fn term_slugs_must_fit_the_tag_rules() {
        let mut term = WpTerm {
            slug: "rust-lang".to_string(),
            ..WpTerm::default()
        };
        assert_eq!(term_slug(&term).as_deref(), Some("rust-lang"));
        term.slug = "%e3%82%bf%e3%82%b0".to_string();
        assert_eq!(term_slug(&term), None);
    }

    #[test]
    fn the_first_real_category_is_chosen() {
        use aegis_wordpress::WpTermRef;
        let mut it = item("post", "publish");
        let term = |slug: &str| WpTermRef {
            taxonomy: "category".to_string(),
            slug: slug.to_string(),
            name: slug.to_string(),
        };
        it.terms = vec![term("uncategorized"), term("news"), term("tech")];
        assert_eq!(pick_category(&it), (Some("news"), vec!["tech"]));
        it.terms = vec![term("uncategorized")];
        assert_eq!(pick_category(&it), (None, vec![]));
    }

    #[test]
    fn excerpts_are_plain_text_and_capped() {
        assert_eq!(
            excerpt_text("<p>Hello <b>world</b></p>\n<p>again</p>").as_deref(),
            Some("Hello world again")
        );
        assert_eq!(excerpt_text("  "), None);
        let long = "あ".repeat(900);
        assert_eq!(excerpt_text(&long).unwrap().chars().count(), 500);
    }

    #[test]
    fn usernames_are_made_valid() {
        assert_eq!(username("taro.yamada", 1), "taro.yamada");
        assert_eq!(username("山田 太郎", 5), "wp-5");
        assert_eq!(username("a b", 5), "a-b");
        assert_eq!(username(&"x".repeat(100), 1).len(), 64);
    }

    fn map(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn media_urls_are_rewritten_including_resized_copies() {
        let map = map(&[(
            "https://old.example/wp-content/uploads/2020/05/cat.jpg",
            "/storage/wordpress/2020/05/cat.jpg",
        )]);
        let html = r#"<img src="https://old.example/wp-content/uploads/2020/05/cat-300x200.jpg"> and https://old.example/wp-content/uploads/2020/05/cat.jpg."#;
        assert_eq!(
            rewrite_urls(html, &map),
            r#"<img src="/storage/wordpress/2020/05/cat.jpg"> and /storage/wordpress/2020/05/cat.jpg."#
        );
        // 別のファイルや、サイズではない接尾辞は触らない
        let other = "https://old.example/wp-content/uploads/2020/05/cat-big.jpg";
        assert_eq!(rewrite_urls(other, &map), other);
        let unrelated = "https://old.example/wp-content/uploads/2020/05/cat.jpgx";
        assert_eq!(
            rewrite_urls(unrelated, &map),
            "/storage/wordpress/2020/05/cat.jpgx"
        );
    }

    #[test]
    fn json_strings_are_rewritten_deeply() {
        let map = map(&[("https://old.example/a.png", "/s/a.png")]);
        let mut value = serde_json::json!({
            "type": "core/image",
            "attrs": { "src": "https://old.example/a.png", "alt": "x" },
            "children": ["see https://old.example/a.png"],
            "n": 3
        });
        rewrite_json(&mut value, &map);
        assert_eq!(value["attrs"]["src"], "/s/a.png");
        assert_eq!(value["children"][0], "see /s/a.png");
    }
}
