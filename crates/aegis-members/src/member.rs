//! 会員アカウント（Member Account）機能。
//! AegisCMS の管理アカウント（`_aegis_users`）とは完全に別管理の、訪問者の会員アカウント。
//! ショップ・掲示板などが共通で使う。注文やレビューなど、機能ごとのデータは扱わない
//! （退会のときの後始末は、呼び出す側が `delete_account` の前に行う）。

use aegis_core::time::TimestampExt;
use std::sync::Arc;

use aegis_audit::AuditService;
use aegis_auth::password::{
    hash_password, verify_dummy_password, verify_password, MIN_PASSWORD_LEN,
};
use aegis_core::{
    AegisError, AuditAction, AuditActor, AuditEvent, AuditOutcome, CacheBackend, Result,
    ShippingAddress,
};
use aegis_db::{DatabaseAdapter, MemberProfileUpdate, MemberRecord};

use crate::config::{MemberRegistration, MembersConfig};
use jiff::Timestamp;
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
/// 確認リンクで入れたパスワードが、そのリンクの登録のものと違うときのメッセージ
pub const VERIFY_PASSWORD_MISMATCH_MESSAGE: &str =
    "The password does not match the registration of this link";
/// 確認リンクで、パスワードを間違えてよい回数（超えたらリンクを使えなくする）
const MAX_VERIFY_PASSWORD_ATTEMPTS: u64 = 5;
/// 読んでから書くまでの間に、ほかの操作で会員が変わったときのメッセージ
const CHANGED_WHILE_SAVING_MESSAGE: &str = "The account changed while saving. Try again";
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
    /// メールの確認: そのリンクを作った登録の内容。確認のときに初めて会員の行へ反映する（確認待ちの会員の行は、
    /// 登録し直しでは書き換えない。他人が同じアドレスで登録し直しても、本人のリンクは本人のパスワードでしか
    /// 有効にならない）。この版より前に作ったリンクには無い（会員の行の内容を使う）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    registration: Option<PendingRegistration>,
}

/// 確認待ちの登録の内容（[`PendingToken::registration`]）
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingRegistration {
    /// Argon2id のハッシュ（平文は持たない）
    password_hash: String,
    name: String,
    phone: Option<String>,
}

impl From<&MemberRecord> for PendingRegistration {
    fn from(record: &MemberRecord) -> Self {
        Self {
            password_hash: record.password_hash.clone(),
            name: record.name.clone(),
            phone: record.phone.clone(),
        }
    }
}

/// パスワードのハッシュを作る（Argon2id は重いので、非同期のワーカーを止めないよう別のスレッドで）
async fn hash_blocking(password: &str) -> Result<String> {
    let password = password.to_string();
    tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .map_err(|e| AegisError::Internal(format!("Password hashing task failed: {}", e)))?
}

/// パスワードを確かめる（[`hash_blocking`] と同じく別のスレッドで）
async fn verify_blocking(password: &str, hash: &str) -> Result<bool> {
    let (password, hash) = (password.to_string(), hash.to_string());
    tokio::task::spawn_blocking(move || verify_password(&password, &hash))
        .await
        .map_err(|e| AegisError::Internal(format!("Password check task failed: {}", e)))?
}

/// 会員がいないときも、同じだけ時間をかける（登録の有無を応答の時間で教えない）
async fn verify_dummy_blocking(password: &str) {
    let password = password.to_string();
    let _ = tokio::task::spawn_blocking(move || verify_dummy_password(&password)).await;
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
    /// 権限レベル（`[members].levels` のどれか）
    pub level: String,
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
            level: r.level,
            shipping_address,
            created_at: r.created_at,
            updated_at: r.updated_at,
            last_login_at: r.last_login_at,
        }
    }
}

fn now() -> String {
    Timestamp::now().to_rfc3339_secs()
}

fn future_timestamp(seconds: u64) -> String {
    (Timestamp::now() + jiff::SignedDuration::from_secs(seconds as i64)).to_rfc3339_secs()
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
    /// `[members].levels`（低 → 高）。先頭が新規会員のレベル
    levels: Vec<String>,
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
                .with_levels(config.levels.clone())
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
            levels: MembersConfig::default().levels,
        }
    }

    pub fn with_ttl(mut self, ttl: u64) -> Self {
        self.session_ttl_secs = ttl;
        self
    }

    /// 新規の会員登録を受け付けるか（`false` でも、登録済みの会員はログインできる）
    /// 権限レベルの一覧（低 → 高）。空なら変えない
    pub fn with_levels(mut self, levels: Vec<String>) -> Self {
        if !levels.is_empty() {
            self.levels = levels;
        }
        self
    }

    /// 権限レベルの一覧（低 → 高）
    pub fn levels(&self) -> &[String] {
        &self.levels
    }

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
        let password_hash = hash_blocking(&req.password).await?;
        self.insert_member(req, email, "active", password_hash, ip, user_agent)
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
        password_hash: String,
        ip: Option<String>,
        user_agent: Option<String>,
    ) -> Result<MemberRecord> {
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
            level: self.levels[0].clone(),
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
    /// 登録済みのアドレスは、応答で区別できないよう `Ok(None)` を返す。
    ///
    /// 登録の内容（パスワード・名前・電話）は、確認メールのトークンに持たせ、確認のとき（[`Self::verify_email`]。
    /// そのリンクの登録のパスワードを入れたときだけ）に初めて会員の行へ反映する。まだ確認していない会員の
    /// アドレスで登録し直しても、会員の行は書き換えず、新しい登録のリンクを送る。他人が同じアドレスで
    /// 登録し直しても、そのリンクは他人のパスワードを知らなければ有効にできない（アカウントを乗っ取れない）
    pub async fn register_pending(
        &self,
        req: MemberRegisterRequest,
        ip: Option<String>,
        user_agent: Option<String>,
    ) -> Result<Option<MemberVerification>> {
        let email = self.check_registration(&req)?;
        let registration = PendingRegistration {
            password_hash: hash_blocking(&req.password).await?,
            name: req.name.trim().to_string(),
            phone: req.phone.as_ref().map(|p| p.trim().to_string()),
        };
        let member = match self.db.get_member_by_email(&email).await? {
            None => {
                let hash = registration.password_hash.clone();
                self.insert_member(req, email, "unverified", hash, ip, user_agent)
                    .await?
            }
            // 確認待ち。行はそのまま（最初の登録の内容）で、この登録のリンクを送る
            Some(existing) if existing.status == "unverified" => existing,
            // 登録済み（有効・停止中・退会の手続き中）。登録の有無を応答で教えない
            Some(_) => return Ok(None),
        };
        self.issue_verification(member, registration).await
    }

    /// 確認メールの情報を作る。同じ宛先へのメールは 1 時間に 3 通まで（超えた分は送らず `None`）
    async fn issue_verification(
        &self,
        member: MemberRecord,
        registration: PendingRegistration,
    ) -> Result<Option<MemberVerification>> {
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
            registration: Some(registration),
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
        if !verify_blocking(password, &record.password_hash).await? {
            return Err(AegisError::Authentication(
                "Password does not match".to_string(),
            ));
        }
        Ok(())
    }

    /// 退会を始める（`confirm_password` のあと、機能ごとのデータの後始末の前に呼ぶ）。会員を `deleting` に
    /// して、セッションをすべて終わらせる。後始末（サブスクの解約・注文やレビューの匿名化など）の間に、
    /// 同じ会員の新しいレビュー・注文・投稿・契約が作られないようにするため。`active` の会員だけ
    /// （ほかは `PermissionDenied`）。すでに `deleting` なら（前の退会が途中で止まった）、そのまま続ける
    pub async fn begin_account_deletion(&self, member_id: &str) -> Result<()> {
        let started = self
            .db
            .set_member_status(member_id, "deleting", &now(), Some(&["active"]))
            .await?;
        if !started {
            let record = self
                .db
                .get_member_by_id(member_id)
                .await?
                .ok_or_else(|| AegisError::NotFound("Member not found".to_string()))?;
            if record.status != "deleting" {
                return Err(AegisError::PermissionDenied(
                    "This member account has been suspended".to_string(),
                ));
            }
        }
        self.revoke_sessions(member_id).await
    }

    /// 退会の後始末に失敗したとき、`deleting` の会員を `active` に戻す（もう一度ログインして、やり直せる）
    pub async fn abort_account_deletion(&self, member_id: &str) -> Result<()> {
        self.db
            .set_member_status(member_id, "active", &now(), Some(&["deleting"]))
            .await?;
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
            // 送り直しのリンクは、会員の行の内容（最初の登録）で有効になる。そのパスワードを知っている人
            // （最初に登録した人）でなければ有効にできない。他人が先に登録していたら、本人は登録し直す
            Some(member) if member.status == "unverified" => {
                let registration = PendingRegistration::from(&member);
                self.issue_verification(member, registration).await
            }
            _ => Ok(None),
        }
    }

    /// 確認リンクのトークンと、そのリンクの登録のパスワードで、メールアドレスを確認する（会員は `active` に
    /// なり、登録の内容が反映される）。パスワードが違えば `Authentication`（リンクは
    /// [`MAX_VERIFY_PASSWORD_ATTEMPTS`] 回まで間違えられ、超えると使えなくなる）。トークンは 1 回だけ
    pub async fn verify_email(
        &self,
        token: &str,
        password: &str,
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
        let pending: PendingToken = serde_json::from_str(&value).map_err(|_| invalid())?;
        let record = self
            .db
            .get_member_by_id(&pending.member_id)
            .await?
            .ok_or_else(invalid)?;
        if !record.email.eq_ignore_ascii_case(&pending.email) {
            return Err(invalid());
        }
        // この版より前に作ったリンク（登録の内容を持たない）は、会員の行の内容で確かめる
        let registration = pending
            .registration
            .unwrap_or_else(|| PendingRegistration::from(&record));

        // リンクを作った登録のパスワードでなければ、有効にしない（他人の登録し直しで届いたリンクを、
        // 本人が開いても、他人のパスワードのアカウントにならない）
        if !verify_blocking(password, &registration.password_hash).await? {
            let failures = self
                .cache
                .increment(
                    &format!("member_verify_fail:{}", sha256_hex(token)),
                    Some(EMAIL_VERIFICATION_TTL_SECS),
                )
                .await?;
            if failures >= MAX_VERIFY_PASSWORD_ATTEMPTS {
                self.cache.delete(&key).await?;
            }
            return Err(AegisError::Authentication(
                VERIFY_PASSWORD_MISMATCH_MESSAGE.to_string(),
            ));
        }
        self.cache.delete(&key).await?;

        match record.status.as_str() {
            "unverified" => {
                let activated = self
                    .db
                    .activate_member(
                        &record.id,
                        &registration.password_hash,
                        &registration.name,
                        registration.phone.as_deref(),
                        &now(),
                    )
                    .await?;
                if activated {
                    let event = AuditEvent::new(AuditAction::MemberVerify, AuditOutcome::Success)
                        .with_actor(Some(AuditActor {
                            user_id: record.id.clone(),
                            role: "member".to_string(),
                        }))
                        .with_resource("member", Some(record.id.clone()))
                        .with_client(ip, user_agent);
                    self.audit.record(event).await;
                }
            }
            // すでに確認済み（別のリンクで確認した）。何も変えない
            "active" => {}
            _ => return Err(invalid()),
        }
        let fresh = self
            .db
            .get_member_by_id(&record.id)
            .await?
            .ok_or_else(invalid)?;
        // 同時に開いた別のリンクで確認済みなら、それでよい。停止・退会などに変わっていたら使えない
        if fresh.status != "active" {
            return Err(invalid());
        }
        Ok(fresh.into())
    }

    /// ログイン
    pub async fn login(
        &self,
        req: MemberLoginRequest,
        ip: Option<String>,
        user_agent: Option<String>,
    ) -> Result<(MemberSessionData, MemberResponse)> {
        // セッションは、会員の行を読む前の時刻で作る。パスワードを確かめている間に停止・パスワードの変更・
        // 退会が挟まって失効の印（`revoke_sessions`）が置かれたら、このセッションも失効している
        let started_micros = Timestamp::now().as_microsecond();
        let email = normalize_email(&req.email);
        let member = self.db.get_member_by_email(&email).await?;

        let record = match member {
            Some(c) => c,
            None => {
                // タイミング攻撃防御
                verify_dummy_blocking(&req.password).await;
                return Err(AegisError::Authentication(
                    "Invalid email or password".to_string(),
                ));
            }
        };

        if !verify_blocking(&req.password, &record.password_hash).await? {
            return Err(AegisError::Authentication(
                "Invalid email or password".to_string(),
            ));
        }
        // 確かめている間に変わっていないか、今の行で見直す（停止・退会・パスワードの変更）
        let record = match self.db.get_member_by_id(&record.id).await? {
            Some(current) if current.password_hash == record.password_hash => current,
            _ => {
                return Err(AegisError::Authentication(
                    "Invalid email or password".to_string(),
                ))
            }
        };

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

        let session = self.create_session(&record, started_micros).await?;

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
    /// 会員のセッションを作って、キャッシュに置く。`issued_at_micros` は、会員の行を読む前に取った時刻
    /// （読んでから作るまでの間に置かれた失効の印で、このセッションも失効するように）
    async fn create_session(
        &self,
        record: &MemberRecord,
        issued_at_micros: i64,
    ) -> Result<MemberSessionData> {
        let token = format!("{}{}", MEMBER_TOKEN_PREFIX, Uuid::new_v4().simple());
        let session = MemberSessionData {
            token: token.clone(),
            member_id: record.id.clone(),
            email: record.email.clone(),
            name: record.name.clone(),
            created_at: now(),
            expires_at: future_timestamp(self.session_ttl_secs),
            issued_at_micros,
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
        let revoked_at = Timestamp::now().as_microsecond();
        self.cache
            .set(&key, &revoked_at.to_string(), Some(self.session_ttl_secs))
            .await?;
        Ok(())
    }

    /// 同じ会員の新しいセッションを作る（パスワードの変更で全セッションを失効させたあと、
    /// 変更した本人の画面を続けて使えるようにするため）
    pub async fn reissue_session(&self, member_id: &str) -> Result<MemberSessionData> {
        let started_micros = Timestamp::now().as_microsecond();
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
        self.create_session(&record, started_micros).await
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

    /// アバター画像のキーを設定する（`None` で外す）。以前のキーを返す（呼び出し側が古い画像を消す）。
    /// `active` の会員だけ。同時に差し替えたときは、片方が `Validation`（以前のキーを二重に返さない）
    pub async fn set_avatar(
        &self,
        member_id: &str,
        avatar_key: Option<String>,
    ) -> Result<(MemberResponse, Option<String>)> {
        let record = self.active_member(member_id).await?;
        let previous = record.avatar_key.clone();
        let changed = self
            .db
            .update_member_avatar(
                member_id,
                avatar_key.as_deref(),
                &now(),
                Some(previous.as_deref()),
            )
            .await?;
        if !changed {
            self.active_member(member_id).await?;
            return Err(AegisError::Validation(
                CHANGED_WHILE_SAVING_MESSAGE.to_string(),
            ));
        }
        let fresh = self.active_member(member_id).await?;
        Ok((fresh.into(), previous))
    }

    /// 有効な（`active`）会員の行。いなければ `NotFound`、有効でなければ `PermissionDenied`
    async fn active_member(&self, member_id: &str) -> Result<MemberRecord> {
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
        Ok(record)
    }

    /// プロフィール更新。`active` の会員だけ。変える列だけを書き、パスワードは今のハッシュが確かめたものの
    /// ときだけ書く（読んでから書くまでの間の、管理者の停止・レベルの変更・別のパスワードの変更を上書きしない）
    pub async fn update_profile(
        &self,
        member_id: &str,
        req: MemberUpdateProfileRequest,
    ) -> Result<MemberResponse> {
        let record = self.active_member(member_id).await?;
        let mut profile = MemberProfileUpdate {
            name: record.name.clone(),
            nickname: record.nickname.clone(),
            phone: record.phone.clone(),
            shipping_address_json: record.shipping_address_json.clone(),
            updated_at: now(),
        };
        let changes_profile = req.name.is_some()
            || req.nickname.is_some()
            || req.phone.is_some()
            || req.shipping_address.is_some();

        if let Some(name) = req.name {
            if name.trim().is_empty() {
                return Err(AegisError::Validation("Name cannot be empty".to_string()));
            }
            profile.name = name.trim().to_string();
        }

        if let Some(nickname) = req.nickname {
            profile.nickname = normalize_nickname(&nickname)?;
        }

        if let Some(phone) = req.phone {
            profile.phone = if phone.trim().is_empty() {
                None
            } else {
                Some(phone.trim().to_string())
            };
        }

        if let Some(ref addr) = req.shipping_address {
            profile.shipping_address_json = Some(
                serde_json::to_string(addr).map_err(|e| AegisError::Validation(e.to_string()))?,
            );
        }

        // パスワード変更（ほかの変更より先に。確かめたあとで変わっていたら、何も変えずに止める）
        if let Some(ref new_pw) = req.new_password {
            let current_pw = req.current_password.as_deref().ok_or_else(|| {
                AegisError::Validation(
                    "Current password is required to set a new password".to_string(),
                )
            })?;
            if new_pw.chars().count() < MIN_PASSWORD_LEN {
                return Err(AegisError::Validation(format!(
                    "New password must be at least {} characters",
                    MIN_PASSWORD_LEN
                )));
            }
            if !verify_blocking(current_pw, &record.password_hash).await? {
                return Err(AegisError::Authentication(
                    "Current password does not match".to_string(),
                ));
            }
            let new_hash = hash_blocking(new_pw).await?;
            let changed = self
                .db
                .update_member_password(member_id, &new_hash, &record.password_hash, &now())
                .await?;
            if !changed {
                self.active_member(member_id).await?;
                return Err(AegisError::Validation(
                    CHANGED_WHILE_SAVING_MESSAGE.to_string(),
                ));
            }
            // ほかの端末のセッションも終わらせる（呼び出し元が、本人の分は作り直す）
            self.revoke_sessions(member_id).await?;
        }

        if changes_profile && !self.db.update_member_profile(member_id, &profile).await? {
            return Err(AegisError::PermissionDenied(
                "This member account has been suspended".to_string(),
            ));
        }

        // 監査ログ
        let event = AuditEvent::new(AuditAction::MemberUpdate, AuditOutcome::Success)
            .with_actor(Some(AuditActor {
                user_id: member_id.to_string(),
                role: "member".to_string(),
            }))
            .with_resource("member", Some(member_id.to_string()));
        self.audit.record(event).await;

        self.get_profile(member_id).await
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
        // 退会の手続き中（`deleting`）の会員は、管理者の操作で戻さない
        let changed = self
            .db
            .set_member_status(
                member_id,
                status,
                &now(),
                Some(&["active", "suspended", "unverified"]),
            )
            .await?;
        if !changed {
            return Err(AegisError::NotFound("Member not found".to_string()));
        }
        if status == "suspended" {
            // 停止した会員が、ログイン中のセッションでそのまま使い続けられないようにする
            self.revoke_sessions(member_id).await?;
        }
        Ok(())
    }

    /// 管理者: 会員の権限レベルを変える。`[members].levels` にあるレベルだけ（`Validation`）。
    /// 変えても、会員のセッションはそのまま使える（レベルは閲覧のたびに引くため、すぐ反映される）
    pub async fn admin_set_level(
        &self,
        member_id: &str,
        level: &str,
        actor_id: &str,
    ) -> Result<()> {
        if !self.levels.iter().any(|l| l == level) {
            return Err(AegisError::Validation(format!(
                "Level must be one of: {}",
                self.levels.join(", ")
            )));
        }
        let record = self
            .db
            .get_member_by_id(member_id)
            .await?
            .ok_or_else(|| AegisError::NotFound("Member not found".to_string()))?;
        let previous = record.level;
        if !self.db.set_member_level(member_id, level, &now()).await? {
            return Err(AegisError::NotFound("Member not found".to_string()));
        }

        let event = AuditEvent::new(AuditAction::MemberUpdate, AuditOutcome::Success)
            .with_actor(Some(AuditActor {
                user_id: actor_id.to_string(),
                role: "admin".to_string(),
            }))
            .with_resource("member", Some(member_id.to_string()))
            .with_details(json!({ "level": { "from": previous, "to": level } }));
        self.audit.record(event).await;
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
        // 停止中・退会の手続き中の会員には送らない
        if member.status != "active" && member.status != "unverified" {
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
            registration: None,
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

        let record = self
            .db
            .get_member_by_id(&pending.member_id)
            .await?
            .ok_or_else(invalid)?;
        // 申し込みのあとで、メールアドレスが変わった・停止された会員には使わせない
        if !record.email.eq_ignore_ascii_case(&pending.email)
            || (record.status != "active" && record.status != "unverified")
        {
            return Err(invalid());
        }

        let password_hash = hash_blocking(new_password).await?;
        // `active` か `unverified` のときだけ書く（ハッシュを作っている間に停止・退会されたら書かない）。
        // メールで受け取ったリンクを開けたので、アドレスの確認にもなる（`unverified` は `active` になる）
        if !self
            .db
            .reset_member_password(&record.id, &password_hash, &now())
            .await?
        {
            return Err(invalid());
        }
        self.revoke_sessions(&record.id).await?;
        let record = self
            .db
            .get_member_by_id(&record.id)
            .await?
            .ok_or_else(invalid)?;

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
