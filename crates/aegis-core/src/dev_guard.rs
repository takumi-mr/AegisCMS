//! 開発サーバー（`aegis-server theme dev` / `plugin dev`）が、ほかのサイトのページからの書き込みを断るための判定。
//!
//! 開発サーバーはループバックで動き、ファイルを書き換えたりビルドを走らせたりできる。ブラウザーで開いている
//! 別のサイトのスクリプトからも、`127.0.0.1` へはリクエストを送れてしまうので、`Host` の検査（DNS
//! リバインディング対策）に加えて、書き込みのリクエストの `Origin` が開発サーバー自身であることを確かめる。

/// 書き込みのリクエストを受けてよいか。`Origin` が付いていれば、`Host`（開発サーバー自身）と同じでなければならない。
///
/// - `Origin` が無い: ブラウザーを通っていない（`curl` やスクリプト）。受け付ける
/// - `Origin` が `Host` と同じ（スキームは問わない）: ビルダーの画面（同じオリジン、または Vite のプロキシ
///   経由で `Host` を保ったもの）。受け付ける
/// - それ以外（ほかのサイト、別のポートのページ、`null`）: 断る
pub fn is_same_origin_write(origin: Option<&str>, host: Option<&str>) -> bool {
    let Some(origin) = origin else {
        return true;
    };
    let Some(host) = host else {
        return false;
    };
    let authority = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"));
    match authority {
        Some(authority) => authority.eq_ignore_ascii_case(host.trim()),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_requests_without_an_origin_are_accepted() {
        assert!(is_same_origin_write(None, Some("127.0.0.1:8091")));
        assert!(is_same_origin_write(None, None));
    }

    #[test]
    fn test_only_the_dev_server_itself_is_accepted() {
        let host = Some("localhost:5175");
        assert!(is_same_origin_write(Some("http://localhost:5175"), host));
        assert!(is_same_origin_write(Some("http://LOCALHOST:5175"), host));
        assert!(is_same_origin_write(Some("https://localhost:5175"), host));
        // ほかのサイト・別のポート（別のローカルのアプリ）・別の名前・null・読めない値
        for origin in [
            "https://evil.example",
            "http://evil.example:5175",
            "http://localhost:3000",
            "http://localhost",
            "http://127.0.0.1:5175",
            "http://localhost:5175.evil.example",
            "http://localhost:5175@evil.example",
            "null",
            "",
            "file://",
        ] {
            assert!(!is_same_origin_write(Some(origin), host), "{origin:?}");
        }
        assert!(!is_same_origin_write(Some("http://localhost:5175"), None));
    }
}
