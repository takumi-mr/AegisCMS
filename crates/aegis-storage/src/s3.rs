use crate::config::S3StorageConfig;
use aegis_core::{
    guess_content_type, AegisError, Result, StorageBackend, StorageMeta, StorageObject,
};
use async_trait::async_trait;
use hmac::{Hmac, Mac};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use sha2::{Digest, Sha256};
use std::str::FromStr;

type HmacSha256 = Hmac<Sha256>;

/// ListObjectsV2 が 1 リクエストで返す最大件数
const LIST_PAGE_SIZE: usize = 1000;

/// SigV4 の URI エンコード（RFC 3986 の非予約文字以外をすべて `%XX` にする）
fn aws_uri_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{:02X}", byte));
        }
    }
    encoded
}

/// 署名用の正規化クエリ文字列（キー順にソートし、キー・値ともエンコード）
fn canonical_query_string(params: &[(&str, String)]) -> String {
    let mut pairs: Vec<(String, String)> = params
        .iter()
        .map(|(k, v)| (aws_uri_encode(k), aws_uri_encode(v)))
        .collect();
    pairs.sort();
    pairs
        .into_iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("&")
}

fn xml_unescape(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// `<tag>...</tag>` の中身を返す（最初の 1 件）
fn xml_tag<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(&xml[start..end])
}

/// ListObjectsV2 の 1 ページ分の結果
#[derive(Debug, Default, PartialEq)]
struct ListObjectsPage {
    /// (key, size, last_modified)
    objects: Vec<(String, u64, Option<String>)>,
    next_continuation_token: Option<String>,
}

fn parse_list_objects_response(xml: &str) -> Result<ListObjectsPage> {
    let mut page = ListObjectsPage::default();
    let mut rest = xml;
    while let Some(start) = rest.find("<Contents>") {
        let body_start = start + "<Contents>".len();
        let end = rest[body_start..].find("</Contents>").ok_or_else(|| {
            AegisError::Database("Malformed S3 ListObjectsV2 response".to_string())
        })? + body_start;
        let entry = &rest[body_start..end];
        rest = &rest[end + "</Contents>".len()..];

        let Some(key) = xml_tag(entry, "Key").map(xml_unescape) else {
            continue;
        };
        let size = xml_tag(entry, "Size")
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        let last_modified = xml_tag(entry, "LastModified").map(|s| {
            chrono::DateTime::parse_from_rfc3339(s.trim())
                .map(|t| {
                    t.with_timezone(&chrono::Utc)
                        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
                })
                .unwrap_or_else(|_| s.trim().to_string())
        });
        page.objects.push((key, size, last_modified));
    }

    if xml_tag(xml, "IsTruncated").map(str::trim) == Some("true") {
        page.next_continuation_token = xml_tag(xml, "NextContinuationToken").map(xml_unescape);
    }
    Ok(page)
}

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
        canonical_query: &str,
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
            "{}\n{}\n{}\n{}\n{}\n{}",
            method, canonical_uri, canonical_query, canonical_headers, signed_headers, payload_hash
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

        let headers = self.sign_request("PUT", path, "", &host, &payload_hash, Some(content_type));

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

        let headers = self.sign_request("GET", path, "", &host, &empty_hash, None);

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

        let headers = self.sign_request("DELETE", path, "", &host, &empty_hash, None);

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

    async fn list(&self, prefix: &str, max_objects: usize) -> Result<Vec<StorageObject>> {
        let (bucket_url, host) = self.build_endpoint_url("");
        let parsed_url = reqwest::Url::parse(&bucket_url)
            .map_err(|e| AegisError::Database(format!("Invalid S3 URL: {}", e)))?;
        let path = parsed_url.path().to_string();
        let empty_hash = hex::encode(Sha256::digest(b""));

        let mut objects = Vec::new();
        let mut continuation_token: Option<String> = None;
        while objects.len() < max_objects {
            let page_size = (max_objects - objects.len()).min(LIST_PAGE_SIZE);
            let mut params = vec![
                ("list-type", "2".to_string()),
                ("max-keys", page_size.to_string()),
            ];
            if !prefix.is_empty() {
                params.push(("prefix", prefix.to_string()));
            }
            if let Some(token) = &continuation_token {
                params.push(("continuation-token", token.clone()));
            }
            let query = canonical_query_string(&params);
            let headers = self.sign_request("GET", &path, &query, &host, &empty_hash, None);

            let mut url = parsed_url.clone();
            url.set_query(Some(&query));
            let res = self
                .client
                .get(url)
                .headers(headers)
                .send()
                .await
                .map_err(|e| AegisError::Database(format!("S3 LIST failed: {}", e)))?;

            if !res.status().is_success() {
                let status = res.status();
                let err_text = res.text().await.unwrap_or_default();
                return Err(AegisError::Database(format!(
                    "S3 LIST returned status {}: {}",
                    status, err_text
                )));
            }

            let body = res.text().await.map_err(|e| {
                AegisError::Database(format!("Failed to read S3 LIST response: {}", e))
            })?;
            let page = parse_list_objects_response(&body)?;
            for (key, size, last_modified) in page.objects {
                // "フォルダ" を表す 0 バイトのオブジェクトは除外
                if key.ends_with('/') {
                    continue;
                }
                objects.push(StorageObject {
                    url: self.public_url(&key),
                    size,
                    content_type: guess_content_type(&key).to_string(),
                    last_modified,
                    key,
                });
            }

            match page.next_continuation_token {
                Some(token) => continuation_token = Some(token),
                None => break,
            }
        }
        objects.truncate(max_objects);
        Ok(objects)
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
            "",
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

    #[test]
    fn test_canonical_query_string() {
        let query = canonical_query_string(&[
            ("prefix", "2026/10 photos".to_string()),
            ("list-type", "2".to_string()),
            ("continuation-token", "1ueG+z/a=".to_string()),
        ]);
        assert_eq!(
            query,
            "continuation-token=1ueG%2Bz%2Fa%3D&list-type=2&prefix=2026%2F10%20photos"
        );
    }

    #[test]
    fn test_parse_list_objects_response() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Name>cms-media</Name>
  <Prefix></Prefix>
  <KeyCount>3</KeyCount>
  <MaxKeys>1000</MaxKeys>
  <IsTruncated>true</IsTruncated>
  <NextContinuationToken>token&amp;next</NextContinuationToken>
  <Contents>
    <Key>2026/10/abcd1234-cat &amp; dog.png</Key>
    <LastModified>2026-10-01T12:34:56.000Z</LastModified>
    <ETag>&quot;abc&quot;</ETag>
    <Size>2048</Size>
    <StorageClass>STANDARD</StorageClass>
  </Contents>
  <Contents>
    <Key>docs/report.pdf</Key>
    <LastModified>2026-09-01T00:00:00.000Z</LastModified>
    <Size>10</Size>
  </Contents>
</ListBucketResult>"#;
        let page = parse_list_objects_response(xml).unwrap();
        assert_eq!(page.objects.len(), 2);
        assert_eq!(page.objects[0].0, "2026/10/abcd1234-cat & dog.png");
        assert_eq!(page.objects[0].1, 2048);
        assert_eq!(page.objects[0].2.as_deref(), Some("2026-10-01T12:34:56Z"));
        assert_eq!(page.next_continuation_token.as_deref(), Some("token&next"));

        let last = parse_list_objects_response(
            "<ListBucketResult><IsTruncated>false</IsTruncated></ListBucketResult>",
        )
        .unwrap();
        assert!(last.objects.is_empty());
        assert_eq!(last.next_continuation_token, None);
    }
}
