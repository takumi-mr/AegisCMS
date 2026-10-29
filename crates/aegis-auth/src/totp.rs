use aegis_core::{AegisError, Result};
use sha2::{Digest, Sha256};
use totp_rs::{Algorithm, Builder, Secret, Totp};

/// バックアップコードの生成枚数（6枚）
pub const BACKUP_CODES_COUNT: usize = 6;

/// 2FA セットアップ情報
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TotpSetup {
    /// Base32 でエンコードされた TOTP シークレット
    pub secret: String,
    /// 認証アプリ登録用の otpauth URL
    pub otpauth_url: String,
    /// 認証アプリで読み取るための QR コード（Base64 エンコードされた PNG 画像データ）
    pub qr_code_base64: String,
}

/// 指定したユーザー名と発行者名で新しい TOTP シークレットと QR コードを生成する
pub fn generate_totp_setup(username: &str, issuer: &str) -> Result<TotpSetup> {
    let secret = Secret::generate();
    let secret_base32 = secret.to_base32();

    let totp: Totp = Builder::new()
        .with_algorithm(Algorithm::SHA1)
        .with_digits(6)
        .with_skew(1)
        .with_step_duration(30)
        .with_secret(secret.as_bytes().to_vec())
        .with_account_name(username)
        .with_issuer(Some(issuer))
        .build()
        .map_err(|e| AegisError::Internal(format!("Failed to build TOTP instance: {}", e)))?;

    let otpauth_url = totp
        .to_url()
        .map_err(|e| AegisError::Internal(format!("Failed to generate otpauth URL: {}", e)))?;

    let qr_code_base64 = totp
        .to_qr_base64()
        .map_err(|e| AegisError::Internal(format!("Failed to generate TOTP QR image: {}", e)))?;

    Ok(TotpSetup {
        secret: secret_base32,
        otpauth_url,
        qr_code_base64,
    })
}

/// TOTP コード（6桁）を Base32 シークレットに対して検証する（前後1ステップの時刻ズレを許容）
pub fn verify_totp_code(secret_base32: &str, code: &str) -> bool {
    let code_clean = code.trim().replace([' ', '-'], "");
    if code_clean.len() != 6 || !code_clean.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }

    let secret = match Secret::try_from_base32(secret_base32) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let totp: Totp = match Builder::new()
        .with_algorithm(Algorithm::SHA1)
        .with_digits(6)
        .with_skew(1)
        .with_step_duration(30)
        .with_secret(secret.as_bytes().to_vec())
        .build()
    {
        Ok(t) => t,
        Err(_) => return false,
    };

    totp.check_current(&code_clean).is_some()
}

/// バックアップコードの正規化（空白・ハイフン除去・大文字化）
fn normalize_backup_code(code: &str) -> String {
    code.trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_uppercase()
}

/// バックアップコードの SHA-256 ハッシュ文字列を計算
fn hash_backup_code(code: &str) -> String {
    let normalized = normalize_backup_code(code);
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    hex::encode(hasher.finalize())
}

/// 指定した Base32 シークレットに対する現在の TOTP コードを生成する
pub fn generate_current_totp_code(secret_base32: &str) -> Option<String> {
    let secret = Secret::try_from_base32(secret_base32).ok()?;
    let totp: Totp = Builder::new()
        .with_algorithm(Algorithm::SHA1)
        .with_digits(6)
        .with_skew(1)
        .with_step_duration(30)
        .with_secret(secret.as_bytes().to_vec())
        .build()
        .ok()?;
    Some(totp.generate_current().to_string())
}

/// 6 枚の使い捨てバックアップコードを生成する
/// 戻り値: (ユーザー表示用平文コード一覧, DB 保存用ハッシュ JSON 文字列)
pub fn generate_backup_codes() -> (Vec<String>, String) {
    let mut plain_codes = Vec::with_capacity(BACKUP_CODES_COUNT);
    let mut hashed_codes = Vec::with_capacity(BACKUP_CODES_COUNT);

    for _ in 0..BACKUP_CODES_COUNT {
        let u = uuid::Uuid::new_v4();
        let simple = u.simple().to_string();
        // 8文字を 4-4 でフォーマット (例: A1B2-C3D4)
        let formatted = format!("{}-{}", &simple[0..4], &simple[4..8]).to_uppercase();
        let hash = hash_backup_code(&formatted);
        plain_codes.push(formatted);
        hashed_codes.push(hash);
    }

    let json = serde_json::to_string(&hashed_codes).unwrap_or_else(|_| "[]".to_string());
    (plain_codes, json)
}

/// バックアップコードを検証し、一致すれば該当コードを消費（削除）した新しい JSON 文字列を返す
/// 戻り値: Some(新しい JSON 文字列)（成功時）、None（不一致または無効時）
pub fn verify_and_consume_backup_code(backup_codes_json: &str, input_code: &str) -> Option<String> {
    let mut hashed_codes: Vec<String> = serde_json::from_str(backup_codes_json).ok()?;
    let input_hash = hash_backup_code(input_code);

    if let Some(pos) = hashed_codes.iter().position(|h| h == &input_hash) {
        hashed_codes.remove(pos);
        let updated_json = serde_json::to_string(&hashed_codes).ok()?;
        Some(updated_json)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_totp_setup_and_verify() {
        let setup = generate_totp_setup("alice", "AegisCMS").unwrap();
        assert!(!setup.secret.is_empty());
        assert!(setup.otpauth_url.contains("otpauth://totp/"));
        assert!(setup.otpauth_url.contains("alice"));
        assert!(setup.otpauth_url.contains("AegisCMS"));
        assert!(!setup.qr_code_base64.is_empty());

        // 現在の有効コードを生成して検証
        let secret = Secret::try_from_base32(&setup.secret).unwrap();
        let totp: Totp = Builder::new()
            .with_algorithm(Algorithm::SHA1)
            .with_digits(6)
            .with_skew(1)
            .with_step_duration(30)
            .with_secret(secret.as_bytes().to_vec())
            .build()
            .unwrap();
        let current_code = totp.generate_current().to_string();

        assert!(verify_totp_code(&setup.secret, &current_code));
        assert!(!verify_totp_code(&setup.secret, "000000"));
        assert!(!verify_totp_code("invalid_secret", &current_code));
    }

    #[test]
    fn test_backup_codes_generation_and_consumption() {
        let (plain, json) = generate_backup_codes();
        assert_eq!(plain.len(), 6);

        let parsed: Vec<String> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 6);

        // 最初のコードを検証・消費
        let first = &plain[0];
        let remaining_json = verify_and_consume_backup_code(&json, first).expect("valid code");
        let remaining: Vec<String> = serde_json::from_str(&remaining_json).unwrap();
        assert_eq!(remaining.len(), 5);

        // 同じコードはもう一度使えない
        assert!(verify_and_consume_backup_code(&remaining_json, first).is_none());

        // ハイフンなし・小文字でも一致することを確認
        let second = &plain[1];
        let second_lower_nohyphen = second.replace('-', "").to_lowercase();
        let remaining_json2 =
            verify_and_consume_backup_code(&remaining_json, &second_lower_nohyphen)
                .expect("valid code");
        let remaining2: Vec<String> = serde_json::from_str(&remaining_json2).unwrap();
        assert_eq!(remaining2.len(), 4);

        // 不正なコード
        assert!(verify_and_consume_backup_code(&remaining_json2, "WRONG-CODE").is_none());
    }
}
