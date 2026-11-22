//! `aegis-server import wordpress <file.xml>` — WordPress の書き出しファイル（WXR）から取り込む。
//!
//! 稼働中のサーバーの REST API（admin の API トークン）を通して作るので、保存のたびに通常の保存処理
//! （ブロックの変換、版の記録、公開、フック、監査ログ）が走る。データベースには直接触れない。
//! 同じファイルを何度流しても、すでにあるものは飛ばす（slug が同じなら同じものとみなす）。

mod client;
mod plan;
mod report;

#[cfg(test)]
mod e2e_tests;

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use aegis_wordpress::{convert, parse_wxr, Context, WpItem, WxrExport};
use clap::Args;
use reqwest::{Method, StatusCode};
use serde_json::{json, Value};

use client::Client;
use report::Report;

#[derive(Args, Debug)]
pub struct WordpressArgs {
    /// WordPress の「ツール → エクスポート」で書き出した XML ファイル（WXR）
    pub file: PathBuf,

    /// 取り込み先のサーバー
    #[arg(long, env = "AEGIS_URL", default_value = "http://127.0.0.1:8080")]
    pub url: String,

    /// 取り込みを行う admin のユーザー名（ログインして取り込む。2 段階認証のユーザーは使えない）
    #[arg(long, short = 'u', env = "AEGIS_USERNAME")]
    pub username: Option<String>,

    /// そのパスワード
    #[arg(long, env = "AEGIS_PASSWORD", hide_env_values = true)]
    pub password: Option<String>,

    /// 何も作らずに、取り込める内容と手直しが要りそうな箇所だけを報告する
    #[arg(long)]
    pub dry_run: bool,

    /// 報告の書き出し先（Markdown）
    #[arg(long, default_value = "wordpress-import-report.md")]
    pub report: PathBuf,

    /// 画像などの添付ファイルを取り込まない（本文の画像は元のサイトのままになる）
    #[arg(long)]
    pub skip_media: bool,

    /// 著者をユーザーとして作らない（記事の著者は API トークンの持ち主になる）
    #[arg(long)]
    pub skip_authors: bool,
}

pub async fn run(args: WordpressArgs) -> anyhow::Result<()> {
    let xml = std::fs::read_to_string(&args.file)
        .map_err(|e| anyhow::anyhow!("Cannot read {}: {e}", args.file.display()))?;
    let export = parse_wxr(&xml).map_err(|e| anyhow::anyhow!("Cannot read the export: {e}"))?;
    drop(xml);

    let summary = export.summary();
    println!(
        "Read '{}': {} authors, {} categories, {} tags, {} comments",
        export.site.title, summary.authors, summary.categories, summary.tags, summary.comments
    );
    for (post_type, statuses) in &summary.items {
        let detail: Vec<String> = statuses.iter().map(|(s, n)| format!("{s} {n}")).collect();
        println!("  {post_type}: {}", detail.join(", "));
    }

    let mut report = Report::default();
    if args.dry_run {
        dry_run(&export, &mut report);
    } else {
        let (Some(username), Some(password)) = (args.username.as_deref(), args.password.as_deref())
        else {
            anyhow::bail!(
                "An admin login is needed: pass --username and --password (or set AEGIS_USERNAME and AEGIS_PASSWORD)"
            );
        };
        // API トークンは editor までなので、ユーザーの作成と公開日の指定ができる admin でログインする
        let mut client = Client::new(&args.url, "").map_err(|e| anyhow::anyhow!(e))?;
        client
            .login(username, password)
            .await
            .map_err(|e| anyhow::anyhow!("Login failed: {e}"))?;
        import(&client, &export, &args, &mut report).await?;
    }

    std::fs::write(&args.report, report.render(args.dry_run))
        .map_err(|e| anyhow::anyhow!("Cannot write {}: {e}", args.report.display()))?;
    println!("Report: {}", args.report.display());
    if report.failures() > 0 {
        anyhow::bail!(
            "{} item(s) failed; see the report and run the same command again to retry them",
            report.failures()
        );
    }
    Ok(())
}

/// 取り込めるものと、取り込めないもの（ネットワークには触れない）
fn dry_run(export: &WxrExport, report: &mut Report) {
    for _ in &export.authors {
        report.created("author");
    }
    for _ in &export.categories {
        report.created("category");
    }
    for _ in &export.tags {
        report.created("tag");
    }
    let mut published: Vec<(&WpItem, &'static str)> = Vec::new();
    for item in &export.items {
        if item.post_type == "attachment" {
            if item.attachment_url.is_some() {
                report.created("media");
            }
            continue;
        }
        let Some((ty, status)) = classify(item, report) else {
            continue;
        };
        report.created(ty);
        let conversion = convert(&item.content, &Context::default());
        for note in conversion.notes {
            report.note(
                ty,
                &describe(item),
                &format!("{}: {}", note.code, note.detail),
            );
        }
        let comments = importable_comments(item);
        if status.status == "published" {
            for _ in &comments {
                report.created("comment");
            }
        } else if !comments.is_empty() {
            report.note(
                "comment",
                &describe(item),
                "comments are not imported: the post is not published",
            );
        }
        if status.status == "published" {
            published.push((item, ty));
        }
    }
    let redirects = plan::plan_redirects(&published, "/articles", MAX_REDIRECTS);
    for _ in redirects.exact.iter().chain(&redirects.patterns) {
        report.created("redirect");
    }
    redirect_notes(&redirects, report);
    notes_for_everything(export, report);
}

/// サーバーが持てるリダイレクトの規則の数（`redirects::MAX_RULES`）
const MAX_REDIRECTS: usize = crate::redirects::MAX_RULES;

/// 取り込むコメント（ピンバック・トラックバックは取り込まない）
fn importable_comments(item: &WpItem) -> Vec<&aegis_wordpress::WpComment> {
    item.comments
        .iter()
        .filter(|c| c.kind.is_empty() || c.kind == "comment")
        .collect()
}

fn redirect_notes(redirects: &plan::RedirectPlan, report: &mut Report) {
    if redirects.unmatched > 0 {
        report.note(
            "redirect",
            &format!("{} posts and pages", redirects.unmatched),
            "no redirect: the old URL is not a path (for example /?p=123)",
        );
    }
    if redirects.over_budget > 0 {
        report.note(
            "redirect",
            &format!("{} posts and pages", redirects.over_budget),
            "no redirect: the server holds at most 2,000 redirect rules",
        );
    }
}

/// 型と状態を決める。取り込まないものは報告に残して `None`
fn classify(item: &WpItem, report: &mut Report) -> Option<(&'static str, plan::Status)> {
    match plan::classify(item) {
        Ok((ty, status)) => {
            if let Some(why) = status.note {
                report.note(ty, &describe(item), &format!("imported as a draft: {why}"));
            }
            Some((ty, status))
        }
        Err(plan::Skip::NotContent) => None,
        Err(plan::Skip::UnsupportedType) => {
            report.skipped(
                "other",
                &describe(item),
                &format!("post type '{}' is not imported", item.post_type),
            );
            None
        }
    }
}

/// 取り込みでは扱わないものを、まとめて報告に残す
fn notes_for_everything(export: &WxrExport, report: &mut Report) {
    let with_thumbnail = export
        .items
        .iter()
        .filter(|i| i.meta.contains_key("_thumbnail_id"))
        .count();
    if with_thumbnail > 0 {
        report.note(
            "media",
            &format!("{with_thumbnail} posts"),
            "featured images are not imported (the content type has no field for them)",
        );
    }
    let nested_pages = export
        .items
        .iter()
        .filter(|i| i.post_type == "page" && i.parent != 0)
        .count();
    if nested_pages > 0 {
        report.note(
            "page",
            &format!("{nested_pages} pages"),
            "parent pages are dropped: the pages are imported flat",
        );
    }
}

fn describe(item: &WpItem) -> String {
    if item.title.trim().is_empty() {
        format!("(no title) #{}", item.id)
    } else {
        format!("{} (#{})", item.title.trim(), item.id)
    }
}

async fn import(
    client: &Client,
    export: &WxrExport,
    args: &WordpressArgs,
    report: &mut Report,
) -> anyhow::Result<()> {
    // admin のトークンか、先に確かめる（途中で権限不足に気づくより早い）
    let (status, _) = client
        .json(Method::GET, "/api/v1/users", None)
        .await
        .map_err(|e| anyhow::anyhow!("Cannot reach {}: {e}", args.url))?;
    if status != StatusCode::OK {
        anyhow::bail!(
            "The server answered {} to GET /api/v1/users: log in as an admin",
            status.as_u16()
        );
    }

    // 取り込む項目（古い順）
    let mut items: Vec<(&WpItem, &'static str, plan::Status)> = Vec::new();
    for item in &export.items {
        if item.post_type == "attachment" {
            continue;
        }
        if let Some((ty, status)) = classify(item, report) {
            items.push((item, ty, status));
        }
    }
    items.sort_by_key(|(item, _, status)| (status.published_at, item.id));

    let authors = if args.skip_authors {
        HashMap::new()
    } else {
        import_authors(client, export, &items, report).await
    };
    let categories = import_categories(client, export, report).await;
    let tags = import_tags(client, export, report).await;
    let (media_urls, media_map) = if args.skip_media {
        (HashMap::new(), Vec::new())
    } else {
        import_media(client, export, report).await
    };

    let ctx = Context {
        attachments: Some(&media_urls),
    };
    let total = items.len();
    for (n, (item, ty, status)) in items.iter().enumerate() {
        if n % 25 == 0 {
            println!("Importing posts and pages… {n}/{total}");
        }
        import_item(
            client,
            item,
            ty,
            status,
            &ImportContext {
                convert: &ctx,
                authors: &authors,
                categories: &categories,
                tags: &tags,
                media_map: &media_map,
            },
            report,
        )
        .await;
    }
    println!("Imported {total} posts and pages");
    let published: Vec<(&WpItem, &'static str)> = items
        .iter()
        .filter(|(_, _, status)| status.status == "published")
        .map(|(item, ty, _)| (*item, *ty))
        .collect();
    import_redirects(client, &published, report).await;
    notes_for_everything(export, report);
    Ok(())
}

/// 旧 URL → 新 URL のリダイレクトを作る。規則の数の上限に収まらないときは、旧パーマリンクの形を
/// 1 つのパターンの規則にする
async fn import_redirects(client: &Client, items: &[(&WpItem, &'static str)], report: &mut Report) {
    let prefix = match client.json(Method::GET, "/api/v1/public-paths", None).await {
        Ok((StatusCode::OK, body)) => body
            .as_array()
            .and_then(|entries| entries.iter().find(|e| e["content_type"] == "article"))
            .and_then(|e| e["running"].as_str())
            .unwrap_or("/articles")
            .to_string(),
        _ => "/articles".to_string(),
    };
    let existing = match client
        .json(Method::GET, "/api/v1/redirects?limit=1", None)
        .await
    {
        Ok((StatusCode::OK, body)) => body["total"].as_u64().unwrap_or(0) as usize,
        _ => 0,
    };
    let redirects = plan::plan_redirects(items, &prefix, MAX_REDIRECTS.saturating_sub(existing));
    println!(
        "Creating redirects… {}",
        redirects.exact.len() + redirects.patterns.len()
    );
    for (from, to) in redirects.patterns.iter().chain(&redirects.exact) {
        let body = json!({ "from": from, "to": to, "status": 301 });
        match client
            .json(Method::POST, "/api/v1/redirects", Some(&body))
            .await
        {
            Ok((StatusCode::CREATED, _)) => report.created("redirect"),
            Ok((StatusCode::CONFLICT, _)) => report.existing("redirect"),
            Ok((status, body)) => report.failed("redirect", from, &error_of(status, &body)),
            Err(e) => report.failed("redirect", from, &e),
        }
    }
    redirect_notes(&redirects, report);
}

/// 投稿・固定ページのコメントを、状態・日時・返信の関係ごと取り込む
async fn import_comments(
    client: &Client,
    item: &WpItem,
    ty: &str,
    slug: &str,
    subject: &str,
    report: &mut Report,
) {
    let comments = importable_comments(item);
    let skipped_kinds = item.comments.len() - comments.len();
    if skipped_kinds > 0 {
        report.note(
            "comment",
            subject,
            &format!("{skipped_kinds} pingbacks and trackbacks are not imported"),
        );
    }
    let path = format!(
        "/api/v1/contents/{ty}/{}/comments/import",
        encode_segment(slug)
    );
    for chunk in comments.chunks(500) {
        let items: Vec<Value> = chunk
            .iter()
            .map(|c| {
                json!({
                    "ref": c.id.to_string(),
                    "parent_ref": (c.parent != 0).then(|| c.parent.to_string()),
                    "name": c.author,
                    "email": c.author_email,
                    "body": plan::comment_text(&c.content),
                    "status": plan::comment_status(&c.approved),
                    "created_at": c.date.map(|d| d.to_string()),
                    "ip": c.author_ip,
                })
            })
            .collect();
        match client
            .json(Method::POST, &path, Some(&json!({ "comments": items })))
            .await
        {
            Ok((StatusCode::OK, body)) => {
                for _ in 0..body["imported"].as_u64().unwrap_or(0) {
                    report.created("comment");
                }
                for entry in body["skipped"].as_array().into_iter().flatten() {
                    report.skipped(
                        "comment",
                        &format!(
                            "{subject} comment #{}",
                            entry["ref"].as_str().unwrap_or("?")
                        ),
                        entry["reason"].as_str().unwrap_or("not imported"),
                    );
                }
            }
            Ok((StatusCode::NOT_FOUND, _)) => {
                report.note(
                    "comment",
                    subject,
                    &format!(
                        "comments are not enabled for '{ty}': enable [comments] to import them"
                    ),
                );
                return;
            }
            Ok((status, body)) => {
                report.failed("comment", subject, &error_of(status, &body));
                return;
            }
            Err(e) => {
                report.failed("comment", subject, &e);
                return;
            }
        }
    }
}

struct ImportContext<'a> {
    convert: &'a Context<'a>,
    /// 元のログイン名 → ユーザー ID
    authors: &'a HashMap<String, String>,
    /// カテゴリの slug → コンテンツ ID
    categories: &'a HashMap<String, String>,
    /// タグの slug → タグ ID
    tags: &'a HashMap<String, String>,
    media_map: &'a [(String, String)],
}

fn random_password() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// URL のパスの 1 区間として使えるように符号化する
fn encode_segment(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn id_of(value: &Value) -> Option<String> {
    value.get("id").and_then(Value::as_str).map(str::to_string)
}

fn error_of(status: StatusCode, body: &Value) -> String {
    let message = body.get("error").and_then(Value::as_str).unwrap_or("");
    format!("HTTP {} {message}", status.as_u16())
        .trim()
        .to_string()
}

/// 著者をユーザーとして作る。ログインできないよう無効にし、パスワードは推測できない値にする
/// （管理者があとで有効にして、パスワードを設定する）
async fn import_authors(
    client: &Client,
    export: &WxrExport,
    items: &[(&WpItem, &'static str, plan::Status)],
    report: &mut Report,
) -> HashMap<String, String> {
    let used: BTreeSet<&str> = items.iter().map(|(i, _, _)| i.author.as_str()).collect();
    let mut ids = HashMap::new();
    for author in export
        .authors
        .iter()
        .filter(|a| used.contains(a.login.as_str()))
    {
        let username = plan::username(&author.login, author.id);
        let display = if author.display_name.trim().is_empty() {
            author.login.clone()
        } else {
            author.display_name.clone()
        };
        let mut body = json!({
            "username": username,
            "password": random_password(),
            "role": "contributor",
            "display_name": display,
        });
        if !author.email.trim().is_empty() {
            body["email"] = json!(author.email.trim());
        }
        let mut created = client
            .json(Method::POST, "/api/v1/users", Some(&body))
            .await;
        // メールアドレスの重複などで断られたら、メールなしで 1 度だけやり直す
        if matches!(&created, Ok((s, _)) if *s == StatusCode::BAD_REQUEST) {
            body.as_object_mut().map(|o| o.remove("email"));
            created = client
                .json(Method::POST, "/api/v1/users", Some(&body))
                .await;
        }
        match created {
            Ok((StatusCode::CREATED, user)) => {
                if let Some(id) = id_of(&user) {
                    let _ = client
                        .json(
                            Method::PATCH,
                            &format!("/api/v1/users/{}", encode_segment(&username)),
                            Some(&json!({ "disabled": true })),
                        )
                        .await;
                    ids.insert(author.login.clone(), id);
                    report.created("author");
                }
            }
            Ok(_) => {
                // すでにある（再実行）
                let found = client
                    .json(
                        Method::GET,
                        &format!("/api/v1/users/{}", encode_segment(&username)),
                        None,
                    )
                    .await;
                match found {
                    Ok((StatusCode::OK, user)) if id_of(&user).is_some() => {
                        ids.insert(author.login.clone(), id_of(&user).unwrap_or_default());
                        report.existing("author");
                    }
                    Ok((status, body)) => {
                        report.failed("author", &author.login, &error_of(status, &body))
                    }
                    Err(e) => report.failed("author", &author.login, &e),
                }
            }
            Err(e) => report.failed("author", &author.login, &e),
        }
    }
    if !ids.is_empty() {
        report.note(
            "author",
            &format!("{} users", ids.len()),
            "created disabled with a random password: enable them and set a password to let them log in",
        );
    }
    ids
}

/// 既存の項目の ID を取る
async fn existing_content_id(client: &Client, ty: &str, slug: &str) -> Option<String> {
    let path = format!("/api/v1/contents/{ty}/{}", encode_segment(slug));
    match client.json(Method::GET, &path, None).await {
        Ok((StatusCode::OK, body)) => id_of(&body),
        _ => None,
    }
}

async fn import_categories(
    client: &Client,
    export: &WxrExport,
    report: &mut Report,
) -> HashMap<String, String> {
    let mut ids = HashMap::new();
    for term in &export.categories {
        let slug = {
            let decoded = plan::percent_decode(&term.slug);
            if decoded.trim().is_empty() {
                format!("wp-category-{}", term.id)
            } else {
                decoded
            }
        };
        let body = json!({
            "title": term.name,
            "slug": slug,
            "status": "published",
            "fields": { "description": term.description },
        });
        match client
            .json(Method::POST, "/api/v1/contents/category", Some(&body))
            .await
        {
            Ok((StatusCode::CREATED, created)) => {
                if let Some(id) = id_of(&created) {
                    ids.insert(term.slug.clone(), id);
                    report.created("category");
                }
            }
            Ok((StatusCode::CONFLICT, _)) => {
                match existing_content_id(client, "category", &slug).await {
                    Some(id) => {
                        ids.insert(term.slug.clone(), id);
                        report.existing("category");
                    }
                    None => report.failed("category", &term.name, "already exists but not found"),
                }
            }
            Ok((status, body)) => report.failed("category", &term.name, &error_of(status, &body)),
            Err(e) => report.failed("category", &term.name, &e),
        }
        if !term.parent.is_empty() {
            report.note(
                "category",
                &term.name,
                "parent category is dropped: categories are flat",
            );
        }
    }
    ids
}

async fn import_tags(
    client: &Client,
    export: &WxrExport,
    report: &mut Report,
) -> HashMap<String, String> {
    let mut ids = HashMap::new();
    for term in &export.tags {
        let slug = plan::term_slug(term).unwrap_or_else(|| format!("wp-tag-{}", term.id));
        let body = json!({ "name": term.name, "slug": slug, "description": term.description });
        match client.json(Method::POST, "/api/v1/tags", Some(&body)).await {
            Ok((StatusCode::CREATED, created)) => {
                if let Some(id) = id_of(&created) {
                    ids.insert(term.slug.clone(), id);
                    report.created("tag");
                }
            }
            Ok((StatusCode::CONFLICT, _)) => {
                let found = client
                    .json(
                        Method::GET,
                        &format!("/api/v1/tags?q={}&limit=50", encode_segment(&term.name)),
                        None,
                    )
                    .await;
                let id = found.ok().and_then(|(_, body)| {
                    body["tags"]
                        .as_array()?
                        .iter()
                        .find(|t| t["slug"] == slug || t["name"] == term.name.as_str())
                        .and_then(id_of)
                });
                match id {
                    Some(id) => {
                        ids.insert(term.slug.clone(), id);
                        report.existing("tag");
                    }
                    None => report.failed("tag", &term.name, "already exists but not found"),
                }
            }
            Ok((status, body)) => report.failed("tag", &term.name, &error_of(status, &body)),
            Err(e) => report.failed("tag", &term.name, &e),
        }
    }
    ids
}

/// 保存先のキー。アップロードのフォルダ（`wp-content/uploads/2020/05/a.jpg`）の形を保つ
fn media_key(item: &WpItem, url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let tail = match path.split_once("/wp-content/uploads/") {
        Some((_, tail)) => tail.to_string(),
        None => {
            let name = path.rsplit('/').next().unwrap_or("file");
            format!("{}-{}", item.id, name)
        }
    };
    let decoded = plan::percent_decode(&tail);
    let safe: String = decoded
        .replace("..", ".")
        .chars()
        .map(|c| {
            if c == ':' || c == '\\' || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    format!("wordpress/{}", safe.trim_start_matches('/'))
}

fn guess_content_type(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "mp4" => "video/mp4",
        "mp3" => "audio/mpeg",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}

/// 添付ファイルを元のサイトから取って、アップロードする。
/// 戻り値は（添付 ID → 新しい URL、元の URL → 新しい URL の一覧）
async fn import_media(
    client: &Client,
    export: &WxrExport,
    report: &mut Report,
) -> (HashMap<u64, String>, Vec<(String, String)>) {
    let mut by_id = HashMap::new();
    let mut map = Vec::new();
    let attachments: Vec<&WpItem> = export
        .items
        .iter()
        .filter(|i| i.post_type == "attachment" && i.attachment_url.is_some())
        .collect();
    let total = attachments.len();
    for (n, item) in attachments.into_iter().enumerate() {
        if n % 10 == 0 {
            println!("Importing media… {n}/{total}");
        }
        let Some(url) = item.attachment_url.as_deref() else {
            continue;
        };
        let key = media_key(item, url);
        let file_name = key.rsplit('/').next().unwrap_or("file").to_string();
        let (data, served_type) = match client.download(url).await {
            Ok(downloaded) => downloaded,
            Err(e) => {
                report.failed("media", url, &format!("download failed: {e}"));
                continue;
            }
        };
        let content_type = served_type
            .map(|t| t.split(';').next().unwrap_or("").trim().to_string())
            .filter(|t| !t.is_empty() && t != "text/html")
            .unwrap_or_else(|| guess_content_type(&file_name).to_string());
        match client.upload(&key, &file_name, &content_type, data).await {
            Ok((StatusCode::CREATED | StatusCode::OK, meta)) => {
                if let Some(new_url) = meta.get("url").and_then(Value::as_str) {
                    by_id.insert(item.id, new_url.to_string());
                    // 本文には http と https の両方で書かれていることがある
                    map.push((url.to_string(), new_url.to_string()));
                    if let Some(rest) = url.strip_prefix("https://") {
                        map.push((format!("http://{rest}"), new_url.to_string()));
                    } else if let Some(rest) = url.strip_prefix("http://") {
                        map.push((format!("https://{rest}"), new_url.to_string()));
                    }
                    report.created("media");
                }
            }
            Ok((status, body)) => report.failed("media", url, &error_of(status, &body)),
            Err(e) => report.failed("media", url, &e),
        }
    }
    (by_id, map)
}

async fn import_item(
    client: &Client,
    item: &WpItem,
    ty: &'static str,
    status: &plan::Status,
    ctx: &ImportContext<'_>,
    report: &mut Report,
) {
    let subject = describe(item);
    let conversion = convert(&item.content, ctx.convert);
    for note in &conversion.notes {
        report.note(ty, &subject, &format!("{}: {}", note.code, note.detail));
    }
    let mut blocks = serde_json::to_value(&conversion.blocks).unwrap_or_else(|_| json!([]));
    plan::rewrite_json(&mut blocks, ctx.media_map);

    let slug = plan::item_slug(item);
    let title = if item.title.trim().is_empty() {
        "(untitled)".to_string()
    } else {
        item.title.trim().to_string()
    };
    let mut body = json!({
        "title": title,
        "slug": slug,
        "status": status.status,
        "blocks": blocks,
    });
    if let Some(at) = status.published_at {
        if status.status == "published" || status.status == "scheduled" {
            body["published_at"] = json!(at.to_string());
        }
    }
    if let Some(id) = ctx.authors.get(&item.author) {
        body["author_id"] = json!(id);
    }
    let mut fields = serde_json::Map::new();
    if ty == "article" {
        if let Some(excerpt) = plan::excerpt_text(&item.excerpt) {
            fields.insert("excerpt".to_string(), json!(excerpt));
        }
        let (category, extra) = plan::pick_category(item);
        if let Some(id) = category.and_then(|slug| ctx.categories.get(slug)) {
            fields.insert("category_id".to_string(), json!(id));
        }
        if !extra.is_empty() {
            report.note(
                ty,
                &subject,
                &format!(
                    "only the first category is kept; dropped: {}",
                    extra.join(", ")
                ),
            );
        }
    }
    if !fields.is_empty() {
        body["fields"] = Value::Object(fields);
    }

    match client
        .json(Method::POST, &format!("/api/v1/contents/{ty}"), Some(&body))
        .await
    {
        Ok((StatusCode::CREATED, _)) => report.created(ty),
        Ok((StatusCode::CONFLICT, _)) => {
            report.existing(ty);
            return;
        }
        Ok((status, body)) => {
            report.failed(ty, &subject, &error_of(status, &body));
            return;
        }
        Err(e) => {
            report.failed(ty, &subject, &e);
            return;
        }
    }

    if status.status == "published" {
        if !item.comments.is_empty() {
            import_comments(client, item, ty, &slug, &subject, report).await;
        }
    } else if !importable_comments(item).is_empty() {
        report.note(
            "comment",
            &subject,
            "comments are not imported: the post is not published",
        );
    }

    if ty == "article" {
        let tag_ids: Vec<&String> = item
            .term_slugs("post_tag")
            .filter_map(|slug| ctx.tags.get(slug))
            .collect();
        if !tag_ids.is_empty() {
            let path = format!("/api/v1/contents/article/{}/tags", encode_segment(&slug));
            match client
                .json(Method::PUT, &path, Some(&json!({ "tag_ids": tag_ids })))
                .await
            {
                Ok((StatusCode::OK, _)) => {}
                Ok((status, body)) => report.note(
                    ty,
                    &subject,
                    &format!("tags not set: {}", error_of(status, &body)),
                ),
                Err(e) => report.note(ty, &subject, &format!("tags not set: {e}")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attachment(id: u64) -> WpItem {
        WpItem {
            id,
            post_type: "attachment".to_string(),
            ..WpItem::default()
        }
    }

    #[test]
    fn media_keys_keep_the_upload_folders() {
        let item = attachment(9);
        assert_eq!(
            media_key(
                &item,
                "https://old.example/wp-content/uploads/2020/05/cat%20pic.jpg?x=1"
            ),
            "wordpress/2020/05/cat pic.jpg"
        );
        assert_eq!(
            media_key(&item, "https://cdn.example/images/a.png"),
            "wordpress/9-a.png"
        );
        let nasty = media_key(
            &item,
            "https://o.example/wp-content/uploads/../../etc:passwd",
        );
        assert!(!nasty.contains("..") && !nasty.contains(':'), "{nasty}");
    }

    #[test]
    fn path_segments_are_encoded() {
        assert_eq!(encode_segment("abc-1.2_x~"), "abc-1.2_x~");
        assert_eq!(encode_segment("あ b/c"), "%E3%81%82%20b%2Fc");
    }

    #[test]
    fn content_types_are_guessed_from_the_extension() {
        assert_eq!(guess_content_type("A.JPG"), "image/jpeg");
        assert_eq!(guess_content_type("x.unknown"), "application/octet-stream");
    }

    #[test]
    fn a_dry_run_reports_without_a_server() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:excerpt="http://wordpress.org/export/1.2/excerpt/"
 xmlns:content="http://purl.org/rss/1.0/modules/content/"
 xmlns:wp="http://wordpress.org/export/1.2/" xmlns:dc="http://purl.org/dc/elements/1.1/">
<channel><title>Old</title><wp:wxr_version>1.2</wp:wxr_version>
<item><title>Hello</title><dc:creator>admin</dc:creator>
<content:encoded><![CDATA[<p>Hi [mystery id="1"]</p>]]></content:encoded>
<wp:post_id>1</wp:post_id><wp:post_name>hello</wp:post_name><wp:status>publish</wp:status><wp:post_type>post</wp:post_type></item>
<item><title>Shop</title><wp:post_id>2</wp:post_id><wp:status>publish</wp:status><wp:post_type>product</wp:post_type><content:encoded></content:encoded></item>
<item><title>Gone</title><wp:post_id>3</wp:post_id><wp:status>trash</wp:status><wp:post_type>post</wp:post_type><content:encoded></content:encoded></item>
</channel></rss>"#;
        let export = parse_wxr(xml).unwrap();
        let mut report = Report::default();
        dry_run(&export, &mut report);
        assert_eq!(report.counts()["article"].created, 1);
        assert_eq!(report.counts()["other"].skipped, 1);
        assert!(!report.counts().contains_key("page"));
        let text = report.render(true);
        assert!(text.contains("shortcode"), "{text}");
        assert!(
            text.contains("post type 'product' is not imported"),
            "{text}"
        );
    }
}
