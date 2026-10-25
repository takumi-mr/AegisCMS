//! 顧客アカウント（Customer Account）機能。
//! AegisCMS の管理アカウント（`_aegis_users`）とは完全に別管理の、EC ストア顧客アカウント。

use std::sync::Arc;

use aegis_audit::AuditService;
use aegis_auth::password::{
    hash_password, verify_dummy_password, verify_password, MIN_PASSWORD_LEN,
};
use aegis_core::{
    AegisError, AuditAction, AuditActor, AuditEvent, AuditOutcome, CacheBackend, Result,
    ShippingAddress,
};
use aegis_db::{CustomerRecord, DatabaseAdapter, OrderRecord};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

/// 顧客セッションのプレフィックス
pub const CUSTOMER_SESSION_PREFIX: &str = "customer_session:";
/// 顧客セッショントークンのプレフィックス
pub const CUSTOMER_TOKEN_PREFIX: &str = "cses_";
/// 顧客セッションのデフォルト有効期限（30日）
pub const DEFAULT_CUSTOMER_SESSION_TTL_SECS: u64 = 30 * 24 * 3600;

/// 会員登録リクエスト
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CustomerRegisterRequest {
    pub email: String,
    pub password: String,
    pub name: String,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub shipping_address: Option<ShippingAddress>,
}

/// ログインリクエスト
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CustomerLoginRequest {
    pub email: String,
    pub password: String,
}

/// 顧客セッションデータ
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct CustomerSessionData {
    pub token: String,
    pub customer_id: String,
    pub email: String,
    pub name: String,
    pub created_at: String,
    pub expires_at: String,
}

/// プロフィール更新リクエスト
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CustomerUpdateProfileRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub shipping_address: Option<ShippingAddress>,
    #[serde(default)]
    pub current_password: Option<String>,
    #[serde(default)]
    pub new_password: Option<String>,
}

/// API 応答用の顧客情報（パスワードハッシュを含まない）
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct CustomerResponse {
    pub id: String,
    pub email: String,
    pub name: String,
    pub phone: Option<String>,
    pub status: String,
    pub shipping_address: Option<ShippingAddress>,
    pub created_at: String,
    pub updated_at: String,
    pub last_login_at: Option<String>,
}

impl From<CustomerRecord> for CustomerResponse {
    fn from(r: CustomerRecord) -> Self {
        let shipping_address = r
            .shipping_address_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());
        Self {
            id: r.id,
            email: r.email,
            name: r.name,
            phone: r.phone,
            status: r.status,
            shipping_address,
            created_at: r.created_at,
            updated_at: r.updated_at,
            last_login_at: r.last_login_at,
        }
    }
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn future_timestamp(seconds: u64) -> String {
    (Utc::now() + chrono::Duration::seconds(seconds as i64))
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

pub struct CustomerService {
    db: Arc<dyn DatabaseAdapter>,
    cache: Arc<dyn CacheBackend>,
    audit: Arc<AuditService>,
    session_ttl_secs: u64,
}

impl CustomerService {
    pub fn new(
        db: Arc<dyn DatabaseAdapter>,
        cache: Arc<dyn CacheBackend>,
        audit: Arc<AuditService>,
    ) -> Self {
        Self {
            db,
            cache,
            audit,
            session_ttl_secs: DEFAULT_CUSTOMER_SESSION_TTL_SECS,
        }
    }

    pub fn with_ttl(mut self, ttl: u64) -> Self {
        self.session_ttl_secs = ttl;
        self
    }

    /// 会員登録
    pub async fn register(
        &self,
        req: CustomerRegisterRequest,
        ip: Option<String>,
        user_agent: Option<String>,
    ) -> Result<CustomerResponse> {
        let email = normalize_email(&req.email);
        if email.is_empty() || !email.contains('@') {
            return Err(AegisError::Validation(
                "A valid email address is required".to_string(),
            ));
        }
        if req.name.trim().is_empty() {
            return Err(AegisError::Validation("Name is required".to_string()));
        }

        // 重複チェック
        if let Some(_existing) = self.db.get_customer_by_email(&email).await? {
            return Err(AegisError::Validation(
                "An account with this email already exists".to_string(),
            ));
        }

        let password_hash = hash_password(&req.password)?;
        let id = Uuid::new_v4().to_string();
        let timestamp = now();
        let shipping_json = req
            .shipping_address
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| AegisError::Validation(e.to_string()))?;

        let record = CustomerRecord {
            id: id.clone(),
            email: email.clone(),
            password_hash,
            name: req.name.trim().to_string(),
            phone: req.phone.map(|p| p.trim().to_string()),
            status: "active".to_string(),
            shipping_address_json: shipping_json,
            created_at: timestamp.clone(),
            updated_at: timestamp,
            last_login_at: None,
        };

        self.db.insert_customer(&record).await?;

        // 監査ログ
        let event = AuditEvent::new(AuditAction::CustomerRegister, AuditOutcome::Success)
            .with_actor(Some(AuditActor {
                user_id: id.clone(),
                role: "customer".to_string(),
            }))
            .with_resource("customer", Some(id))
            .with_client(ip, user_agent)
            .with_details(json!({ "email": email }));
        self.audit.record(event).await;

        Ok(record.into())
    }

    /// ログイン
    pub async fn login(
        &self,
        req: CustomerLoginRequest,
        ip: Option<String>,
        user_agent: Option<String>,
    ) -> Result<(CustomerSessionData, CustomerResponse)> {
        let email = normalize_email(&req.email);
        let customer = self.db.get_customer_by_email(&email).await?;

        let record = match customer {
            Some(c) => c,
            None => {
                // タイミング攻撃防御
                verify_dummy_password(&req.password);
                return Err(AegisError::Authentication(
                    "Invalid email or password".to_string(),
                ));
            }
        };

        if !verify_password(&req.password, &record.password_hash)? {
            return Err(AegisError::Authentication(
                "Invalid email or password".to_string(),
            ));
        }

        if record.status != "active" {
            return Err(AegisError::PermissionDenied(
                "This customer account has been suspended".to_string(),
            ));
        }

        let token = format!("{}{}", CUSTOMER_TOKEN_PREFIX, Uuid::new_v4().simple());
        let created_at = now();
        let expires_at = future_timestamp(self.session_ttl_secs);

        let session = CustomerSessionData {
            token: token.clone(),
            customer_id: record.id.clone(),
            email: record.email.clone(),
            name: record.name.clone(),
            created_at,
            expires_at,
        };

        let session_json =
            serde_json::to_string(&session).map_err(|e| AegisError::Internal(e.to_string()))?;
        let cache_key = format!("{}{}", CUSTOMER_SESSION_PREFIX, token);
        self.cache
            .set(&cache_key, &session_json, Some(self.session_ttl_secs))
            .await?;

        let login_time = now();
        self.db
            .update_customer_last_login(&record.id, &login_time)
            .await?;

        let mut updated_record = record;
        updated_record.last_login_at = Some(login_time);

        // 監査ログ
        let event = AuditEvent::new(AuditAction::CustomerLogin, AuditOutcome::Success)
            .with_actor(Some(AuditActor {
                user_id: updated_record.id.clone(),
                role: "customer".to_string(),
            }))
            .with_resource("customer", Some(updated_record.id.clone()))
            .with_client(ip, user_agent);
        self.audit.record(event).await;

        Ok((session, updated_record.into()))
    }

    /// セッションの検証
    pub async fn verify_session(&self, token: &str) -> Result<Option<CustomerSessionData>> {
        if !token.starts_with(CUSTOMER_TOKEN_PREFIX) {
            return Ok(None);
        }
        let cache_key = format!("{}{}", CUSTOMER_SESSION_PREFIX, token);
        let cached = self.cache.get(&cache_key).await?;
        match cached {
            Some(json) => {
                let session: CustomerSessionData =
                    serde_json::from_str(&json).map_err(|e| AegisError::Internal(e.to_string()))?;
                Ok(Some(session))
            }
            None => Ok(None),
        }
    }

    /// ログアウト
    pub async fn logout(&self, token: &str) -> Result<()> {
        let cache_key = format!("{}{}", CUSTOMER_SESSION_PREFIX, token);
        self.cache.delete(&cache_key).await?;
        Ok(())
    }

    /// プロフィール取得
    pub async fn get_profile(&self, customer_id: &str) -> Result<CustomerResponse> {
        let record = self
            .db
            .get_customer_by_id(customer_id)
            .await?
            .ok_or_else(|| AegisError::NotFound("Customer not found".to_string()))?;
        Ok(record.into())
    }

    /// プロフィール更新
    pub async fn update_profile(
        &self,
        customer_id: &str,
        req: CustomerUpdateProfileRequest,
    ) -> Result<CustomerResponse> {
        let mut record = self
            .db
            .get_customer_by_id(customer_id)
            .await?
            .ok_or_else(|| AegisError::NotFound("Customer not found".to_string()))?;

        if let Some(name) = req.name {
            if name.trim().is_empty() {
                return Err(AegisError::Validation("Name cannot be empty".to_string()));
            }
            record.name = name.trim().to_string();
        }

        if let Some(phone) = req.phone {
            record.phone = if phone.trim().is_empty() {
                None
            } else {
                Some(phone.trim().to_string())
            };
        }

        if let Some(ref addr) = req.shipping_address {
            record.shipping_address_json = Some(
                serde_json::to_string(addr).map_err(|e| AegisError::Validation(e.to_string()))?,
            );
        }

        // パスワード変更
        if let Some(ref new_pw) = req.new_password {
            let current_pw = req.current_password.as_deref().ok_or_else(|| {
                AegisError::Validation(
                    "Current password is required to set a new password".to_string(),
                )
            })?;
            if !verify_password(current_pw, &record.password_hash)? {
                return Err(AegisError::Authentication(
                    "Current password does not match".to_string(),
                ));
            }
            if new_pw.chars().count() < MIN_PASSWORD_LEN {
                return Err(AegisError::Validation(format!(
                    "New password must be at least {} characters",
                    MIN_PASSWORD_LEN
                )));
            }
            record.password_hash = hash_password(new_pw)?;
        }

        record.updated_at = now();
        self.db.update_customer(&record).await?;

        // 監査ログ
        let event = AuditEvent::new(AuditAction::CustomerUpdate, AuditOutcome::Success)
            .with_actor(Some(AuditActor {
                user_id: customer_id.to_string(),
                role: "customer".to_string(),
            }))
            .with_resource("customer", Some(customer_id.to_string()));
        self.audit.record(event).await;

        Ok(record.into())
    }

    /// 注文履歴一覧取得
    pub async fn list_orders(&self, customer_id: &str) -> Result<Vec<OrderRecord>> {
        self.db.list_customer_orders(customer_id).await
    }

    /// 注文詳細取得（自分の注文であることを検証）
    pub async fn get_order(&self, customer_id: &str, order_id: &str) -> Result<OrderRecord> {
        let order = self
            .db
            .get_order(order_id)
            .await?
            .ok_or_else(|| AegisError::NotFound("Order not found".to_string()))?;

        // customer_id が一致するか、あるいは顧客のメールアドレスと注文のメールアドレスが一致するか検証
        if order.customer_id.as_deref() != Some(customer_id) {
            let profile = self.get_profile(customer_id).await?;
            if order.customer_email.to_lowercase() != profile.email.to_lowercase() {
                return Err(AegisError::PermissionDenied(
                    "You do not have access to this order".to_string(),
                ));
            }
        }

        Ok(order)
    }

    // --- 管理者向け操作 ---

    /// 管理者: 顧客一覧
    pub async fn admin_list_customers(
        &self,
        limit: u64,
        offset: u64,
    ) -> Result<(Vec<CustomerResponse>, u64)> {
        let (records, total) = self.db.list_customers(limit, offset).await?;
        let responses = records.into_iter().map(CustomerResponse::from).collect();
        Ok((responses, total))
    }

    /// 管理者: 顧客詳細取得
    pub async fn admin_get_customer(&self, customer_id: &str) -> Result<CustomerResponse> {
        self.get_profile(customer_id).await
    }

    /// 管理者: 顧客ステータス変更（active / suspended）
    pub async fn admin_set_status(&self, customer_id: &str, status: &str) -> Result<()> {
        if status != "active" && status != "suspended" {
            return Err(AegisError::Validation(
                "Status must be either 'active' or 'suspended'".to_string(),
            ));
        }
        self.db
            .set_customer_status(customer_id, status, &now())
            .await?;
        Ok(())
    }
}
