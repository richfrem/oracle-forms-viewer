use std::path::{Path, PathBuf};
use axum::extract::{DefaultBodyLimit, FromRequest, Multipart, Path as AxumPath, Query, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use oracle_forms_core::model::*;
use crate::assets::static_handler;
use crate::service::FormsService;

pub fn app_router(service: FormsService) -> Router {
    Router::new()
        .route("/api/samples", get(get_samples))
        .route("/api/open-sample/{name}", post(open_sample))
        .route("/api/open-file", post(open_file).layer(DefaultBodyLimit::max(50 * 1024 * 1024)))
        .route("/api/module/{module_name}/source", get(get_source))
        .route("/api/module/{module_name}/xml", get(get_xml))
        .route("/api/module/{module_name}/effective-item", get(get_effective_item))
        .fallback(static_handler)
        .with_state(service)
}

async fn get_samples(State(service): State<FormsService>) -> impl IntoResponse {
    let samples = service.get_sample_list();
    Json(samples)
}

async fn open_sample(
    AxumPath(name): AxumPath<String>,
    State(service): State<FormsService>,
) -> Response {
    match service.load_sample(&name) {
        Ok(index) => Json(index.to_payload()).into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: e.to_string(),
                requires_conversion: false,
                sample_options: service.get_sample_list(),
            }),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct SourceQuery {
    path: Option<String>,
}

async fn get_source(
    AxumPath(module_name): AxumPath<String>,
    Query(query): Query<SourceQuery>,
    State(service): State<FormsService>,
) -> Response {
    let rel_path = match query.path {
        Some(p) if !p.trim().is_empty() => p,
        _ => return (StatusCode::BAD_REQUEST, "Query parameter 'path' is required.").into_response(),
    };

    match service.read_source(&module_name, &rel_path) {
        Some(code) => Json(CodeResponse {
            module_name,
            path: rel_path,
            code,
        })
        .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            format!("Source not found for {} at {}", module_name, rel_path),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct XmlQuery {
    #[serde(rename = "startLine")]
    start_line: Option<usize>,
    #[serde(rename = "endLine")]
    end_line: Option<usize>,
}

async fn get_xml(
    AxumPath(module_name): AxumPath<String>,
    Query(query): Query<XmlQuery>,
    State(service): State<FormsService>,
) -> Response {
    let start_line = query.start_line.unwrap_or(1);
    let end_line = query.end_line.unwrap_or(start_line);

    match service.read_xml_slice(&module_name, start_line, end_line) {
        Some(xml) => Json(XmlSliceResponse {
            module_name,
            start_line,
            end_line,
            xml,
        })
        .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            format!("XML slice not found for {}", module_name),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct EffectiveItemQuery {
    block: Option<String>,
    item: Option<String>,
}

async fn get_effective_item(
    AxumPath(module_name): AxumPath<String>,
    Query(query): Query<EffectiveItemQuery>,
    State(service): State<FormsService>,
) -> Response {
    let block_name = match query.block {
        Some(b) => b,
        None => return (StatusCode::BAD_REQUEST, "Query param 'block' required").into_response(),
    };
    let item_name = match query.item {
        Some(i) => i,
        None => return (StatusCode::BAD_REQUEST, "Query param 'item' required").into_response(),
    };

    let index = match service.get_module(&module_name) {
        Some(idx) => idx,
        None => return (StatusCode::NOT_FOUND, "Module not found").into_response(),
    };

    let block = match index.blocks.iter().find(|b| b.name.eq_ignore_ascii_case(&block_name)) {
        Some(b) => b,
        None => return (StatusCode::NOT_FOUND, format!("Block {} not found", block_name)).into_response(),
    };

    let item = match block.items.iter().find(|i| i.name.eq_ignore_ascii_case(&item_name)) {
        Some(i) => i,
        None => return (StatusCode::NOT_FOUND, format!("Item {}.{} not found", block_name, item_name)).into_response(),
    };

    let eff = service.resolve_effective_item(&module_name, item);
    Json(eff).into_response()
}

async fn open_file(
    State(service): State<FormsService>,
    req: Request,
) -> Response {
    let content_type = req
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if content_type.starts_with("multipart/form-data") {
        let upload_error = |status, message: String| {
            (
                status,
                Json(ErrorResponse {
                    error: format!("File upload failed: {}", message),
                    requires_conversion: false,
                    sample_options: service.get_sample_list(),
                }),
            ).into_response()
        };
        let mut multipart = match Multipart::from_request(req, &service).await {
            Ok(multipart) => multipart,
            Err(e) => return upload_error(e.status(), e.to_string()),
        };
        loop {
            let field = match multipart.next_field().await {
                Ok(Some(field)) => field,
                Ok(None) => break,
                Err(e) => return upload_error(e.status(), e.to_string()),
            };
            if let Some(file_name) = field.file_name().map(|s| s.to_string()) {
                let temp_dir = std::env::temp_dir().join("forms-upload");
                if let Err(e) = std::fs::create_dir_all(&temp_dir) {
                    return upload_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
                }
                let target_file = temp_dir.join(&file_name);

                let bytes = match field.bytes().await {
                    Ok(bytes) => bytes,
                    Err(e) => return upload_error(e.status(), e.to_string()),
                };
                if let Err(e) = std::fs::write(&target_file, &bytes) {
                    return upload_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
                }
                match service.load_module(&target_file) {
                    Ok(index) => return Json(index.to_payload()).into_response(),
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Failed to process form: {}", e),
                                requires_conversion: false,
                                sample_options: service.get_sample_list(),
                            }),
                        )
                            .into_response();
                    }
                }
            }
        }
    } else if let Ok(body_bytes) = axum::body::to_bytes(req.into_body(), 50 * 1024 * 1024).await {
        if let Ok(req) = serde_json::from_slice::<OpenFileRequest>(&body_bytes) {
            if let Some(path_str) = req.file_path {
                let mut path = PathBuf::from(&path_str);
                if !path.is_file() {
                    let parent_rel = Path::new("..").join(&path_str);
                    if parent_rel.is_file() {
                        path = parent_rel;
                    }
                }
                if !path.is_file() {
                    return (
                        StatusCode::NOT_FOUND,
                        Json(ErrorResponse {
                            error: format!("File not found at '{}'", path_str),
                            requires_conversion: false,
                            sample_options: service.get_sample_list(),
                        }),
                    )
                        .into_response();
                }

                match service.load_module(&path) {
                    Ok(index) => return Json(index.to_payload()).into_response(),
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse {
                                error: format!("Failed to process form: {}", e),
                                requires_conversion: false,
                                sample_options: service.get_sample_list(),
                            }),
                        )
                            .into_response();
                    }
                }
            }
        }
    }

    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: "No file uploaded or filePath provided in request.".to_string(),
            requires_conversion: false,
            sample_options: service.get_sample_list(),
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use tower::ServiceExt;

    async fn upload(body: String) -> Response {
        static NEXT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let cache = std::env::temp_dir().join(format!("forms-upload-test-{}-{}", std::process::id(), id));
        let response = app_router(FormsService::new(&cache))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/open-file")
                    .header("content-type", "multipart/form-data; boundary=test-boundary")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        std::fs::remove_dir_all(cache).unwrap();
        response
    }

    #[tokio::test]
    async fn upload_over_two_megabytes_is_parsed() {
        let body = format!(
            "--test-boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"large-upload-regression.xml\"\r\nContent-Type: application/xml\r\n\r\n<Module><FormModule Name=\"LARGE\"/><!--{}--></Module>\r\n--test-boundary--\r\n",
            "x".repeat(3 * 1024 * 1024)
        );
        let response = upload(body).await;
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        std::fs::remove_file(std::env::temp_dir().join("forms-upload/large-upload-regression.xml")).ok();
        assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    }

    #[tokio::test]
    async fn upload_over_limit_returns_payload_too_large() {
        let body = format!(
            "--test-boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"oversized-upload-regression.xml\"\r\n\r\n{}\r\n--test-boundary--\r\n",
            "x".repeat(50 * 1024 * 1024)
        );
        let response = upload(body).await;
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let error: ErrorResponse = serde_json::from_slice(&body).unwrap();
        assert!(error.error.contains("upload"), "{}", error.error);
    }
}
