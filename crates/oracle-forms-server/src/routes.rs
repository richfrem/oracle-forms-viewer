use std::path::{Path, PathBuf};
use axum::extract::{DefaultBodyLimit, FromRequest, Multipart, Path as AxumPath, Query, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
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
        .route("/api/db/test", post(test_db_connection))
        .route("/api/forms/query-block", post(query_form_block))
        .route("/api/forms/export-xml", post(export_form_xml))
        .route("/api/forms/diff", post(diff_forms))
        .route("/api/forms/module/{module_name}/item", post(update_form_item))
        .route("/api/reports", get(get_reports))
        .route("/api/reports/run", post(run_report_endpoint))
        .route("/api/reports/{name}/html", get(get_report_html))
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryBlockRequest {
    module_name: String,
    block_name: Option<String>,
    #[serde(default)]
    backend: oracle_forms_db::DbBackend,
    host: Option<String>,
    port: Option<u16>,
    service_name: Option<String>,
    username: Option<String>,
    password: Option<String>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TestDbRequest {
    #[serde(default)]
    backend: oracle_forms_db::DbBackend,
    host: Option<String>,
    port: Option<u16>,
    service_name: Option<String>,
    username: Option<String>,
    password: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TestDbResponse {
    success: bool,
    message: String,
    backend: oracle_forms_db::DbBackend,
}

async fn test_db_connection(Json(payload): Json<TestDbRequest>) -> Response {
    let mut config = oracle_forms_db::DbConfig {
        backend: payload.backend,
        ..Default::default()
    };
    if let Some(h) = payload.host { config.host = Some(h); }
    if let Some(p) = payload.port { config.port = Some(p); }
    if let Some(s) = payload.service_name { config.service_name = Some(s); }
    if let Some(u) = payload.username { config.username = Some(u); }
    if let Some(pw) = payload.password { config.password = Some(pw); }

    match oracle_forms_db::create_driver(&config) {
        Ok(driver) => match driver.test_connection().await {
            Ok(_) => Json(TestDbResponse {
                success: true,
                message: format!("Successfully connected to {:?} database listener", payload.backend),
                backend: payload.backend,
            }).into_response(),
            Err(e) => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(TestDbResponse {
                    success: false,
                    message: e.to_string(),
                    backend: payload.backend,
                }),
            ).into_response(),
        },
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(TestDbResponse {
                success: false,
                message: e.to_string(),
                backend: payload.backend,
            }),
        ).into_response(),
    }
}

async fn query_form_block(
    State(service): State<FormsService>,
    Json(payload): Json<QueryBlockRequest>,
) -> Response {
    let mut config = oracle_forms_db::DbConfig {
        backend: payload.backend,
        ..Default::default()
    };
    if let Some(h) = payload.host { config.host = Some(h); }
    if let Some(p) = payload.port { config.port = Some(p); }
    if let Some(s) = payload.service_name { config.service_name = Some(s); }
    if let Some(u) = payload.username { config.username = Some(u); }
    if let Some(pw) = payload.password { config.password = Some(pw); }

    match service.query_block(
        &payload.module_name,
        payload.block_name.as_deref(),
        &config,
        payload.limit,
    ).await {
        Ok(result) => Json(result).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ).into_response(),
    }
}

#[derive(Deserialize)]
struct ExportXmlRequest {
    #[serde(rename = "moduleName")]
    module_name: Option<String>,
    module: Option<ModuleIndex>,
}

#[derive(Serialize)]
struct ExportXmlResponse {
    #[serde(rename = "moduleName")]
    module_name: String,
    xml: String,
}

async fn export_form_xml(
    State(service): State<FormsService>,
    Json(payload): Json<ExportXmlRequest>,
) -> Response {
    if let Some(module) = payload.module {
        let xml = oracle_forms_xml::FormsXmlWriter::write_string(&module);
        return Json(ExportXmlResponse {
            module_name: module.key.name,
            xml,
        })
        .into_response();
    }

    if let Some(name) = payload.module_name {
        match service.export_xml(&name) {
            Ok(xml) => Json(ExportXmlResponse {
                module_name: name,
                xml,
            })
            .into_response(),
            Err(e) => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response(),
        }
    } else {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Either 'moduleName' or 'module' must be provided" })),
        )
            .into_response()
    }
}

#[derive(Deserialize)]
struct DiffRequest {
    #[serde(rename = "baseModule")]
    base_module: Option<String>,
    #[serde(rename = "modifiedModule")]
    modified_module: Option<String>,
    base: Option<ModuleIndex>,
    modified: Option<ModuleIndex>,
}

async fn diff_forms(
    State(service): State<FormsService>,
    Json(payload): Json<DiffRequest>,
) -> Response {
    if let (Some(base), Some(modified)) = (payload.base, payload.modified) {
        let diff = oracle_forms_core::diff::FormDiffer::diff(&base, &modified);
        return Json(diff).into_response();
    }

    if let (Some(b_name), Some(m_name)) = (payload.base_module, payload.modified_module) {
        match service.diff_modules(&b_name, &m_name) {
            Ok(diff) => Json(diff).into_response(),
            Err(e) => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response(),
        }
    } else {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Either 'baseModule'/'modifiedModule' or 'base'/'modified' must be provided" })),
        )
            .into_response()
    }
}

#[derive(Deserialize)]
struct UpdateItemRequest {
    #[serde(rename = "blockName")]
    block_name: String,
    #[serde(rename = "itemName")]
    item_name: String,
    x: Option<i32>,
    y: Option<i32>,
    width: Option<i32>,
    height: Option<i32>,
    prompt: Option<String>,
    label: Option<String>,
}

async fn update_form_item(
    AxumPath(module_name): AxumPath<String>,
    State(service): State<FormsService>,
    Json(payload): Json<UpdateItemRequest>,
) -> Response {
    match service.update_item(
        &module_name,
        &payload.block_name,
        &payload.item_name,
        payload.x,
        payload.y,
        payload.width,
        payload.height,
        payload.prompt,
        payload.label,
    ) {
        Ok(updated_mod) => Json(updated_mod.to_payload()).into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
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

async fn get_reports(State(service): State<FormsService>) -> impl IntoResponse {
    let reports = service.get_report_samples();
    Json(reports)
}

#[derive(Deserialize)]
struct RunReportRequest {
    report_name: String,
    backend: Option<String>,
    parameters: Option<std::collections::HashMap<String, String>>,
}

#[derive(Serialize, Deserialize)]
struct RunReportResponse {
    report: oracle_forms_core::report::ReportDefinition,
    execution: oracle_forms_runtime::ReportExecutionResult,
    html: String,
}

async fn run_report_endpoint(
    State(service): State<FormsService>,
    Json(req): Json<RunReportRequest>,
) -> Response {
    let backend = req.backend.unwrap_or_else(|| "mock".to_string());
    let params = req.parameters.unwrap_or_default();

    match service.run_report(&req.report_name, &backend, &params).await {
        Ok((report, execution, html)) => Json(RunReportResponse {
            report,
            execution,
            html,
        })
        .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn get_report_html(
    AxumPath(name): AxumPath<String>,
    Query(query): Query<std::collections::HashMap<String, String>>,
    State(service): State<FormsService>,
) -> Response {
    let backend = query.get("backend").cloned().unwrap_or_else(|| "mock".to_string());
    match service.run_report(&name, &backend, &query).await {
        Ok((_report, _exec, html)) => (
            StatusCode::OK,
            [("Content-Type", "text/html; charset=utf-8")],
            html,
        )
            .into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            [("Content-Type", "text/html; charset=utf-8")],
            format!("<h1>Error Running Report</h1><p>{}</p>", e),
        )
            .into_response(),
    }
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

    #[tokio::test]
    async fn test_db_test_endpoint() {
        let cache = std::env::temp_dir().join(format!("forms-test-db-{}", std::process::id()));
        let service = FormsService::new(&cache);
        let app = app_router(service);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/db/test")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"backend": "mock"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let resp: TestDbResponse = serde_json::from_slice(&body).unwrap();
        assert!(resp.success);
        let _ = std::fs::remove_dir_all(cache);
    }

    #[tokio::test]
    async fn test_query_block_endpoint() {
        let cache = std::env::temp_dir().join(format!("forms-query-test-{}", std::process::id()));
        let service = FormsService::new(&cache);
        let app = app_router(service);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/forms/query-block")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"moduleName": "orders_fmb.xml", "backend": "mock", "limit": 3}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let resp: oracle_forms_db::QueryResult = serde_json::from_slice(&body).unwrap();
        assert!(!resp.records.is_empty(), "Should return mock queried records");
        let _ = std::fs::remove_dir_all(cache);
    }

    #[tokio::test]
    async fn test_export_xml_endpoint() {
        let cache = std::env::temp_dir().join(format!("forms-export-test-{}", std::process::id()));
        let service = FormsService::new(&cache);
        let app = app_router(service);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/forms/export-xml")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"moduleName": "orders_fmb.xml"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let xml_str = resp.get("xml").and_then(|v| v.as_str()).unwrap();
        assert!(xml_str.contains("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(xml_str.contains("<FormModule Name=\"ORDERS\""));
        let _ = std::fs::remove_dir_all(cache);
    }

    #[tokio::test]
    async fn test_diff_endpoint() {
        let cache = std::env::temp_dir().join(format!("forms-diff-test-{}", std::process::id()));
        let service = FormsService::new(&cache);
        let app = app_router(service);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/forms/diff")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"baseModule": "orders_fmb.xml", "modifiedModule": "orders_fmb.xml"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let resp: oracle_forms_core::diff::ModuleDiff = serde_json::from_slice(&body).unwrap();
        assert!(!resp.has_differences());
        let _ = std::fs::remove_dir_all(cache);
    }

    #[tokio::test]
    async fn test_update_item_endpoint() {
        let cache = std::env::temp_dir().join(format!("forms-update-test-{}", std::process::id()));
        let service = FormsService::new(&cache);
        let _ = service.load_sample("orders_fmb.xml");
        let app = app_router(service);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/forms/module/ORDERS/item")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"blockName": "ORDERS", "itemName": "ORDER_ID", "x": 123, "y": 456, "prompt": "New Order ID"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            resp.get("summary").and_then(|s| s.get("moduleName")).and_then(|v| v.as_str()),
            Some("ORDERS")
        );
        let _ = std::fs::remove_dir_all(cache);
    }

    #[tokio::test]
    async fn test_reports_list_endpoint() {
        let cache = std::env::temp_dir().join(format!("forms-reports-list-test-{}", std::process::id()));
        let service = FormsService::new(&cache);
        let app = app_router(service);

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/reports")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let resp: Vec<crate::service::ReportSampleItem> = serde_json::from_slice(&body).unwrap();
        assert!(resp.iter().any(|r| r.name == "ORDERS_SUMMARY"));
        assert!(resp.iter().any(|r| r.name == "DEPT_SALARIES"));
        let _ = std::fs::remove_dir_all(cache);
    }

    #[tokio::test]
    async fn test_run_report_endpoint() {
        let cache = std::env::temp_dir().join(format!("forms-run-report-test-{}", std::process::id()));
        let service = FormsService::new(&cache);
        let app = app_router(service);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/reports/run")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"report_name": "orders_summary_report.xml", "backend": "mock"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let resp: RunReportResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(resp.report.name, "ORDERS_SUMMARY");
        assert!(resp.html.contains("<!DOCTYPE html>"));
        assert!(resp.html.contains("Customer Orders Summary Report"));
        let _ = std::fs::remove_dir_all(cache);
    }
}
