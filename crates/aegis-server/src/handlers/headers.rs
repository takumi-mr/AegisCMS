use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use crate::audit::AuditContext;
use crate::auth::authorize::RequireRole;
use crate::auth::AdminSession;
use crate::headers::{
    self, CustomHeader, HeaderDefaults, HeaderOverrides, COOP_SETTING, CSP_SETTING,
    CUSTOM_HEADERS_SETTING, EMBED_FRAME_DOMAINS_SETTING, MAX_CUSTOM_HEADERS_COUNT,
    PERMISSIONS_POLICY_SETTING, REFERRER_POLICY_SETTING, X_CONTENT_TYPE_OPTIONS_SETTING,
    X_FRAME_OPTIONS_SETTING,
};
use crate::state::AppState;
use aegis_core::{AuditAction, AuditActor, AuditOutcome};

type ApiError = (StatusCode, Json<serde_json::Value>);

/// `GET` / `PUT /api/v1/headers` のレスポンス
#[derive(Debug, Serialize)]
pub struct HeadersSettingsResponse {
    pub content_security_policy: Option<String>,
    pub x_frame_options: Option<String>,
    pub x_content_type_options: Option<String>,
    pub referrer_policy: Option<String>,
    pub permissions_policy: Option<String>,
    pub cross_origin_opener_policy: Option<String>,
    pub custom_headers: Vec<CustomHeader>,
    /// 管理者が許可する埋め込みのホスト（Admin Studio で設定したもの）
    pub embed_frame_domains: Vec<String>,
    /// 設定ファイルの `[embed].frame_domains`（読み取り専用。Admin Studio の値と合わせて使う）
    pub config_embed_frame_domains: Vec<String>,
    /// 許可したホストを受け入れるプラグイン（`capabilities.embed.site_domains`）が有効か。
    /// 無効なら、許可したホストは本文にも CSP にも効かない
    pub embed_frame_domains_in_use: bool,
    pub defaults: HeaderDefaults,
}

/// `PUT /api/v1/headers` のリクエスト本文
#[derive(Debug, Default, Deserialize)]
pub struct UpdateHeadersSettings {
    #[serde(default)]
    pub content_security_policy: Option<String>,
    #[serde(default)]
    pub x_frame_options: Option<String>,
    #[serde(default)]
    pub x_content_type_options: Option<String>,
    #[serde(default)]
    pub referrer_policy: Option<String>,
    #[serde(default)]
    pub permissions_policy: Option<String>,
    #[serde(default)]
    pub cross_origin_opener_policy: Option<String>,
    #[serde(default)]
    pub custom_headers: Option<Vec<CustomHeader>>,
    /// 省略すると今の値のまま（空の配列で消す）
    #[serde(default)]
    pub embed_frame_domains: Option<Vec<String>>,
}

async fn response(state: &AppState, overrides: HeaderOverrides) -> HeadersSettingsResponse {
    let in_use = state.supervisor.read().await.accepts_site_frame_domains();
    HeadersSettingsResponse {
        content_security_policy: overrides.content_security_policy,
        x_frame_options: overrides.x_frame_options,
        x_content_type_options: overrides.x_content_type_options,
        referrer_policy: overrides.referrer_policy,
        permissions_policy: overrides.permissions_policy,
        cross_origin_opener_policy: overrides.cross_origin_opener_policy,
        custom_headers: overrides.custom_headers,
        embed_frame_domains: overrides.embed_frame_domains,
        config_embed_frame_domains: state.config.embed.frame_domains.clone(),
        embed_frame_domains_in_use: in_use,
        defaults: HeaderDefaults::default(),
    }
}

fn bad_request(message: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": message })))
}

fn normalize_opt(val: Option<String>) -> Option<String> {
    val.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// `GET /api/v1/headers` — 共通ヘッダーおよび防御ヘッダー設定の取得（admin 必須）
pub async fn get_headers_settings(
    State(state): State<Arc<AppState>>,
    RequireRole(_session): AdminSession,
) -> Json<HeadersSettingsResponse> {
    let overrides = state.headers();
    Json(response(&state, overrides).await)
}

/// `PUT /api/v1/headers` — 共通ヘッダーおよび防御ヘッダー設定の更新（admin 必須）
pub async fn update_headers_settings(
    State(state): State<Arc<AppState>>,
    RequireRole(session): AdminSession,
    audit_ctx: AuditContext,
    request_headers: axum::http::HeaderMap,
    Json(payload): Json<UpdateHeadersSettings>,
) -> Result<Json<HeadersSettingsResponse>, ApiError> {
    let csp = normalize_opt(payload.content_security_policy);
    let x_frame = normalize_opt(payload.x_frame_options);
    let x_content_type = normalize_opt(payload.x_content_type_options);
    let referrer = normalize_opt(payload.referrer_policy);
    let permissions = normalize_opt(payload.permissions_policy);
    let coop = normalize_opt(payload.cross_origin_opener_policy);

    // 単体フィールドのバリデーション
    if let Some(ref v) = csp {
        headers::validate_header_value(v).map_err(bad_request)?;
    }
    if let Some(ref v) = x_frame {
        headers::validate_header_value(v).map_err(bad_request)?;
    }
    if let Some(ref v) = x_content_type {
        headers::validate_header_value(v).map_err(bad_request)?;
    }
    if let Some(ref v) = referrer {
        headers::validate_header_value(v).map_err(bad_request)?;
    }
    if let Some(ref v) = permissions {
        headers::validate_header_value(v).map_err(bad_request)?;
    }
    if let Some(ref v) = coop {
        headers::validate_header_value(v).map_err(bad_request)?;
    }

    // カスタムヘッダーのバリデーション
    let mut custom_headers = Vec::new();
    if let Some(headers_list) = payload.custom_headers {
        if headers_list.len() > MAX_CUSTOM_HEADERS_COUNT {
            return Err(bad_request(&format!(
                "Too many custom headers (maximum is {})",
                MAX_CUSTOM_HEADERS_COUNT
            )));
        }
        for h in headers_list {
            let name = h.name.trim().to_string();
            let value = h.value.trim().to_string();
            if name.is_empty() && value.is_empty() {
                continue;
            }
            headers::validate_header_name(&name).map_err(bad_request)?;
            headers::validate_header_value(&value).map_err(bad_request)?;
            custom_headers.push(CustomHeader { name, value });
        }
    }

    // 埋め込みを許可するホスト（小文字にそろえ、重複を除く。形式が違えば 400）
    let embed_frame_domains = match payload.embed_frame_domains {
        None => state.headers().embed_frame_domains,
        Some(list) => {
            let mut hosts: Vec<String> = list
                .iter()
                .map(|h| h.trim().to_ascii_lowercase())
                .filter(|h| !h.is_empty())
                .collect();
            hosts.sort();
            hosts.dedup();
            crate::embeds::validate_hosts("embed_frame_domains", &hosts)
                .map_err(|m| bad_request(&m))?;
            // このサイト自身（正規 URL と、このリクエストの Host）は許可できない
            let request_host = request_headers
                .get(axum::http::header::HOST)
                .and_then(|v| v.to_str().ok())
                .and_then(|h| crate::embeds::url_host(&format!("http://{h}")));
            let own = [crate::embeds::own_host(&state), request_host];
            if let Some(host) = hosts.iter().find(|h| own.iter().flatten().any(|o| o == *h)) {
                return Err(bad_request(&format!(
                    "embed_frame_domains: {host:?} is this site's own host; a same-origin page in the generic embed could remove its sandbox"
                )));
            }
            hosts
        }
    };

    let next = HeaderOverrides {
        content_security_policy: csp,
        x_frame_options: x_frame,
        x_content_type_options: x_content_type,
        referrer_policy: referrer,
        permissions_policy: permissions,
        cross_origin_opener_policy: coop,
        custom_headers,
        embed_frame_domains,
    };

    let previous = state.headers();
    let actor = Some(AuditActor {
        user_id: session.user_id,
        role: session.role,
    });

    let custom_json = if next.custom_headers.is_empty() {
        None
    } else {
        match serde_json::to_string(&next.custom_headers) {
            Ok(s) => Some(s),
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("Serialization error: {}", e) })),
                ));
            }
        }
    };

    let embed_json = if next.embed_frame_domains.is_empty() {
        None
    } else {
        serde_json::to_string(&next.embed_frame_domains).ok()
    };

    let saved = async {
        let pairs: [(&str, &Option<String>); 8] = [
            (CSP_SETTING, &next.content_security_policy),
            (X_FRAME_OPTIONS_SETTING, &next.x_frame_options),
            (X_CONTENT_TYPE_OPTIONS_SETTING, &next.x_content_type_options),
            (REFERRER_POLICY_SETTING, &next.referrer_policy),
            (PERMISSIONS_POLICY_SETTING, &next.permissions_policy),
            (COOP_SETTING, &next.cross_origin_opener_policy),
            (CUSTOM_HEADERS_SETTING, &custom_json),
            (EMBED_FRAME_DOMAINS_SETTING, &embed_json),
        ];

        for (key, val) in pairs {
            match val {
                Some(v) => state.db.set_setting(key, v).await?,
                None => state.db.delete_setting(key).await?,
            }
        }
        Ok::<_, aegis_core::AegisError>(())
    }
    .await;

    if let Err(e) = saved {
        tracing::error!("Failed to save header settings: {}", e);
        state
            .audit
            .record(
                audit_ctx
                    .event(AuditAction::HeadersUpdate, AuditOutcome::Failure)
                    .with_actor(actor)
                    .with_details(json!({ "error": e.to_string() })),
            )
            .await;
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to save header settings" })),
        ));
    }

    *state
        .headers
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = next.clone();

    state
        .audit
        .record(
            audit_ctx
                .event(AuditAction::HeadersUpdate, AuditOutcome::Success)
                .with_actor(actor)
                .with_details(json!({
                    "previous": serde_json::to_value(&previous).unwrap_or_default(),
                    "updated": serde_json::to_value(&next).unwrap_or_default(),
                })),
        )
        .await;

    // セキュリティ・カスタムレスポンスヘッダーの変更をエッジにも反映させるためCDNキャッシュをパージ
    crate::cdn::purge_site_global(&state).await;

    Ok(Json(response(&state, next).await))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AegisConfig;
    use crate::test_support::test_state_with_config;
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

    #[tokio::test]
    async fn test_get_and_update_headers_settings() {
        let state = test_state_with_config(AegisConfig::default(), PluginSupervisor::new()).await;

        let Json(initial) = get_headers_settings(State(state.clone()), admin()).await;
        assert_eq!(initial.content_security_policy, None);
        assert_eq!(initial.x_frame_options, None);
        assert_eq!(initial.defaults.x_frame_options, "SAMEORIGIN");
        assert_eq!(initial.defaults.x_content_type_options, "nosniff");

        // 更新
        let update_payload = UpdateHeadersSettings {
            content_security_policy: Some("default-src 'self'".to_string()),
            x_frame_options: Some("DENY".to_string()),
            x_content_type_options: None,
            referrer_policy: Some("no-referrer".to_string()),
            permissions_policy: None,
            cross_origin_opener_policy: None,
            custom_headers: Some(vec![CustomHeader {
                name: "X-Custom-Header".to_string(),
                value: "CustomValue".to_string(),
            }]),
            embed_frame_domains: Some(vec![
                " Forms.Example.com ".to_string(),
                "forms.example.com".to_string(),
                "".to_string(),
            ]),
        };

        let Json(updated) = update_headers_settings(
            State(state.clone()),
            admin(),
            AuditContext::default(),
            axum::http::HeaderMap::new(),
            Json(update_payload),
        )
        .await
        .unwrap();

        assert_eq!(
            updated.content_security_policy.as_deref(),
            Some("default-src 'self'")
        );
        assert_eq!(updated.x_frame_options.as_deref(), Some("DENY"));
        assert_eq!(updated.referrer_policy.as_deref(), Some("no-referrer"));
        assert_eq!(updated.custom_headers.len(), 1);
        assert_eq!(updated.custom_headers[0].name, "X-Custom-Header");
        // 小文字にそろえ、重複と空を除く
        assert_eq!(updated.embed_frame_domains, ["forms.example.com"]);
        assert!(!updated.embed_frame_domains_in_use);

        // state.headers() に反映されていること
        let current = state.headers();
        assert_eq!(
            current.content_security_policy.as_deref(),
            Some("default-src 'self'")
        );
        assert_eq!(current.x_frame_options.as_deref(), Some("DENY"));
        assert_eq!(current.custom_headers.len(), 1);

        // DB からの load でも取得できること
        let loaded = HeaderOverrides::load(state.db.as_ref()).await;
        assert_eq!(
            loaded.content_security_policy.as_deref(),
            Some("default-src 'self'")
        );
        assert_eq!(loaded.x_frame_options.as_deref(), Some("DENY"));
        assert_eq!(loaded.custom_headers.len(), 1);
        assert_eq!(loaded.embed_frame_domains, ["forms.example.com"]);

        // 空にしてリセット
        let reset_payload = UpdateHeadersSettings {
            content_security_policy: Some("".to_string()),
            x_frame_options: None,
            x_content_type_options: None,
            referrer_policy: None,
            permissions_policy: None,
            cross_origin_opener_policy: None,
            custom_headers: Some(vec![]),
            embed_frame_domains: None,
        };
        let Json(reset) = update_headers_settings(
            State(state.clone()),
            admin(),
            AuditContext::default(),
            axum::http::HeaderMap::new(),
            Json(reset_payload),
        )
        .await
        .unwrap();
        assert_eq!(reset.content_security_policy, None);
        assert_eq!(reset.x_frame_options, None);
        assert!(reset.custom_headers.is_empty());
        // 送らなければ今の値のまま（古いクライアントが消してしまわないように）
        assert_eq!(reset.embed_frame_domains, ["forms.example.com"]);

        // 空の配列で消す。形式の違うホストは 400
        let clear = |hosts: Vec<&str>| UpdateHeadersSettings {
            embed_frame_domains: Some(hosts.into_iter().map(str::to_string).collect()),
            ..Default::default()
        };
        let Json(cleared) = update_headers_settings(
            State(state.clone()),
            admin(),
            AuditContext::default(),
            axum::http::HeaderMap::new(),
            Json(clear(vec![])),
        )
        .await
        .unwrap();
        assert!(cleared.embed_frame_domains.is_empty());
        assert!(HeaderOverrides::load(state.db.as_ref())
            .await
            .embed_frame_domains
            .is_empty());
        for bad in [
            "https://a.example",
            "*.example.com",
            "a.example:8443",
            "a.example/x",
        ] {
            let result = update_headers_settings(
                State(state.clone()),
                admin(),
                AuditContext::default(),
                axum::http::HeaderMap::new(),
                Json(clear(vec![bad])),
            )
            .await;
            assert_eq!(result.unwrap_err().0, StatusCode::BAD_REQUEST, "{}", bad);
        }
    }

    #[tokio::test]
    async fn test_the_sites_own_host_cannot_be_allowed_for_embeds() {
        let state = test_state_with_config(AegisConfig::default(), PluginSupervisor::new()).await;
        state.seo.write().unwrap().canonical_base_url = Some("https://Example.com/cms".to_string());
        let put = |hosts: &[&str], host_header: Option<&str>| {
            let state = state.clone();
            let hosts: Vec<String> = hosts.iter().map(|h| h.to_string()).collect();
            let mut headers = axum::http::HeaderMap::new();
            if let Some(h) = host_header {
                headers.insert(axum::http::header::HOST, h.parse().unwrap());
            }
            async move {
                update_headers_settings(
                    State(state),
                    admin(),
                    AuditContext::default(),
                    headers,
                    Json(UpdateHeadersSettings {
                        embed_frame_domains: Some(hosts),
                        ..Default::default()
                    }),
                )
                .await
            }
        };
        // 正規 URL のホスト
        assert_eq!(
            put(&["example.com"], None).await.unwrap_err().0,
            StatusCode::BAD_REQUEST
        );
        // このリクエストの Host（ポートつきでも）
        assert_eq!(
            put(&["admin.example.net"], Some("admin.example.net:8443"))
                .await
                .unwrap_err()
                .0,
            StatusCode::BAD_REQUEST
        );
        // ほかのホストは許可できる
        assert!(put(&["forms.example.com"], Some("example.com"))
            .await
            .is_ok());

        // 設定ファイル・DB に入っていても、正規 URL のホストは効かない
        state
            .headers
            .write()
            .unwrap()
            .embed_frame_domains
            .push("example.com".to_string());
        assert_eq!(
            crate::embeds::site_frame_domains(&state),
            ["forms.example.com"]
        );
    }

    #[tokio::test]
    async fn test_invalid_header_rejection() {
        let state = test_state_with_config(AegisConfig::default(), PluginSupervisor::new()).await;

        // CRLF in value
        let bad_val = UpdateHeadersSettings {
            x_frame_options: Some("DENY\r\nSet-Cookie: evil=1".to_string()),
            ..Default::default()
        };
        let res = update_headers_settings(
            State(state.clone()),
            admin(),
            AuditContext::default(),
            axum::http::HeaderMap::new(),
            Json(bad_val),
        )
        .await;
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().0, StatusCode::BAD_REQUEST);

        // Invalid name in custom headers
        let bad_name = UpdateHeadersSettings {
            custom_headers: Some(vec![CustomHeader {
                name: "Bad Name with Space".to_string(),
                value: "val".to_string(),
            }]),
            ..Default::default()
        };
        let res2 = update_headers_settings(
            State(state.clone()),
            admin(),
            AuditContext::default(),
            axum::http::HeaderMap::new(),
            Json(bad_name),
        )
        .await;
        assert!(res2.is_err());
        assert_eq!(res2.unwrap_err().0, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_router_applies_security_headers() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        let state = test_state_with_config(AegisConfig::default(), PluginSupervisor::new()).await;
        let app = crate::app::router(&state.config, state.clone());

        // GET /health
        let req = Request::builder()
            .uri("/health")
            .method("GET")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let headers = resp.headers();
        assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
        assert_eq!(headers.get("x-frame-options").unwrap(), "SAMEORIGIN");
        assert_eq!(
            headers.get("referrer-policy").unwrap(),
            "strict-origin-when-cross-origin"
        );
        assert!(headers.contains_key("content-security-policy"));
        assert!(headers.contains_key("permissions-policy"));
        assert_eq!(
            headers.get("cross-origin-opener-policy").unwrap(),
            "same-origin"
        );
    }
}
