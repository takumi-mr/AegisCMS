//! 商品レビュー（Product Review）の API。
//! 購入者（Verified Purchase）による★1〜5の評価、1注文1件の制約、
//! ステマ規制対応（低評価を機械的に非承認としない方針と非承認理由の記録）、
//! 運営者返信、モデレーション、および集計（AggregateRating）を処理する。

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::audit::ClientIp;
use crate::auth::authorize::RequireRole;
use crate::auth::EditorSession;
use crate::handlers::members::resolve_member_session;
use crate::state::AppState;
use aegis_cache::CacheTags;
use aegis_comments::moderation::PolicyOverrides;
use aegis_commerce::{
    JsonLdAggregateRating, PublicReview, ReviewAuthor, ReviewService, ReviewSubmission,
};
use aegis_core::AegisError;
use aegis_db::{ReviewListQuery, ReviewRecord};

type ApiError = (StatusCode, Json<serde_json::Value>);

fn api_error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (status, Json(json!({ "error": message.into() })))
}

fn map_aegis_error(e: AegisError) -> ApiError {
    let status = match e {
        AegisError::Validation(_) => StatusCode::BAD_REQUEST,
        AegisError::Authentication(_) => StatusCode::UNAUTHORIZED,
        AegisError::PermissionDenied(_) => StatusCode::FORBIDDEN,
        AegisError::NotFound(_) => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json(json!({ "error": e.to_string() })))
}

fn reviews(state: &AppState) -> Result<Arc<ReviewService>, ApiError> {
    state
        .reviews
        .clone()
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "Product reviews are not enabled"))
}

#[derive(Debug, Deserialize)]
pub struct PublicReviewListQuery {
    #[serde(default = "default_limit")]
    pub limit: u64,
    #[serde(default)]
    pub offset: u64,
}

fn default_limit() -> u64 {
    50
}

/// `GET /api/v1/products/{product_id}/reviews` — 公開: 承認済みレビュー一覧と集計
pub async fn list_product_reviews(
    State(state): State<Arc<AppState>>,
    Path(product_id): Path<String>,
    Query(query): Query<PublicReviewListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let service = reviews(&state)?;
    let (mut items, stats) = service
        .list_public_reviews(&product_id, Some(query.limit), query.offset)
        .await
        .map_err(map_aegis_error)?;
    // 会員のアバターは、表示のたびに今の値を引く
    let avatars = crate::handlers::members::avatar_urls(
        &state,
        items.iter().filter_map(|r| r.member_id.clone()).collect(),
    )
    .await;
    for review in &mut items {
        review.avatar_url = review
            .member_id
            .as_deref()
            .and_then(|id| avatars.get(id).cloned());
    }

    let json_ld = JsonLdAggregateRating::from_stats(&stats);

    Ok(Json(json!({
        "reviews": items,
        "stats": stats,
        "json_ld": json_ld,
    })))
}

/// `POST /api/v1/products/{product_id}/reviews` — レビュー投稿（購入者のみ・1注文1件）
pub async fn submit_product_review(
    State(state): State<Arc<AppState>>,
    ClientIp(client_ip): ClientIp,
    headers: HeaderMap,
    Path(product_id): Path<String>,
    Json(submission): Json<ReviewSubmission>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let service = reviews(&state)?;
    let user_agent = headers
        .get("user-agent")
        .and_then(|h| h.to_str().ok())
        .map(str::to_string);

    // 1. 顧客ログインがあるかチェック
    let author = if let Ok(session) = resolve_member_session(&state, &headers).await {
        // 公開の場に出る名前は、公開名（あれば）。ログイン時のコピー（session.name）は使わない
        let name = match state.members.as_ref() {
            Some(members) => members
                .get_profile(&session.member_id)
                .await
                .map(|profile| profile.public_name().to_string())
                .unwrap_or_else(|_| session.name.clone()),
            None => session.name.clone(),
        };
        ReviewAuthor::Member {
            member_id: session.member_id,
            email: session.email,
            name,
        }
    } else if let Some(access_token) = &submission.access_token {
        // 2. ゲスト購入（注文トークンによる検証）
        ReviewAuthor::GuestWithToken {
            order_id: submission.order_id.clone(),
            access_token: access_token.clone(),
            author_name: submission.author_name.clone().unwrap_or_default(),
        }
    } else {
        return Err(api_error(
            StatusCode::UNAUTHORIZED,
            "Sign in with a member account or provide an order access token to review",
        ));
    };

    let record = service
        .submit_review(&product_id, submission, author, client_ip, user_agent)
        .await
        .map_err(map_aegis_error)?;

    let status = record.status.clone();
    if record.status == "approved" {
        let mut tags = CacheTags::new();
        tags.add_dependency("reviews", &product_id);
        tags.add_dependency(&state.config.commerce.product_type, &product_id);
        crate::cdn::purge_tags(&state, &tags).await;
    }

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "review": PublicReview::from(record),
            // `approved`（公開された）か `pending`（承認待ち）か。投稿者に伝える文を決めるのに使う
            "status": status,
        })),
    ))
}

// --- 管理者向けエンドポイント ---

#[derive(Debug, Deserialize)]
pub struct AdminReviewListQuery {
    pub product_id: Option<String>,
    pub status: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: u64,
    #[serde(default)]
    pub offset: u64,
}

#[derive(Debug, Deserialize)]
pub struct PatchReviewRequest {
    pub status: String,
    /// ステマ規制対応: 非承認理由
    pub rejection_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReplyReviewRequest {
    pub reply: String,
}

#[derive(Debug, Deserialize)]
pub struct BulkReviewRequest {
    pub ids: Vec<String>,
    pub action: String, // "approve", "pending", "spam", "trash", "delete"
    pub rejection_reason: Option<String>,
}

/// `GET /api/v1/admin/reviews` — 管理者: レビュー一覧
pub async fn admin_list_reviews(
    RequireRole(_session): EditorSession,
    State(state): State<Arc<AppState>>,
    Query(query): Query<AdminReviewListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let service = reviews(&state)?;
    let list_query = ReviewListQuery {
        product_id: query.product_id,
        status: query.status,
        member_id: None,
        oldest_first: false,
        limit: Some(query.limit),
        offset: query.offset,
    };
    let page = service
        .admin_list_reviews(list_query)
        .await
        .map_err(map_aegis_error)?;

    Ok(Json(json!({
        "reviews": page.reviews,
        "total": page.total,
    })))
}

/// `GET /api/v1/admin/reviews/{id}` — 管理者: レビュー詳細
pub async fn admin_get_review(
    RequireRole(_session): EditorSession,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ReviewRecord>, ApiError> {
    let service = reviews(&state)?;
    let record = service
        .admin_get_review(&id)
        .await
        .map_err(map_aegis_error)?;
    Ok(Json(record))
}

/// `PATCH /api/v1/admin/reviews/{id}` — 管理者: ステータス変更（ステマ規制非承認理由付き）
pub async fn admin_patch_review(
    RequireRole(_session): EditorSession,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<PatchReviewRequest>,
) -> Result<StatusCode, ApiError> {
    let service = reviews(&state)?;
    service
        .admin_set_status(&id, &req.status, req.rejection_reason.as_deref())
        .await
        .map_err(map_aegis_error)?;
    if let Ok(record) = service.admin_get_review(&id).await {
        let mut tags = CacheTags::new();
        tags.add_dependency("reviews", &record.product_id);
        tags.add_dependency(&state.config.commerce.product_type, &record.product_id);
        crate::cdn::purge_tags(&state, &tags).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/v1/admin/reviews/{id}/reply` — 管理者: 返信
pub async fn admin_set_reply(
    RequireRole(_session): EditorSession,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<ReplyReviewRequest>,
) -> Result<StatusCode, ApiError> {
    let service = reviews(&state)?;
    service
        .admin_set_reply(&id, Some(&req.reply))
        .await
        .map_err(map_aegis_error)?;
    if let Ok(record) = service.admin_get_review(&id).await {
        let mut tags = CacheTags::new();
        tags.add_dependency("reviews", &record.product_id);
        tags.add_dependency(&state.config.commerce.product_type, &record.product_id);
        crate::cdn::purge_tags(&state, &tags).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/v1/admin/reviews/{id}/reply` — 管理者: 返信削除
pub async fn admin_delete_reply(
    RequireRole(_session): EditorSession,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let service = reviews(&state)?;
    service
        .admin_set_reply(&id, None)
        .await
        .map_err(map_aegis_error)?;
    if let Ok(record) = service.admin_get_review(&id).await {
        let mut tags = CacheTags::new();
        tags.add_dependency("reviews", &record.product_id);
        tags.add_dependency(&state.config.commerce.product_type, &record.product_id);
        crate::cdn::purge_tags(&state, &tags).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/v1/admin/reviews/bulk` — 管理者: 一括操作
pub async fn admin_bulk_reviews(
    RequireRole(_session): EditorSession,
    State(state): State<Arc<AppState>>,
    Json(req): Json<BulkReviewRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let service = reviews(&state)?;
    let count = match req.action.as_str() {
        "delete" => service
            .admin_bulk_delete(&req.ids)
            .await
            .map_err(map_aegis_error)?,
        "approve" => service
            .admin_bulk_status(&req.ids, "approved", None)
            .await
            .map_err(map_aegis_error)?,
        "pending" => service
            .admin_bulk_status(&req.ids, "pending", req.rejection_reason.as_deref())
            .await
            .map_err(map_aegis_error)?,
        "spam" => service
            .admin_bulk_status(&req.ids, "spam", req.rejection_reason.as_deref())
            .await
            .map_err(map_aegis_error)?,
        "trash" => service
            .admin_bulk_status(&req.ids, "trash", req.rejection_reason.as_deref())
            .await
            .map_err(map_aegis_error)?,
        other => {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                format!("Unknown bulk action: '{}'", other),
            ))
        }
    };

    Ok(Json(json!({ "count": count })))
}

/// `GET /api/v1/admin/reviews/settings` — 管理者: 承認制設定取得
pub async fn admin_get_review_settings(
    RequireRole(_session): EditorSession,
    State(state): State<Arc<AppState>>,
) -> Result<Json<PolicyOverrides>, ApiError> {
    let service = reviews(&state)?;
    let settings = service
        .admin_get_settings()
        .await
        .map_err(map_aegis_error)?;
    Ok(Json(settings))
}

/// `PUT /api/v1/admin/reviews/settings` — 管理者: 承認制設定更新
pub async fn admin_put_review_settings(
    RequireRole(_session): EditorSession,
    State(state): State<Arc<AppState>>,
    Json(settings): Json<PolicyOverrides>,
) -> Result<Json<PolicyOverrides>, ApiError> {
    let service = reviews(&state)?;
    let updated = service
        .admin_update_settings(settings)
        .await
        .map_err(map_aegis_error)?;
    Ok(Json(updated))
}
