//! Admin Studio のビルド成果物（`admin/dist`）を、実行ファイルに埋め込んで配信する（Cargo feature `embed-admin`）。
//!
//! ディスクの `[paths].admin_dist_dir` があれば、そちらを優先する（バインドマウントでの差し替えや、
//! フロントだけ別に配信する使い方を変えない）。ここは、ディスクに無いときの予備

use axum::{
    body::Body,
    extract::OriginalUri,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Redirect, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../admin/dist"]
struct AdminDist;

/// 埋め込んだ `admin/dist` のファイルを返す。無いパスは SPA の `index.html`（ディスクの配信と同じ）
pub async fn serve(OriginalUri(original): OriginalUri, uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    // ビルドはアセットを相対パス（./assets/）で参照するので、末尾のスラッシュが無いと読み込めない
    if path.is_empty() && !original.path().ends_with('/') {
        let query = original
            .query()
            .map(|q| format!("?{q}"))
            .unwrap_or_default();
        return Redirect::permanent(&format!("{}/{query}", original.path())).into_response();
    }
    match AdminDist::get(path) {
        Some(file) => respond(path, file),
        None => match AdminDist::get("index.html") {
            Some(index) => respond("index.html", index),
            None => StatusCode::NOT_FOUND.into_response(),
        },
    }
}

fn respond(path: &str, file: rust_embed::EmbeddedFile) -> Response {
    // `assets/` のファイル名にはハッシュが入るので、長く持たせてよい。それ以外（index.html など）は毎回確かめる
    let cache = if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    Response::builder()
        .header(header::CONTENT_TYPE, file.metadata.mimetype())
        .header(header::CACHE_CONTROL, cache)
        .body(Body::from(file.data.into_owned()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_build_has_an_index() {
        assert!(AdminDist::get("index.html").is_some());
    }
}
