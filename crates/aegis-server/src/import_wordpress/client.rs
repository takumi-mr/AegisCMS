//! AegisCMS の REST API と、元のサイトからのダウンロード。

use reqwest::{Method, StatusCode};
use serde_json::Value;

pub struct Client {
    http: reqwest::Client,
    base: String,
    token: String,
}

impl Client {
    pub fn new(base: &str, token: &str) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("aegis-import/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            http,
            base: base.trim_end_matches('/').to_string(),
            token: token.to_string(),
        })
    }

    /// ユーザー名とパスワードでログインして、以後のリクエストにセッションを使う
    pub async fn login(&mut self, username: &str, password: &str) -> Result<(), String> {
        let body = serde_json::json!({ "username": username, "password": password });
        let (status, session) = self
            .json(Method::POST, "/api/v1/auth/session", Some(&body))
            .await?;
        if status != StatusCode::CREATED {
            let message = session.get("error").and_then(Value::as_str).unwrap_or("");
            return Err(format!("HTTP {} {message}", status.as_u16()));
        }
        let id = ["id", "session_id", "token"]
            .iter()
            .find_map(|key| session.get(key).and_then(Value::as_str))
            .ok_or("the server's answer has no session id (is 2FA enabled?)")?;
        self.token = id.to_string();
        Ok(())
    }

    /// JSON の API を呼ぶ。応答が JSON でなければ `Null`
    pub async fn json(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<(StatusCode, Value), String> {
        let mut request = self
            .http
            .request(method, format!("{}{}", self.base, path))
            .bearer_auth(&self.token);
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request.send().await.map_err(|e| e.to_string())?;
        let status = response.status();
        let bytes = response.bytes().await.map_err(|e| e.to_string())?;
        Ok((
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        ))
    }

    /// ファイルを `storage/upload` に置く（`key` は保存先のキー）
    pub async fn upload(
        &self,
        key: &str,
        file_name: &str,
        content_type: &str,
        data: Vec<u8>,
    ) -> Result<(StatusCode, Value), String> {
        let boundary = format!("aegis-{}", uuid::Uuid::new_v4().simple());
        let body = multipart_body(&boundary, key, file_name, content_type, &data);
        let response = self
            .http
            .post(format!("{}/api/v1/storage/upload", self.base))
            .bearer_auth(&self.token)
            .header(
                reqwest::header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(body)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = response.status();
        let bytes = response.bytes().await.map_err(|e| e.to_string())?;
        Ok((
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        ))
    }

    /// 元のサイトからファイルを取る（応答の Content-Type も返す）
    pub async fn download(&self, url: &str) -> Result<(Vec<u8>, Option<String>), String> {
        let response = self.http.get(url).send().await.map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("HTTP {}", response.status().as_u16()));
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let bytes = response.bytes().await.map_err(|e| e.to_string())?;
        Ok((bytes.to_vec(), content_type))
    }
}

/// `key` と `file` の 2 つの項目を持つ multipart の本文
fn multipart_body(
    boundary: &str,
    key: &str,
    file_name: &str,
    content_type: &str,
    data: &[u8],
) -> Vec<u8> {
    // ヘッダーの値に改行や引用符が入ると項目が壊れるので落とす
    let clean = |s: &str| s.replace(['"', '\r', '\n'], "_");
    let mut body = Vec::with_capacity(data.len() + 512);
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"key\"\r\n\r\n{}\r\n",
            key.replace(['\r', '\n'], "")
        )
        .as_bytes(),
    );
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\nContent-Type: {}\r\n\r\n",
            clean(file_name),
            clean(content_type)
        )
        .as_bytes(),
    );
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multipart_has_both_fields_and_clean_headers() {
        let body = multipart_body("B", "wordpress/a.png", "a\"\r\nx.png", "image/png", b"DATA");
        let text = String::from_utf8(body).unwrap();
        assert!(text.contains("name=\"key\"\r\n\r\nwordpress/a.png\r\n"));
        assert!(text.contains("filename=\"a___x.png\""));
        assert!(text.contains("Content-Type: image/png\r\n\r\nDATA\r\n--B--\r\n"));
    }
}
