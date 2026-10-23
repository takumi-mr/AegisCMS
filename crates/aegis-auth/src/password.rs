use aegis_core::{AegisError, Result};
use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, SaltString};
use argon2::{Argon2, PasswordVerifier};
use std::sync::OnceLock;

/// ローカルユーザーのパスワードの最小文字数
pub const MIN_PASSWORD_LEN: usize = 8;

/// パスワードを Argon2id（既定パラメータ）でハッシュし、PHC 文字列を返す
pub fn hash_password(password: &str) -> Result<String> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(AegisError::Validation(format!(
            "Password must be at least {} characters",
            MIN_PASSWORD_LEN
        )));
    }
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| AegisError::Internal(format!("Failed to hash password: {}", e)))
}

/// パスワードが PHC 文字列のハッシュと一致するか検証する
pub fn verify_password(password: &str, password_hash: &str) -> Result<bool> {
    let parsed = PasswordHash::new(password_hash)
        .map_err(|e| AegisError::Internal(format!("Invalid stored password hash: {}", e)))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

/// 存在しないユーザーへのログインでも同じだけ時間をかけ、応答時間からユーザー名の有無を
/// 推測されないようにするためのダミー検証
pub fn verify_dummy_password(password: &str) {
    static DUMMY_HASH: OnceLock<String> = OnceLock::new();
    let hash = DUMMY_HASH.get_or_init(|| {
        hash_password("aegis-dummy-password").expect("hashing a constant password succeeds")
    });
    let _ = verify_password(password, hash);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify_password() {
        let hash = hash_password("correct horse battery").unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_password("correct horse battery", &hash).unwrap());
        assert!(!verify_password("wrong password", &hash).unwrap());

        // 同じパスワードでもソルトが違うのでハッシュは毎回変わる
        assert_ne!(hash, hash_password("correct horse battery").unwrap());
    }

    #[test]
    fn test_hash_password_rejects_short_password() {
        assert!(matches!(
            hash_password("short"),
            Err(AegisError::Validation(_))
        ));
    }

    #[test]
    fn test_verify_password_rejects_malformed_hash() {
        assert!(verify_password("anything", "not-a-phc-string").is_err());
    }
}
