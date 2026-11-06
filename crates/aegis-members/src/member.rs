//! 会員アカウント（Member Account）機能。
//! AegisCMS の管理アカウント（`_aegis_users`）とは完全に別管理の、訪問者の会員アカウント。
//! ショップ・掲示板などが共通で使う。注文やレビューなど、機能ごとのデータは扱わない
//! （退会のときの後始末は、呼び出す側が `delete_account` の前に行う）。

use std::sync::Arc;

use aegis_audit::AuditService;
use aegis_auth::password::{
    hash_password, verify_dummy_password, verify_password, MIN_PASSWORD_LEN,
};
use aegis_core::{
    AegisError, AuditAction, AuditActor, AuditEvent, AuditOutcome, CacheBackend, Result,
    ShippingAddress,
};
use aegis_db::{DatabaseAdapter, MemberRecord};

use crate::config::{MemberRegistration, MembersConfig};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

/// 会員セッションのプレフィックス
pub const MEMBER_SESSION_PREFIX: &str = "member_session:";
/// 会員セッショントークンのプレフィックス
pub const MEMBER_TOKEN_PREFIX: &str = "mses_";
/// 会員セッションのデフォルト有効期限（30日）
pub const DEFAULT_MEMBER_SESSION_TTL_SECS: u64 = 30 * 24 * 3600;
/// 会員の「ここより前に作ったセッションは無効」の印（マイクロ秒）のキャッシュキーの接頭辞
const REVOKED_PREFIX: &str = "member_revoked:";
/// パスワード再設定のトークンのキャッシュキーの接頭辞
const RESET_PREFIX: &str = "member_reset:";
/// メールアドレスの確認のトークンのキャッシュキーの接頭辞
const VERIFY_PREFIX: &str = "member_verify:";
/// 確認リンクの有効期間（秒。48 時間）
pub const EMAIL_VERIFICATION_TTL_SECS: u64 = 48 * 3600;
/// ログインを断るときのメッセージ（クライアントは、これで「確認が済んでいない」と見分ける）
pub const EMAIL_NOT_VERIFIED_MESSAGE: &str = "Please confirm your email address first";
/// 再設定リンクの有効期間（秒）
pub const PASSWORD_RESET_TTL_SECS: u64 = 3600;
/// 同じ IP からの再設定の申し込みの上限（1 時間あたり）
const RESET_MAX_PER_IP_PER_HOUR: u64 = 5;
/// 同じメールアドレスへ送る再設定メールの上限（1 時間あたり。メールの連打を防ぐ）
const RESET_MAX_PER_EMAIL_PER_HOUR: u64 = 3;

/// 会員登録リクエスト
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemberRegisterRequest {
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
pub struct MemberLoginRequest {
    pub email: String,
    pub password: String,
}

/// 会員セッションデータ
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct MemberSessionData {
    pub token: String,
    pub member_id: String,
    pub email: String,
    pub name: String,
    pub created_at: String,
    pub expires_at: String,
    /// セッションを作った時刻（UNIX 時刻のマイクロ秒）。会員の「ここより前のセッションは無効」の印と比べて、
    /// 失効を判断する。印を置いた直後に作り直したセッションを巻き込まないよう、ミリ秒より細かく持つ
    #[serde(default)]
    pub issued_at_micros: i64,
}

/// 再設定の申し込みの結果（メールに載せる情報）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberReset {
    pub token: String,
    pub email: String,
    pub name: String,
}

/// 確認メールに載せる情報
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberVerification {
    pub token: String,
    pub email: String,
    pub name: String,
}

/// キャッシュに置くトークンのデータ（再設定・メールの確認）
#[derive(Debug, Serialize, Deserialize)]
struct PendingToken {
    member_id: String,
    email: String,
}

fn sha256_hex(value: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn random_token() -> String {
    let mut bytes = Vec::with_capacity(32);
    bytes.extend_from_slice(Uuid::new_v4().as_bytes());
    bytes.extend_from_slice(Uuid::new_v4().as_bytes());
    hex::encode(bytes)
}

/// プロフィール更新リクエスト
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MemberUpdateProfileRequest {
    #[serde(default)]
    pub name: Option<String>,
    /// 公開名。空文字で消す（消すと、公開の場には `name` が出る）
    #[serde(default)]
    pub nickname: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub shipping_address: Option<ShippingAddress>,
    #[serde(default)]
    pub current_password: Option<String>,
    #[serde(default)]
    pub new_password: Option<String>,
}

/// API 応答用の会員情報（パスワードハッシュを含まない）
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct MemberResponse {
    pub id: String,
    pub email: String,
    pub name: String,
    /// 公開名（未設定なら `None`）
    pub nickname: Option<String>,
    /// アバター画像のストレージキー（API の応答には載せない。公開 URL は `avatar_url`）
    #[serde(default, skip_serializing)]
    pub avatar_key: Option<String>,
    /// アバター画像の公開 URL（未設定なら `None`）。ストレージの設定を知る呼び出し側が `avatar_key` から埋める
    #[serde(default)]
    pub avatar_url: Option<String>,
    pub phone: Option<String>,
    pub status: String,
    pub shipping_address: Option<ShippingAddress>,
    pub created_at: String,
    pub updated_at: String,
    pub last_login_at: Option<String>,
}

impl MemberResponse {
    /// 公開の場（レビュー・掲示板）に出す名前。公開名があればそれ、なければ `name`
    pub fn public_name(&self) -> &str {
        match self.nickname.as_deref() {
            Some(nickname) if !nickname.trim().is_empty() => nickname,
            _ => &self.name,
        }
    }
}

/// 公開名の最大文字数
pub const NICKNAME_MAX_CHARS: usize = 30;

/// 公開名を検査して整える（空なら `None` = 消す）。改行などの制御文字は使えない
fn normalize_nickname(value: &str) -> Result<Option<String>> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > NICKNAME_MAX_CHARS {
        return Err(AegisError::Validation(format!(
            "Nickname must be at most {} characters",
            NICKNAME_MAX_CHARS
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(AegisError::Validation(
            "Nickname cannot contain control characters".to_string(),
        ));
    }
    Ok(Some(value.to_string()))
}

impl From<MemberRecord> for MemberResponse {
    fn from(r: MemberRecord) -> Self {
        let shipping_address = r
            .shipping_address_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());
        Self {
            id: r.id,
            email: r.email,
            name: r.name,
            nickname: r.nickname,
            avatar_key: r.avatar_key,
            avatar_url: None,
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

pub struct MemberService {
    db: Arc<dyn DatabaseAdapter>,
    cache: Arc<dyn CacheBackend>,
    audit: Arc<AuditService>,
    session_ttl_secs: u64,
    registration_open: bool,
    require_email_verification: bool,
}

impl MemberService {
    /// 設定（`[members]`）から作る。会員アカウントを使わない設定なら `None`
    pub fn from_config(
        config: &MembersConfig,
        db: Arc<dyn DatabaseAdapter>,
        cache: Arc<dyn CacheBackend>,
        audit: Arc<AuditService>,
    ) -> Option<Self> {
        config.enabled.then(|| {
            Self::new(db, cache, audit)
                .with_ttl(config.session_ttl_secs())
                .with_registration_open(config.registration == MemberRegistration::Open)
                .with_email_verification(config.require_email_verification)
        })
    }

    pub fn new(
        db: Arc<dyn DatabaseAdapter>,
        cache: Arc<dyn CacheBackend>,
        audit: Arc<AuditService>,
    ) -> Self {
        Self {
            db,
            cache,
            audit,
            session_ttl_secs: DEFAULT_MEMBER_SESSION_TTL_SECS,
            registration_open: true,
            require_email_verification: false,
        }
    }

    pub fn with_ttl(mut self, ttl: u64) -> Self {
        self.session_ttl_secs = ttl;
        self
    }

    /// 新規の会員登録を受け付けるか（`false` でも、登録済みの会員はログインできる）
    pub fn with_registration_open(mut self, open: bool) -> Self {
        self.registration_open = open;
        self
    }

    /// 登録のあと、メールの確認が済むまでログインさせないか
    pub fn with_email_verification(mut self, required: bool) -> Self {
        self.require_email_verification = required;
        self
    }

    /// 登録のあと、メールの確認が要るか（要るなら、登録は `register_pending` を使う）
    pub fn requires_email_verification(&self) -> bool {
        self.require_email_verification
    }

    /// ログインの有効期間（秒）。ブラウザの Cookie の `Max-Age` にも使う
    pub fn session_ttl_secs(&self) -> u64 {
        self.session_ttl_secs
    }

    /// 新規の会員登録を受け付けているか
    pub fn registration_open(&self) -> bool {
        self.registration_open
    }

    /// 会員登録
    pub async fn register(
        &self,
        req: MemberRegisterRequest,
        ip: Option<String>,
        user_agent: Option<String>,
    ) -> Result<MemberResponse> {
        if self.require_email_verification {
            // 確認を飛ばして有効な会員を作らせない
            return Err(AegisError::Internal(
                "Email confirmation is required: use register_pending".to_string(),
            ));
        }
        let email = self.check_registration(&req)?;

        // 重複チェック
        if let Some(_existing) = self.db.get_member_by_email(&email).await? {
            return Err(AegisError::Validation(
                "An account with this email already exists".to_string(),
            ));
        }
        self.insert_member(req, email, "active", ip, user_agent)
            .await
            .map(Into::into)
    }

    /// 登録の受付と入力の検査。正規化したメールアドレスを返す
    fn check_registration(&self, req: &MemberRegisterRequest) -> Result<String> {
        if !self.registration_open {
            return Err(AegisError::PermissionDenied(
                "Member registration is closed".to_string(),
            ));
        }
        let email = normalize_email(&req.email);
        if email.is_empty() || !email.contains('@') {
            return Err(AegisError::Validation(
                "A valid email address is required".to_string(),
            ));
        }
        if req.name.trim().is_empty() {
            return Err(AegisError::Validation("Name is required".to_string()));
        }
        Ok(email)
    }

    /// 新しい会員の行を作る（`status` は `active` か、確認待ちの `unverified`）
    async fn insert_member(
        &self,
        req: MemberRegisterRequest,
        email: String,
        status: &str,
        ip: Option<String>,
        user_agent: Option<String>,
    ) -> Result<MemberRecord> {
        let password_hash = hash_password(&req.password)?;
        let id = Uuid::new_v4().to_string();
        let timestamp = now();
        let shipping_json = req
            .shipping_address
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| AegisError::Validation(e.to_string()))?;

        let record = MemberRecord {
            id: id.clone(),
            email: email.clone(),
            password_hash,
            name: req.name.trim().to_string(),
            nickname: None,
            avatar_key: None,
            phone: req.phone.map(|p| p.trim().to_string()),
            status: status.to_string(),
            shipping_address_json: shipping_json,
            created_at: timestamp.clone(),
            updated_at: timestamp,
            last_login_at: None,
        };

        self.db.insert_member(&record).await?;

        // 監査ログ
        let event = AuditEvent::new(AuditAction::MemberRegister, AuditOutcome::Success)
            .with_actor(Some(AuditActor {
                user_id: id.clone(),
                role: "member".to_string(),
            }))
            .with_resource("member", Some(id))
            .with_client(ip, user_agent)
            .with_details(json!({ "email": email }));
        self.audit.record(event).await;

        Ok(record)
    }

    /// 確認が要るときの会員登録。会員は `unverified`（確認が済むまでログインできない）で作る。
    /// 登録済みのアドレスは、応答で区別できないよう `Ok(None)` を返す。まだ確認していない会員のアドレスで
    /// 登録し直したときは、名前とパスワードを今の登録のもので置き換えて、確認メールを送り直す
    /// （他人のアドレスで先に登録されても、本人が登録し直せば、本人のものになる）
    pub async fn register_pending(
        &self,
        req: MemberRegisterRequest,
        ip: Option<String>,
        user_agent: Option<String>,
    ) -> Result<Option<MemberVerification>> {
        let email = self.check_registration(&req)?;
        let member = match self.db.get_member_by_email(&email).await? {
            None => {
                self.insert_member(req, email, "unverified", ip, user_agent)
                    .await?
            }
            Some(mut existing) if existing.status == "unverified" => {
                existing.password_hash = hash_password(&req.password)?;
                existing.name = req.name.trim().to_string();
                existing.phone = req.phone.map(|p| p.trim().to_string());
                existing.updated_at = now();
                self.db.update_member(&existing).await?;
                existing
            }
            // 登録済み（有効・停止中）。登録の有無を応答で教えない
            Some(_) => return Ok(None),
        };
        self.issue_verification(member).await
    }

    /// 確認メールの情報を作る。同じ宛先へのメールは 1 時間に 3 通まで（超えた分は送らず `None`）
    async fn issue_verification(&self, member: MemberRecord) -> Result<Option<MemberVerification>> {
        let sent = self
            .cache
            .increment(
                &format!("member_verify_email:{}", sha256_hex(&member.email)),
                Some(3600),
            )
            .await?;
        if sent > RESET_MAX_PER_EMAIL_PER_HOUR {
            return Ok(None);
        }
        let token = random_token();
        let pending = PendingToken {
            member_id: member.id.clone(),
            email: member.email.clone(),
        };
        let value =
            serde_json::to_string(&pending).map_err(|e| AegisError::Internal(e.to_string()))?;
        self.cache
            .set(
                &format!("{}{}", VERIFY_PREFIX, sha256_hex(&token)),
                &value,
                Some(EMAIL_VERIFICATION_TTL_SECS),
            )
            .await?;
        Ok(Some(MemberVerification {
            token,
            email: member.email,
            name: member.name,
        }))
    }

    /// 退会の前に、パスワードで本人であることを確かめる。違えば `Authentication`（何も変えない）
    pub async fn confirm_password(&self, member_id: &str, password: &str) -> Result<()> {
        let record = self
            .db
            .get_member_by_id(member_id)
            .await?
            .ok_or_else(|| AegisError::NotFound("Member not found".to_string()))?;
        if !verify_password(password, &record.password_hash)? {
            return Err(AegisError::Authentication(
                "Password does not match".to_string(),
            ));
        }
        Ok(())
    }

    /// 退会（自分のアカウントの削除）。会員の行を消して、セッションをすべて終わらせる。
    /// 注文・レビュー・投稿など、機能ごとのデータの後始末（会員との紐づきを外す・匿名にする）は、
    /// 呼び出す側が、この前に行う（本人の確認も `confirm_password` で先に済ませる）。
    /// 監査ログにはメールアドレスを残さない。途中で失敗しても、同じ手順をやり直せる
    pub async fn delete_account(
        &self,
        member_id: &str,
        ip: Option<String>,
        user_agent: Option<String>,
    ) -> Result<()> {
        self.db.delete_member(member_id).await?;
        // 消した会員のセッションを、その場で使えなくする
        self.revoke_sessions(member_id).await?;

        let event = AuditEvent::new(AuditAction::MemberDelete, AuditOutcome::Success)
            .with_actor(Some(AuditActor {
                user_id: member_id.to_string(),
                role: "member".to_string(),
            }))
            .with_resource("member", Some(member_id.to_string()))
            .with_client(ip, user_agent);
        self.audit.record(event).await;
        Ok(())
    }

    /// 確認メールを送り直す。確認待ちの会員がいるときだけ情報を返す（いない・確認済み・停止中は `None`。
    /// 応答では区別しない）。同じ IP からは 1 時間に 5 回まで（登録の有無によらず数える）
    pub async fn resend_verification(
        &self,
        email: &str,
        ip: Option<&str>,
    ) -> Result<Option<MemberVerification>> {
        let email = normalize_email(email);
        if email.is_empty() || !email.contains('@') || email.len() > 254 {
            return Err(AegisError::Validation(
                "A valid email address is required".to_string(),
            ));
        }
        if let Some(ip) = ip {
            let count = self
                .cache
                .increment(&format!("member_verify_ip:{}", ip), Some(3600))
                .await?;
            if count > RESET_MAX_PER_IP_PER_HOUR {
                return Err(AegisError::ResourceLimitExceeded(
                    "Too many requests".to_string(),
                ));
            }
        }
        match self.db.get_member_by_email(&email).await? {
            Some(member) if member.status == "unverified" => self.issue_verification(member).await,
            _ => Ok(None),
        }
    }

    /// 確認リンクのトークンで、メールアドレスを確認する（会員は `active` になる）。トークンは 1 回だけ
    pub async fn verify_email(
        &self,
        token: &str,
        ip: Option<String>,
        user_agent: Option<String>,
    ) -> Result<MemberResponse> {
        let invalid = || {
            AegisError::Validation("This confirmation link is invalid or has expired".to_string())
        };
        let token = token.trim();
        if token.is_empty() {
            return Err(invalid());
        }
        let key = format!("{}{}", VERIFY_PREFIX, sha256_hex(token));
        let Some(value) = self.cache.get(&key).await? else {
            return Err(invalid());
        };
        self.cache.delete(&key).await?;
        let pending: PendingToken = serde_json::from_str(&value).map_err(|_| invalid())?;

        let mut record = self
            .db
            .get_member_by_id(&pending.member_id)
            .await?
            .ok_or_else(invalid)?;
        if !record.email.eq_ignore_ascii_case(&pending.email) {
            return Err(invalid());
        }
        match record.status.as_str() {
            "unverified" => {
                record.status = "active".to_string();
                record.updated_at = now();
                self.db.update_member(&record).await?;
                let event = AuditEvent::new(AuditAction::MemberVerify, AuditOutcome::Success)
                    .with_actor(Some(AuditActor {
                        user_id: record.id.clone(),
                        role: "member".to_string(),
                    }))
                    .with_resource("member", Some(record.id.clone()))
                    .with_client(ip, user_agent);
                self.audit.record(event).await;
            }
            // すでに確認済み（別のリンクで確認した）。何も変えない
            "active" => {}
            _ => return Err(invalid()),
        }
        Ok(record.into())
    }

    /// ログイン
    pub async fn login(
        &self,
        req: MemberLoginRequest,
        ip: Option<String>,
        user_agent: Option<String>,
    ) -> Result<(MemberSessionData, MemberResponse)> {
        let email = normalize_email(&req.email);
        let member = self.db.get_member_by_email(&email).await?;

        let record = match member {
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

        if record.status == "unverified" {
            return Err(AegisError::PermissionDenied(
                EMAIL_NOT_VERIFIED_MESSAGE.to_string(),
            ));
        }
        if record.status != "active" {
            return Err(AegisError::PermissionDenied(
                "This member account has been suspended".to_string(),
            ));
        }

        let session = self.create_session(&record).await?;

        let login_time = now();
        self.db
            .update_member_last_login(&record.id, &login_time)
            .await?;

        let mut updated_record = record;
        updated_record.last_login_at = Some(login_time);

        // 監査ログ
        let event = AuditEvent::new(AuditAction::MemberLogin, AuditOutcome::Success)
            .with_actor(Some(AuditActor {
                user_id: updated_record.id.clone(),
                role: "member".to_string(),
            }))
            .with_resource("member", Some(updated_record.id.clone()))
            .with_client(ip, user_agent);
        self.audit.record(event).await;

        Ok((session, updated_record.into()))
    }

    /// セッションの検証
    /// 会員のセッションを作って、キャッシュに置く
    async fn create_session(&self, record: &MemberRecord) -> Result<MemberSessionData> {
        let token = format!("{}{}", MEMBER_TOKEN_PREFIX, Uuid::new_v4().simple());
        let session = MemberSessionData {
            token: token.clone(),
            member_id: record.id.clone(),
            email: record.email.clone(),
            name: record.name.clone(),
            created_at: now(),
            expires_at: future_timestamp(self.session_ttl_secs),
            issued_at_micros: Utc::now().timestamp_micros(),
        };
        let session_json =
            serde_json::to_string(&session).map_err(|e| AegisError::Internal(e.to_string()))?;
        let cache_key = format!("{}{}", MEMBER_SESSION_PREFIX, token);
        self.cache
            .set(&cache_key, &session_json, Some(self.session_ttl_secs))
            .await?;
        Ok(session)
    }

    /// 会員のセッションをすべて失効させる（停止・パスワードの変更・再設定のとき）。
    /// セッションごとに消す代わりに、「この時刻より前に作ったセッションは無効」の印を置く
    /// （キャッシュが Redis でも、全サーバーで効く。印は最長のセッションの有効期間だけ残す）
    pub async fn revoke_sessions(&self, member_id: &str) -> Result<()> {
        let key = format!("{}{}", REVOKED_PREFIX, member_id);
        let revoked_at = Utc::now().timestamp_micros();
        self.cache
            .set(&key, &revoked_at.to_string(), Some(self.session_ttl_secs))
            .await?;
        Ok(())
    }

    /// 同じ会員の新しいセッションを作る（パスワードの変更で全セッションを失効させたあと、
    /// 変更した本人の画面を続けて使えるようにするため）
    pub async fn reissue_session(&self, member_id: &str) -> Result<MemberSessionData> {
        let record = self
            .db
            .get_member_by_id(member_id)
            .await?
            .ok_or_else(|| AegisError::NotFound("Member not found".to_string()))?;
        if record.status != "active" {
            return Err(AegisError::PermissionDenied(
                "This member account has been suspended".to_string(),
            ));
        }
        self.create_session(&record).await
    }

    pub async fn verify_session(&self, token: &str) -> Result<Option<MemberSessionData>> {
        if !token.starts_with(MEMBER_TOKEN_PREFIX) {
            return Ok(None);
        }
        let cache_key = format!("{}{}", MEMBER_SESSION_PREFIX, token);
        let Some(json) = self.cache.get(&cache_key).await? else {
            return Ok(None);
        };
        let session: MemberSessionData =
            serde_json::from_str(&json).map_err(|e| AegisError::Internal(e.to_string()))?;
        // 失効の印より前に作ったセッションは通さない
        let revoked_key = format!("{}{}", REVOKED_PREFIX, session.member_id);
        if let Some(revoked_at) = self.cache.get(&revoked_key).await? {
            if revoked_at
                .parse::<i64>()
                .is_ok_and(|revoked_at| session.issued_at_micros < revoked_at)
            {
                return Ok(None);
            }
        }
        Ok(Some(session))
    }

    /// ログアウト
    pub async fn logout(&self, token: &str) -> Result<()> {
        let cache_key = format!("{}{}", MEMBER_SESSION_PREFIX, token);
        self.cache.delete(&cache_key).await?;
        Ok(())
    }

    /// プロフィール取得
    pub async fn get_profile(&self, member_id: &str) -> Result<MemberResponse> {
        let record = self
            .db
            .get_member_by_id(member_id)
            .await?
            .ok_or_else(|| AegisError::NotFound("Member not found".to_string()))?;
        Ok(record.into())
    }

    /// 会員 ID の一覧から、アバター画像のある会員の（ID → `avatar_key`）をまとめて引く（1 回の問い合わせ）
    pub async fn avatar_keys(
        &self,
        member_ids: &[String],
    ) -> Result<std::collections::HashMap<String, String>> {
        let mut ids: Vec<String> = member_ids.to_vec();
        ids.sort();
        ids.dedup();
        if ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        Ok(self
            .db
            .list_member_avatar_keys(&ids)
            .await?
            .into_iter()
            .collect())
    }

    /// アバター画像のキーを設定する（`None` で外す）。以前のキーを返す（呼び出し側が古い画像を消す）
    pub async fn set_avatar(
        &self,
        member_id: &str,
        avatar_key: Option<String>,
    ) -> Result<(MemberResponse, Option<String>)> {
        let mut record = self
            .db
            .get_member_by_id(member_id)
            .await?
            .ok_or_else(|| AegisError::NotFound("Member not found".to_string()))?;
        let previous = std::mem::replace(&mut record.avatar_key, avatar_key);
        record.updated_at = now();
        self.db.update_member(&record).await?;
        Ok((record.into(), previous))
    }

    /// プロフィール更新
    pub async fn update_profile(
        &self,
        member_id: &str,
        req: MemberUpdateProfileRequest,
    ) -> Result<MemberResponse> {
        let mut record = self
            .db
            .get_member_by_id(member_id)
            .await?
            .ok_or_else(|| AegisError::NotFound("Member not found".to_string()))?;

        if let Some(name) = req.name {
            if name.trim().is_empty() {
                return Err(AegisError::Validation("Name cannot be empty".to_string()));
            }
            record.name = name.trim().to_string();
        }

        if let Some(nickname) = req.nickname {
            record.nickname = normalize_nickname(&nickname)?;
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
        self.db.update_member(&record).await?;
        // パスワードを変えたら、ほかの端末のセッションも終わらせる（呼び出し元が、本人の分は作り直す）
        if req.new_password.is_some() {
            self.revoke_sessions(member_id).await?;
        }

        // 監査ログ
        let event = AuditEvent::new(AuditAction::MemberUpdate, AuditOutcome::Success)
            .with_actor(Some(AuditActor {
                user_id: member_id.to_string(),
                role: "member".to_string(),
            }))
            .with_resource("member", Some(member_id.to_string()));
        self.audit.record(event).await;

        Ok(record.into())
    }

    // --- 管理者向け操作 ---

    /// 管理者: 会員一覧
    pub async fn admin_list_members(
        &self,
        limit: u64,
        offset: u64,
    ) -> Result<(Vec<MemberResponse>, u64)> {
        let (records, total) = self.db.list_members(limit, offset).await?;
        let responses = records.into_iter().map(MemberResponse::from).collect();
        Ok((responses, total))
    }

    /// 管理者: 会員詳細取得
    pub async fn admin_get_member(&self, member_id: &str) -> Result<MemberResponse> {
        self.get_profile(member_id).await
    }

    /// 管理者: 会員ステータス変更（active / suspended）
    pub async fn admin_set_status(&self, member_id: &str, status: &str) -> Result<()> {
        if status != "active" && status != "suspended" {
            return Err(AegisError::Validation(
                "Status must be either 'active' or 'suspended'".to_string(),
            ));
        }
        self.db.set_member_status(member_id, status, &now()).await?;
        if status == "suspended" {
            // 停止した会員が、ログイン中のセッションでそのまま使い続けられないようにする
            self.revoke_sessions(member_id).await?;
        }
        Ok(())
    }

    /// パスワードの再設定を申し込む。該当する有効な会員がいれば、メールに載せる情報を返す。
    /// いない・停止中・メールの上限を超えた場合は `Ok(None)`（登録の有無を応答で教えない）。
    /// IP ごとの上限を超えたら `ResourceLimitExceeded`（どのメールアドレスでも同じ数え方）
    pub async fn request_password_reset(
        &self,
        email: &str,
        ip: Option<&str>,
    ) -> Result<Option<MemberReset>> {
        let email = normalize_email(email);
        if email.len() > 254 || email.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(AegisError::Validation(
                "A valid email address is required".to_string(),
            ));
        }
        match email.split_once('@') {
            Some((local, domain))
                if !local.is_empty()
                    && !domain.contains('@')
                    && domain.contains('.')
                    && !domain.starts_with('.')
                    && !domain.ends_with('.') => {}
            _ => {
                return Err(AegisError::Validation(
                    "A valid email address is required".to_string(),
                ))
            }
        }

        if let Some(ip) = ip {
            let count = self
                .cache
                .increment(&format!("member_reset_ip:{}", ip), Some(3600))
                .await?;
            if count > RESET_MAX_PER_IP_PER_HOUR {
                return Err(AegisError::ResourceLimitExceeded(
                    "Too many password reset requests".to_string(),
                ));
            }
        }

        let Some(member) = self.db.get_member_by_email(&email).await? else {
            return Ok(None);
        };
        if member.status == "suspended" {
            return Ok(None);
        }
        // 同じ宛先へのメールの連打を止める（宛先の数え方は、登録の有無によらない順序で、実在するときだけ）
        let sent = self
            .cache
            .increment(
                &format!("member_reset_email:{}", sha256_hex(&email)),
                Some(3600),
            )
            .await?;
        if sent > RESET_MAX_PER_EMAIL_PER_HOUR {
            return Ok(None);
        }

        let token = random_token();
        let pending = PendingToken {
            member_id: member.id.clone(),
            email: member.email.clone(),
        };
        let value =
            serde_json::to_string(&pending).map_err(|e| AegisError::Internal(e.to_string()))?;
        self.cache
            .set(
                &format!("{}{}", RESET_PREFIX, sha256_hex(&token)),
                &value,
                Some(PASSWORD_RESET_TTL_SECS),
            )
            .await?;

        Ok(Some(MemberReset {
            token,
            email: member.email,
            name: member.name,
        }))
    }

    /// トークンと新しいパスワードでパスワードを再設定する。トークンは 1 回だけ使える。
    /// 完了したら、その会員のセッションをすべて失効させる
    pub async fn complete_password_reset(
        &self,
        token: &str,
        new_password: &str,
        ip: Option<String>,
        user_agent: Option<String>,
    ) -> Result<MemberResponse> {
        let invalid =
            || AegisError::Validation("This reset link is invalid or has expired".to_string());
        let token = token.trim();
        if token.is_empty() {
            return Err(invalid());
        }
        if new_password.chars().count() < MIN_PASSWORD_LEN {
            return Err(AegisError::Validation(format!(
                "Password must be at least {} characters",
                MIN_PASSWORD_LEN
            )));
        }

        let key = format!("{}{}", RESET_PREFIX, sha256_hex(token));
        let Some(value) = self.cache.get(&key).await? else {
            return Err(invalid());
        };
        // 使えるのは 1 回だけ。失敗しても使えないようにする
        self.cache.delete(&key).await?;
        let pending: PendingToken = serde_json::from_str(&value).map_err(|_| invalid())?;

        let mut record = self
            .db
            .get_member_by_id(&pending.member_id)
            .await?
            .ok_or_else(invalid)?;
        // 申し込みのあとで、メールアドレスが変わった・停止された会員には使わせない
        if !record.email.eq_ignore_ascii_case(&pending.email) || record.status == "suspended" {
            return Err(invalid());
        }

        let password = new_password.to_string();
        record.password_hash = tokio::task::spawn_blocking(move || hash_password(&password))
            .await
            .map_err(|e| AegisError::Internal(format!("Password hashing task failed: {}", e)))??;
        // メールで受け取ったリンクを開けたので、アドレスの確認にもなる
        if record.status == "unverified" {
            record.status = "active".to_string();
        }
        record.updated_at = now();
        self.db.update_member(&record).await?;
        self.revoke_sessions(&record.id).await?;

        let event = AuditEvent::new(AuditAction::MemberPasswordReset, AuditOutcome::Success)
            .with_actor(Some(AuditActor {
                user_id: record.id.clone(),
                role: "member".to_string(),
            }))
            .with_resource("member", Some(record.id.clone()))
            .with_client(ip, user_agent);
        self.audit.record(event).await;

        Ok(record.into())
    }
}
