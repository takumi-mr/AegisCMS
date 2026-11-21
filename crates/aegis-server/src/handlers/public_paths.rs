//! 型ごとの公開 URL の先頭（スキーマの `public_path`）を Admin Studio から見る・変える（admin 権限）。
//! 変更は DB に保存し、サーバーの再起動で反映する（[`crate::public_paths`]）

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

use crate::audit::AuditContext;
use crate::auth::authorize::RequireRole;
use crate::auth::AdminSession;
use crate::public_paths::{is_fixed_type, normalize_input, setting_name, validate_configured};
use crate::state::AppState;
use aegis_core::{AuditAction, AuditActor, AuditOutcome};

type ApiError = (StatusCode, Json<serde_json::Value>);

/// 記事の既定の URL（スキーマに `public_path` を書かなかったとき）
const ARTICLE_DEFAULT: &str = "/articles";

/// 1 つの型の公開 URL の先頭
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PublicPathEntry {
    pub content_type: String,
    pub label: String,
    /// いまサーバーが使っている値（公開ページを持たない型は `null`）
    pub running: Option<String>,
    /// スキーマファイル（`schemas/*.yaml`）の値。記事は書いていなければ既定の `/articles`
    pub file: Option<String>,
    /// Admin Studio で設定した値（`null` はスキーマファイルの値を使う）
    #[serde(rename = "override")]
    pub override_path: Option<String>,
    /// 再起動したあとに使われる値
    pub configured: Option<String>,
    /// `configured` が `running` と違う（再起動で変わる）
    pub restart_required: bool,
}

/// `PUT /api/v1/public-paths/{content_type}` の本文。`null` または空文字は上書きを消す。
/// `auto` は `/<型名>`
#[derive(Debug, Deserialize)]
pub struct UpdatePublicPath {
    #[serde(default)]
    pub public_path: Option<String>,
}

fn error(status: StatusCode, message: &str) -> ApiError {
    (status, Json(json!({ "error": message })))
}

/// 記事だけ、書いていなければ `/articles`
fn with_article_default(content_type: &str, path: Option<String>) -> Option<String> {
    match (content_type, path) {
        ("article", None) => Some(ARTICLE_DEFAULT.to_string()),
        (_, path) => path,
    }
}

/// スキーマファイルの値（起動時に記録したもの。無ければ、いま動いている値）
fn file_value(state: &AppState, content_type: &str) -> Option<String> {
    match state.public_path_files.get(content_type) {
        Some(path) => path.clone(),
        None => state.public_paths.get(content_type).cloned(),
    }
}

fn override_value(settings: &HashMap<String, String>, content_type: &str) -> Option<String> {
    settings.get(&setting_name(content_type)).cloned()
}

async fn entries(state: &AppState, settings: &HashMap<String, String>) -> Vec<PublicPathEntry> {
    let schemas = state.schemas.read().await;
    let mut out: Vec<PublicPathEntry> = schemas
        .values()
        .filter(|s| !is_fixed_type(&s.content_type, &state.config.commerce.product_type))
        .map(|schema| {
            let content_type = schema.content_type.as_str();
            let file = file_value(state, content_type);
            let override_path = override_value(settings, content_type);
            let configured = override_path.clone().or_else(|| file.clone());
            let running = state.public_paths.get(content_type).cloned();
            let (running, file, configured) = (
                with_article_default(content_type, running),
                with_article_default(content_type, file),
                with_article_default(content_type, configured),
            );
            PublicPathEntry {
                content_type: schema.content_type.clone(),
                label: schema.label.clone(),
                restart_required: configured != running,
                running,
                file,
                override_path,
                configured,
            }
        })
        .collect();
    out.sort_by(|a, b| a.content_type.cmp(&b.content_type));
    out
}

async fn load_settings(state: &AppState) -> Result<HashMap<String, String>, ApiError> {
    state.db.list_settings().await.map_err(|e| {
        tracing::error!("Failed to read the public path settings: {}", e);
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to read the settings",
        )
    })
}

/// `GET /api/v1/public-paths` — 型ごとの公開 URL の先頭（admin 権限必須）
pub async fn list_public_paths(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
) -> Result<Json<Vec<PublicPathEntry>>, ApiError> {
    let settings = load_settings(&state).await?;
    Ok(Json(entries(&state, &settings).await))
}

/// `PUT /api/v1/public-paths/{content_type}` — 公開 URL の先頭を変える（admin 権限必須）。
/// DB に保存し、サーバーの再起動で反映する
pub async fn update_public_path(
    State(state): State<Arc<AppState>>,
    RequireRole(session): AdminSession,
    audit_ctx: AuditContext,
    Path(content_type): Path<String>,
    Json(payload): Json<UpdatePublicPath>,
) -> Result<Json<PublicPathEntry>, ApiError> {
    if is_fixed_type(&content_type, &state.config.commerce.product_type) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "This content type has its own public URL",
        ));
    }
    let next = normalize_input(&content_type, payload.public_path.as_deref());
    let settings = load_settings(&state).await?;

    // 再起動後の各型の値（ほかの型は、いまの上書きかスキーマファイルの値）で検査する
    let validation = {
        let schemas = state.schemas.read().await;
        if !schemas.contains_key(&content_type) {
            return Err(error(StatusCode::NOT_FOUND, "Content type not found"));
        }
        let configured: HashMap<String, Option<String>> = schemas
            .keys()
            .map(|ct| {
                let value = if *ct == content_type {
                    next.clone().or_else(|| file_value(&state, ct))
                } else {
                    override_value(&settings, ct).or_else(|| file_value(&state, ct))
                };
                (ct.clone(), value)
            })
            .collect();
        validate_configured(&schemas, &configured, &content_type)
    };
    validation.map_err(|e| error(StatusCode::BAD_REQUEST, &e))?;

    let previous = override_value(&settings, &content_type);
    let actor = Some(AuditActor {
        user_id: session.user_id,
        role: session.role,
    });
    let saved = match &next {
        Some(value) => {
            state
                .db
                .set_setting(&setting_name(&content_type), value)
                .await
        }
        None => state.db.delete_setting(&setting_name(&content_type)).await,
    };
    if let Err(e) = saved {
        tracing::error!("Failed to save the public path of {}: {}", content_type, e);
        state
            .audit
            .record(
                audit_ctx
                    .event(AuditAction::SiteUpdate, AuditOutcome::Failure)
                    .with_actor(actor)
                    .with_details(json!({
                        "content_type": content_type,
                        "error": e.to_string(),
                    })),
            )
            .await;
        return Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to save the setting",
        ));
    }
    state
        .audit
        .record(
            audit_ctx
                .event(AuditAction::SiteUpdate, AuditOutcome::Success)
                .with_actor(actor)
                .with_details(json!({
                    "content_type": content_type,
                    "public_path": { "from": previous, "to": next },
                })),
        )
        .await;

    let settings = load_settings(&state).await?;
    entries(&state, &settings)
        .await
        .into_iter()
        .find(|e| e.content_type == content_type)
        .map(Json)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "Content type not found"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AegisConfig;
    use aegis_auth::SessionData;
    use aegis_runtime::PluginSupervisor;

    fn admin() -> AdminSession {
        RequireRole(SessionData {
            id: "ses_admin".to_string(),
            user_id: "admin1".to_string(),
            role: "admin".to_string(),
            created_at: String::new(),
            expires_at: String::new(),
            metadata: Default::default(),
        })
    }

    fn schema(content_type: &str, public_path: Option<&str>) -> aegis_schema::SchemaDefinition {
        serde_json::from_value(json!({
            "content_type": content_type,
            "label": content_type.to_uppercase(),
            "public_path": public_path,
            "fields": []
        }))
        .unwrap()
    }

    async fn state() -> Arc<AppState> {
        Arc::new(
            crate::test_support::test_app_state(
                AegisConfig::default(),
                PluginSupervisor::new(),
                vec![
                    schema("article", None),
                    schema("page", None),
                    schema("works", Some("/works")),
                    schema("memo", None),
                ],
            )
            .await,
        )
    }

    async fn update(
        state: &Arc<AppState>,
        content_type: &str,
        value: Option<&str>,
    ) -> Result<Json<PublicPathEntry>, ApiError> {
        update_public_path(
            State(state.clone()),
            admin(),
            AuditContext::default(),
            Path(content_type.to_string()),
            Json(UpdatePublicPath {
                public_path: value.map(str::to_string),
            }),
        )
        .await
    }

    async fn list(state: &Arc<AppState>) -> Vec<PublicPathEntry> {
        list_public_paths(State(state.clone()), admin())
            .await
            .unwrap()
            .0
    }

    #[tokio::test]
    async fn test_list_skips_types_with_their_own_url_and_shows_the_article_default() {
        let state = state().await;
        let entries = list(&state).await;
        let names: Vec<&str> = entries.iter().map(|e| e.content_type.as_str()).collect();
        assert_eq!(names, ["article", "memo", "works"]);

        let article = &entries[0];
        assert_eq!(article.running.as_deref(), Some("/articles"));
        assert_eq!(article.file.as_deref(), Some("/articles"));
        assert_eq!(article.override_path, None);
        assert!(!article.restart_required);
        // 公開ページを持たない型
        assert_eq!(entries[1].running, None);
        assert_eq!(entries[2].running.as_deref(), Some("/works"));
    }

    #[tokio::test]
    async fn test_update_saves_the_override_and_asks_for_a_restart() {
        let state = state().await;

        let saved = update(&state, "article", Some(" /news ")).await.unwrap().0;
        assert_eq!(saved.override_path.as_deref(), Some("/news"));
        assert_eq!(saved.configured.as_deref(), Some("/news"));
        // いま動いている値は変わらない。再起動で変わる
        assert_eq!(saved.running.as_deref(), Some("/articles"));
        assert!(saved.restart_required);
        assert_eq!(
            state.db.list_settings().await.unwrap()["public_path.article"],
            "/news"
        );

        // `auto` は /<型名>。公開ページの無かった型にも付けられる
        let memo = update(&state, "memo", Some("auto")).await.unwrap().0;
        assert_eq!(memo.configured.as_deref(), Some("/memo"));
        assert!(memo.restart_required);

        // 空にすると上書きを消して、スキーマファイルの値に戻る（すでに動いている値と同じなら再起動は要らない）
        let reset = update(&state, "article", None).await.unwrap().0;
        assert_eq!(reset.override_path, None);
        assert_eq!(reset.configured.as_deref(), Some("/articles"));
        assert!(!reset.restart_required);
        assert!(!state
            .db
            .list_settings()
            .await
            .unwrap()
            .contains_key("public_path.article"));
    }

    #[tokio::test]
    async fn test_update_rejects_bad_values_without_saving() {
        let state = state().await;

        for (content_type, value, needle) in [
            ("memo", "/works", "same public_path"),
            ("article", "/api", "reserved"),
            ("memo", "/a/b", "single segment"),
            ("memo", "memo", "single segment"),
        ] {
            let (status, body) = update(&state, content_type, Some(value)).await.unwrap_err();
            assert_eq!(status, StatusCode::BAD_REQUEST, "{value}");
            assert!(
                body.0["error"].as_str().unwrap().contains(needle),
                "{value}"
            );
        }
        let (status, _) = update(&state, "page", Some("/pages")).await.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = update(&state, "missing", Some("/x")).await.unwrap_err();
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(state.db.list_settings().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_a_pending_value_counts_against_other_types() {
        let state = state().await;

        // 記事を /news にしたあとなら、別の型が /articles を取れる（再起動後はぶつからない）
        let _ = update(&state, "article", Some("/news")).await.unwrap();
        assert!(update(&state, "memo", Some("/articles")).await.is_ok());
        // ただし、いま予約した /news は別の型には使えない
        let (status, body) = update(&state, "works", Some("/news")).await.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body.0["error"]
            .as_str()
            .unwrap()
            .contains("same public_path"));
    }
}
