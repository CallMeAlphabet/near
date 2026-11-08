use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    extract::State,
    http::{header, Method, StatusCode, Uri},
    response::Response,
};

use crate::hub::AppState;

pub const PAGE: &str = include_str!("../web/index.html");

fn mime(path: &Path) -> &'static str {
    match path.extension().and_then(|value| value.to_str()).unwrap_or("") {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "webmanifest" => "application/manifest+json",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn html(body: &'static str, status: StatusCode) -> Response {
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response
}

fn not_found() -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::NOT_FOUND;
    response
}

fn page() -> Response {
    html(PAGE, StatusCode::OK)
}

fn file(path: &Path) -> Response {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return not_found(),
    };
    let mut response = Response::new(Body::from(bytes));
    let value = header::HeaderValue::from_str(mime(path))
        .unwrap_or_else(|_| header::HeaderValue::from_static("application/octet-stream"));
    response.headers_mut().insert(header::CONTENT_TYPE, value);
    response
}

fn from_root(root: &PathBuf, uri: &Uri) -> Response {
    let relative = uri.path().trim_start_matches('/');
    if relative.contains("..") || relative.contains('\\') {
        return not_found();
    }

    if !relative.is_empty() {
        let candidate = root.join(relative);
        if candidate.is_file() {
            let inside = match (root.canonicalize(), candidate.canonicalize()) {
                (Ok(base), Ok(target)) => target.starts_with(base),
                _ => false,
            };
            return if inside { file(&candidate) } else { not_found() };
        }
    }

    if !Path::new(relative).extension().is_some_and(|ext| !ext.is_empty()) {
        let index = root.join("index.html");
        if index.is_file() {
            return file(&index);
        }
        return page();
    }

    not_found()
}

pub async fn serve(State(app): State<AppState>, method: Method, uri: Uri) -> Response {
    if method != Method::GET && method != Method::HEAD {
        let mut response = Response::new(Body::empty());
        *response.status_mut() = StatusCode::METHOD_NOT_ALLOWED;
        return response;
    }
    match &app.web_root {
        Some(root) => from_root(root, &uri),
        None => {
            let path = uri.path();
            let asset = Path::new(path).extension().is_some_and(|ext| !ext.is_empty());
            if asset && path != "/index.html" {
                let mut response = Response::new(Body::empty());
                *response.status_mut() = StatusCode::NOT_FOUND;
                return response;
            }
            page()
        }
    }
}
