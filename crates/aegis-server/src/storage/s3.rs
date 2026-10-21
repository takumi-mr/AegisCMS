use aegis_core::{AegisError, Result, S3StorageConfig, StorageBackend, StorageMeta};
use async_trait::async_trait;
use hmac::{Hmac, Mac};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use sha2::{Digest, Sha256};
use std::str::FromStr;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone)]
pub struct S3StorageBackend {
    pub config: S3StorageConfig,
    client: reqwest::Client,
}

impl S3StorageBackend {
    pub fn new(config: S3StorageConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    /// S3 URL を組み立てる
    fn build_endpoint_url(&self, key: &str) -> (String, String) {
        let clean_key = key.trim_start_matches('/');
        let host: String;
        let url: String;

        if let Some(endpoint) = &self.config.endpoint {
            let endpoint_clean = endpoint.trim_end_matches('/');
            if self.config.force_path_style {
                // MinIO 等: http://localhost:9000/my-bucket/path/to/key
                url = format!("{}/{}/{}", endpoint_clean, self.config.bucket, clean_key);
                host = endpoint_clean
                    .trim_start_matches("http://")
                    .trim_start_matches("https://")
                    .to_string();
            } else {
                // Cloudflare R2 等: https://<account>.r2.cloudflarestorage.com/my-bucket/...
                // またはバケットサブドメイン
                if endpoint_clean.starts_with("https://") {
                    let base = endpoint_clean.trim_start_matches("https://");
                    host = format!("{}.{}", self.config.bucket, base);
                    url = format!("https://{}/{}", host, clean_key);
                } else {
                    url = format!("{}/{}/{}", endpoint_clean, self.config.bucket, clean_key);
                    host = endpoint_clean.trim_start_matches("http://").to_string();
                }
            }
        } else {
            // デフォルト AWS S3
            host = format!(
                "{}.s3.{}.amazonaws.com",
                self.config.bucket, self.config.region
            );
            url = format!("https://{}/{}", host, clean_key);
        }

        (url, host)
    }

    /// AWS SigV4 認証ヘッダーを計算
    fn sign_request(
        &self,
        method: &str,
        path: &str,
        host: &str,
        payload_hash: &str,
        content_type: Option<&str>,
    ) -> HeaderMap {
        let now = chrono::Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date_stamp = now.format("%Y%m%d").to_string();

        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_str(host).unwrap());
        headers.insert("x-amz-date", HeaderValue::from_str(&amz_date).unwrap());
        headers.insert(
            "x-amz-content-sha256",
            HeaderValue::from_str(payload_hash).unwrap(),
        );

        if let Some(ct) = content_type {
            headers.insert("content-type", HeaderValue::from_str(ct).unwrap());
        }

        // Canonical URI
        let canonical_uri = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{}", path)
        };

        let signed_headers = if content_type.is_some() {
            "content-type;host;x-amz-content-sha256;x-amz-date"
        } else {
            "host;x-amz-content-sha256;x-amz-date"
        };

        let canonical_headers = if let Some(ct) = content_type {
            format!(
                "content-type:{}\nhost:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\n",
                ct, host, payload_hash, amz_date
            )
        } else {
            format!(
                "host:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\n",
                host, payload_hash, amz_date
            )
        };

        let canonical_request = format!(
            "{}\n{}\n\n{}\n{}\n{}",
            method, canonical_uri, canonical_headers, signed_headers, payload_hash
        );

        let canonical_request_hash = hex::encode(Sha256::digest(canonical_request.as_bytes()));

        let credential_scope = format!("{}/{}/s3/aws4_request", date_stamp, self.config.region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{}\n{}\n{}",
            amz_date, credential_scope, canonical_request_hash
        );

        // Signing key calculation
        let k_secret = format!("AWS4{}", self.config.secret_access_key);
        let mut mac = HmacSha256::new_from_slice(k_secret.as_bytes()).unwrap();
        mac.update(date_stamp.as_bytes());
        let k_date = mac.finalize().into_bytes();

        let mut mac = HmacSha256::new_from_slice(&k_date).unwrap();
        mac.update(self.config.region.as_bytes());
        let k_region = mac.finalize().into_bytes();

        let mut mac = HmacSha256::new_from_slice(&k_region).unwrap();
        mac.update(b"s3");
        let k_service = mac.finalize().into_bytes();

        let mut mac = HmacSha256::new_from_slice(&k_service).unwrap();
        mac.update(b"aws4_request");
        let k_signing = mac.finalize().into_bytes();

        let mut mac = HmacSha256::new_from_slice(&k_signing).unwrap();
        mac.update(string_to_sign.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());

        let auth_header = format!(
            "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            self.config.access_key_id, credential_scope, signed_headers, signature
        );

        headers.insert(
            HeaderName::from_str("authorization").unwrap(),
            HeaderValue::from_str(&auth_header).unwrap(),
        );

        headers
    }
}

#[async_trait]
impl StorageBackend for S3StorageBackend {
    async fn put(&self, key: &str, data: &[u8], content_type: &str) -> Result<StorageMeta> {
        let (url, host) = self.build_endpoint_url(key);
        let payload_hash = hex::encode(Sha256::digest(data));

        // URL からパス部分を抽出
        let parsed_url = reqwest::Url::parse(&url)
            .map_err(|e| AegisError::Database(format!("Invalid S3 URL: {}", e)))?;
        let path = parsed_url.path();

        let headers = self.sign_request("PUT", path, &host, &payload_hash, Some(content_type));

        let res = self
            .client
            .put(&url)
            .headers(headers)
            .body(data.to_vec())
            .send()
            .await
            .map_err(|e| AegisError::Database(format!("S3 PUT failed: {}", e)))?;

        if !res.status().is_success() {
            let status = res.status();
            let err_text = res.text().await.unwrap_or_default();
            return Err(AegisError::Database(format!(
                "S3 PUT returned status {}: {}",
                status, err_text
            )));
        }

        let public_url = self.public_url(key);
        Ok(StorageMeta {
            key: key.to_string(),
            url: public_url,
            size: data.len(),
            content_type: content_type.to_string(),
        })
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>> {
        let (url, host) = self.build_endpoint_url(key);
        let empty_hash = hex::encode(Sha256::digest(b""));

        let parsed_url = reqwest::Url::parse(&url)
            .map_err(|e| AegisError::Database(format!("Invalid S3 URL: {}", e)))?;
        let path = parsed_url.path();

        let headers = self.sign_request("GET", path, &host, &empty_hash, None);

        let res = self
            .client
            .get(&url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| AegisError::Database(format!("S3 GET failed: {}", e)))?;

        if !res.status().is_success() {
            let status = res.status();
            let err_text = res.text().await.unwrap_or_default();
            return Err(AegisError::Database(format!(
                "S3 GET returned status {}: {}",
                status, err_text
            )));
        }

        let bytes = res.bytes().await.map_err(|e| {
            AegisError::Database(format!("Failed to read S3 response bytes: {}", e))
        })?;

        Ok(bytes.to_vec())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let (url, host) = self.build_endpoint_url(key);
        let empty_hash = hex::encode(Sha256::digest(b""));

        let parsed_url = reqwest::Url::parse(&url)
            .map_err(|e| AegisError::Database(format!("Invalid S3 URL: {}", e)))?;
        let path = parsed_url.path();

        let headers = self.sign_request("DELETE", path, &host, &empty_hash, None);

        let res = self
            .client
            .delete(&url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| AegisError::Database(format!("S3 DELETE failed: {}", e)))?;

        if !res.status().is_success() {
            let status = res.status();
            let err_text = res.text().await.unwrap_or_default();
            return Err(AegisError::Database(format!(
                "S3 DELETE returned status {}: {}",
                status, err_text
            )));
        }

        Ok(())
    }

    fn public_url(&self, key: &str) -> String {
        let clean_key = key.trim_start_matches('/');
        if let Some(prefix) = &self.config.public_url_prefix {
            format!("{}/{}", prefix.trim_end_matches('/'), clean_key)
        } else {
            let (url, _) = self.build_endpoint_url(key);
            url
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_s3_url_building_aws_default() {
        let config = S3StorageConfig {
            bucket: "my-bucket".to_string(),
            region: "ap-northeast-1".to_string(),
            endpoint: None,
            access_key_id: "test-access".to_string(),
            secret_access_key: "test-secret".to_string(),
            force_path_style: false,
            public_url_prefix: None,
        };
        let backend = S3StorageBackend::new(config);
        let (url, host) = backend.build_endpoint_url("images/photo.png");
        assert_eq!(
            url,
            "https://my-bucket.s3.ap-northeast-1.amazonaws.com/images/photo.png"
        );
        assert_eq!(host, "my-bucket.s3.ap-northeast-1.amazonaws.com");
    }

    #[test]
    fn test_s3_url_building_minio_path_style() {
        let config = S3StorageConfig {
            bucket: "cms-media".to_string(),
            region: "us-east-1".to_string(),
            endpoint: Some("http://localhost:9000".to_string()),
            access_key_id: "minioadmin".to_string(),
            secret_access_key: "minioadmin".to_string(),
            force_path_style: true,
            public_url_prefix: Some("https://cdn.example.com/assets".to_string()),
        };
        let backend = S3StorageBackend::new(config);
        let (url, _) = backend.build_endpoint_url("docs/report.pdf");
        assert_eq!(url, "http://localhost:9000/cms-media/docs/report.pdf");

        let pub_url = backend.public_url("docs/report.pdf");
        assert_eq!(pub_url, "https://cdn.example.com/assets/docs/report.pdf");
    }

    #[test]
    fn test_sigv4_headers_generation() {
        let config = S3StorageConfig {
            bucket: "test-bucket".to_string(),
            region: "us-east-1".to_string(),
            endpoint: None,
            access_key_id: "AKIAIOSFODNN7EXAMPLE".to_string(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            force_path_style: false,
            public_url_prefix: None,
        };
        let backend = S3StorageBackend::new(config);
        let headers = backend.sign_request(
            "PUT",
            "/test.txt",
            "test-bucket.s3.us-east-1.amazonaws.com",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            Some("text/plain"),
        );

        assert!(headers.contains_key("authorization"));
        assert!(headers.contains_key("x-amz-date"));
        assert!(headers.contains_key("x-amz-content-sha256"));
        let auth = headers.get("authorization").unwrap().to_str().unwrap();
        assert!(auth.starts_with("AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/"));
    }
}
