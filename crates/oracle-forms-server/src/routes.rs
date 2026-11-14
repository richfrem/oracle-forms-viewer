use std::path::{Path, PathBuf};
use axum::extract::{DefaultBodyLimit, FromRequest, Multipart, Path as AxumPath, Query, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use oracle_forms_core::model::*;
use crate::assets::static_handler;
use crate::security::{self, SecurityConfig};
use crate::service::FormsService;

/// Router with the default (loopback-only) security policy.
#[cfg_attr(not(test), allow(dead_code))]
pub fn app_router(service: FormsService) -> Router {
    app_router_with_security(service, SecurityConfig::loopback_only())
}

/// Router with an explicit security policy. Every request passes through
/// [`security::guard`] (Host allow-list, same-origin API requests).
pub fn app_router_with_security(service: FormsService, security_cfg: SecurityConfig) -> Router {
    Router::new()
        .route("/api/samples", get(get_samples))
        .route("/api/files/browse", get(browse_files))
        .route("/api/open-sample/{name}", post(open_sample))
        .route("/api/open-file", post(open_file).layer(DefaultBodyLimit::max(50 * 1024 * 1024)))
        .route("/api/module/{module_name}/source", get(get_source))
        .route("/api/module/{module_name}/xml", get(get_xml))
        .route("/api/module/{module_name}/effective-item", get(get_effective_item))
        .route("/api/db/test", post(test_db_connection))
        .route("/api/db/describe", post(describe_table))
        .route("/api/forms/query-block", post(query_form_block))
        .route("/api/forms/export-xml", post(export_form_xml))
        .route("/api/artifacts/convert-to-xml", post(convert_artifact_to_xml))
        .route("/api/forms/save", post(save_form_file))
        .route("/api/forms/oracle-preview", post(start_oracle_preview))
        .route("/api/forms/oracle-preview/{id}", get(oracle_preview_status))
        .route("/api/forms/oracle-preview/readiness", get(oracle_preview_readiness))
        .route("/api/forms/diff", post(diff_forms))
        .route("/api/forms/new", post(create_form))
        .route("/api/forms/module/{module_name}/item", post(update_form_item))
        .route("/api/forms/module/{module_name}/sync", post(sync_form_module))
        .route("/api/forms/module/{module_name}/attach-menu", post(attach_form_menu))
        .route("/api/reports", get(get_reports))
        .route("/api/reports/run", post(run_report_endpoint))
        .route("/api/reports/{name}/definition", get(get_report_definition))
        .route("/api/reports/{name}/html", get(get_report_html))
        .route("/api/sessions", post(start_session))
        .route("/api/sessions/{id}", get(session_state).delete(end_session))
        .route("/api/sessions/{id}/events", post(session_event))
        .route("/api/sessions/{id}/alert", post(session_alert))
        .fallback(static_handler)
        .with_state(service)
        .layer(axum::middleware::from_fn_with_state(
            std::sync::Arc::new(security_cfg),
            security::guard,
        ))
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
    // No line range means the whole file (the XML Source tab shows the full XML by default).
    let start_line = query.start_line.unwrap_or(1);
    let end_line = query.end_line.unwrap_or(usize::MAX);

    match service.read_xml_slice(&module_name, start_line, end_line) {
        Some(xml) => {
            let end_line = if query.end_line.is_none() {
                start_line.saturating_add(xml.lines().count().saturating_sub(1))
            } else {
                end_line
            };
            Json(XmlSliceResponse {
                module_name,
                start_line,
                end_line,
                xml,
            })
            .into_response()
        }
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DescribeTableRequest {
    table_name: String,
    #[serde(flatten)]
    connection: TestDbRequest,
}

async fn describe_table(State(service): State<FormsService>, Json(req): Json<DescribeTableRequest>) -> Response {
    use oracle_forms_db::{BoundSql, DbBackend, DbConfig, SqlValue};
    let table = req.table_name.trim().to_ascii_uppercase();
    if table.is_empty() || !table.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') {
        return session_error(400, "Enter a plain table or view name in your connected schema".into());
    }
    let c = req.connection;
    let config = service.apply_credential_policy(DbConfig {
        backend: c.backend, host: c.host, port: c.port, service_name: c.service_name,
        username: c.username, password: c.password, ..Default::default()
    });
    let (sql, placeholder) = match c.backend {
        DbBackend::Oracle | DbBackend::OracleThin => (
            "SELECT COLUMN_NAME, DATA_TYPE FROM USER_TAB_COLUMNS WHERE TABLE_NAME = :b1 ORDER BY COLUMN_ID", ":b1"),
        DbBackend::Sqlite | DbBackend::Mock => (
            "SELECT name AS COLUMN_NAME, type AS DATA_TYPE FROM pragma_table_info(?1) ORDER BY cid", "?1"),
        _ => return session_error(400, "Schema discovery supports Oracle and SQLite".into()),
    };
    let driver = match oracle_forms_db::create_driver(&config) {
        Ok(driver) => driver, Err(error) => return session_error(400, error.to_string()),
    };
    match driver.query(&BoundSql::with_binds(sql, vec![(placeholder.into(), SqlValue::Text(table))])).await {
        Ok(result) => Json(result).into_response(),
        Err(error) => session_error(400, error.to_string()),
    }
}

async fn test_db_connection(
    State(service): State<FormsService>,
    Json(payload): Json<TestDbRequest>,
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
    let config = service.apply_credential_policy(config);

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
#[serde(rename_all = "camelCase")]
struct StartSessionRequest {
    module_name: String,
    /// Omit (or "none") to run without a database.
    backend: Option<String>,
    host: Option<String>,
    port: Option<u16>,
    service_name: Option<String>,
    username: Option<String>,
    password: Option<String>,
    /// Off by default: the session refuses DML, locks and commits.
    #[serde(default)]
    allow_writes: bool,
}

fn session_error(code: u16, message: String) -> Response {
    let status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

/// Starts a live form session: loads the module and its attached libraries, connects
/// to the database (if requested) and runs the startup triggers.
async fn start_session(State(service): State<FormsService>, Json(req): Json<StartSessionRequest>) -> Response {
    let module = match service.get_or_load_module(&req.module_name) {
        Ok(m) => m,
        Err(e) => return session_error(404, e.to_string()),
    };

    let mut libraries = Vec::new();
    let mut missing = Vec::new();
    for lib in &module.attached_libraries {
        match service.resolve_library(&req.module_name, &lib.name) {
            Some(idx) => libraries.push(idx),
            None => missing.push(lib.name.clone()),
        }
    }

    let db = match req.backend.as_deref().map(str::trim).filter(|b| !b.is_empty() && !b.eq_ignore_ascii_case("none")) {
        None => None,
        Some(name) => {
            let backend = match oracle_forms_db::DbBackend::parse(name) {
                Ok(b) => b,
                Err(e) => return session_error(400, e.to_string()),
            };
            let mut cfg = oracle_forms_db::DbConfig { backend, ..Default::default() };
            if let Some(h) = req.host { cfg.host = Some(h); }
            if let Some(p) = req.port { cfg.port = Some(p); }
            if let Some(s) = req.service_name { cfg.service_name = Some(s); }
            if let Some(u) = req.username.filter(|u| !u.is_empty()) { cfg.username = Some(u); }
            if let Some(pw) = req.password.filter(|p| !p.is_empty()) { cfg.password = Some(pw); }
            Some(service.apply_credential_policy(cfg))
        }
    };

    let opts = crate::runtime_sessions::StartOptions {
        module,
        libraries,
        missing_libraries: missing,
        db,
        allow_writes: req.allow_writes,
    };
    match service.sessions.start(opts).await {
        Ok(state) => Json(state).into_response(),
        Err(e) => session_error(400, e),
    }
}

async fn session_event(
    State(service): State<FormsService>,
    AxumPath(id): AxumPath<String>,
    Json(event): Json<crate::runtime_sessions::SessionEvent>,
) -> Response {
    match service.sessions.send_event(&id, event).await {
        Ok(state) => Json(state).into_response(),
        Err((code, msg)) => session_error(code, msg),
    }
}

#[derive(Deserialize)]
struct AlertAnswer {
    /// 1, 2 or 3.
    button: u8,
}

async fn session_alert(State(service): State<FormsService>, AxumPath(id): AxumPath<String>, Json(answer): Json<AlertAnswer>) -> Response {
    let code = match answer.button {
        1 => oracle_forms_runtime::ALERT_BUTTON1,
        2 => oracle_forms_runtime::ALERT_BUTTON2,
        3 => oracle_forms_runtime::ALERT_BUTTON3,
        _ => return session_error(400, "button must be 1, 2 or 3".to_string()),
    };
    match service.sessions.answer_alert(&id, code).await {
        Ok(state) => Json(state).into_response(),
        Err((c, msg)) => session_error(c, msg),
    }
}

async fn session_state(State(service): State<FormsService>, AxumPath(id): AxumPath<String>) -> Response {
    match service.sessions.state(&id).await {
        Ok(state) => Json(state).into_response(),
        Err((code, msg)) => session_error(code, msg),
    }
}

async fn end_session(State(service): State<FormsService>, AxumPath(id): AxumPath<String>) -> Response {
    if service.sessions.remove(&id) {
        StatusCode::NO_CONTENT.into_response()
    } else {
        session_error(404, "Session not found".to_string())
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
    /// `patchedOriginal` (lossless) or `summary` (regenerated, lossy).
    mode: crate::service::ExportMode,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveFormRequest { module_name: String, username: Option<String>, password: Option<String>, #[serde(default)] offline: bool }

async fn save_form_file(State(service): State<FormsService>, Json(req): Json<SaveFormRequest>) -> Response {
    match service.save_form_file(&req.module_name) {
        Ok((filename, mode)) => Json(serde_json::json!({
            "fileName": filename, "path": format!("samples/{}", filename), "mode": mode
        })).into_response(),
        Err(error) => session_error(400, error.to_string()),
    }
}

async fn start_oracle_preview(State(service): State<FormsService>, Json(req): Json<SaveFormRequest>) -> Response {
    let credentials = if req.offline { None } else {
        match req.username.zip(req.password).filter(|(u, p)| !u.trim().is_empty() && !p.is_empty()) {
            Some(credentials) => Some(credentials),
            None => return session_error(400, "Database-connected compilation needs a username and password. Enter compile credentials in Preview, or explicitly select offline compilation.".into()),
        }
    };
    match service.save_form_file(&req.module_name) {
        Ok((filename, mode)) => match crate::oracle_preview::start(&filename, credentials) {
            Ok(id) => Json(serde_json::json!({ "jobId": id, "fileName": filename, "mode": mode })).into_response(),
            Err(error) => session_error(400, format!("Saved samples/{}. Oracle preview failed: {}", filename, error)),
        },
        Err(error) => session_error(400, error.to_string()),
    }
}

async fn oracle_preview_readiness() -> impl IntoResponse {
    Json(crate::oracle_preview::readiness().await)
}

async fn oracle_preview_status(AxumPath(id): AxumPath<String>) -> Response {
    match crate::oracle_preview::status(&id) {
        Some(status) => Json(status).into_response(),
        None => session_error(404, "Preview job not found".into()),
    }
}

async fn export_form_xml(
    State(service): State<FormsService>,
    Json(payload): Json<ExportXmlRequest>,
) -> Response {
    if let Some(name) = payload.module_name {
        match service.export_xml(&name) {
            Ok((xml, mode)) => Json(ExportXmlResponse {
                module_name: name,
                xml,
                mode,
            })
            .into_response(),
            Err(e) => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response(),
        }
    } else if let Some(module) = payload.module {
        // A client-supplied model can only be exported as a (lossy) summary.
        Json(ExportXmlResponse {
            module_name: module.key.name.clone(),
            xml: FormsService::summary_xml(&module),
            mode: crate::service::ExportMode::Summary,
        })
        .into_response()
    } else {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Either 'moduleName' or 'module' must be provided" })),
        )
            .into_response()
    }
}

#[derive(Deserialize)]
struct ConvertArtifactRequest {
    #[serde(rename = "moduleName")]
    module_name: Option<String>,
    #[serde(rename = "fileName")]
    file_name: Option<String>,
}

async fn convert_artifact_to_xml(
    State(service): State<FormsService>,
    Json(payload): Json<ConvertArtifactRequest>,
) -> Response {
    let source_path = if let Some(ref fname) = payload.file_name {
        if !crate::security::is_plain_file_name(fname) {
            return session_error(400, format!("Invalid file name '{}'", fname));
        }
        match crate::security::safe_join(&FormsService::samples_dir(), fname) {
            Some(p) if p.is_file() => p,
            _ => return session_error(404, format!("File '{}' not found in samples", fname)),
        }
    } else if let Some(ref mname) = payload.module_name {
        match service.get_source_file(mname) {
            Some(p) if p.is_file() => p,
            _ => return session_error(404, format!("Source binary for '{}' not found", mname)),
        }
    } else {
        return session_error(400, "Either 'moduleName' or 'fileName' must be provided".into());
    };

    match crate::oracle_preview::convert_to_xml(&source_path).await {
        Ok((output_path, log)) => {
            let output_file = output_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            let loaded_payload = service.load_module(&output_path).ok();
            Json(serde_json::json!({
                "ok": true,
                "outputFile": output_file,
                "outputPath": output_path.to_string_lossy(),
                "payload": loaded_payload,
                "log": log,
            }))
            .into_response()
        }
        Err(err) => session_error(500, err),
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
    #[serde(default, rename = "includeModels")]
    include_models: bool,
}

async fn diff_forms(
    State(service): State<FormsService>,
    Json(payload): Json<DiffRequest>,
) -> Response {
    let resolve = |model: Option<ModuleIndex>, name: Option<String>| {
        model.map(Ok).unwrap_or_else(|| match name {
            Some(name) => service.get_or_load_module(&name),
            None => Err(oracle_forms_core::error::FormsError::InvalidFormat(
                "Each side needs a model or module name".into()
            )),
        })
    };
    let (base, modified) = match (
        resolve(payload.base, payload.base_module),
        resolve(payload.modified, payload.modified_module),
    ) {
        (Ok(base), Ok(modified)) => (base, modified),
        (Err(error), _) | (_, Err(error)) => return (
            StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": error.to_string() }))
        ).into_response(),
    };
    let diff = oracle_forms_core::diff::FormDiffer::diff(&base, &modified);
    if payload.include_models {
        Json(serde_json::json!({ "diff": diff, "base": base, "modified": modified })).into_response()
    } else {
        Json(diff).into_response()
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateItemRequest {
    pub block_name: String,
    pub item_name: String,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub prompt: Option<String>,
    pub label: Option<String>,
    pub item_type: Option<String>,
    pub data_type: Option<String>,
    pub column_name: Option<String>,
    pub back_color: Option<String>,
    pub fore_color: Option<String>,
    pub enabled: Option<bool>,
    pub visible: Option<bool>,
    pub required: Option<bool>,
    pub prompt_attachment_edge: Option<String>,
    pub prompt_attachment_offset: Option<i32>,
    pub initial_value: Option<String>,
    pub maximum_length: Option<i32>,
}

async fn update_form_item(
    AxumPath(module_name): AxumPath<String>,
    State(service): State<FormsService>,
    Json(payload): Json<UpdateItemRequest>,
) -> Response {
    match service.update_item_ext(&module_name, payload) {
        Ok(updated_mod) => Json(updated_mod.to_payload()).into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn browse_files(State(service): State<FormsService>) -> impl IntoResponse {
    Json(service.browse_workspace_files())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AttachMenuRequest {
    menu_file: String,
}

async fn attach_form_menu(
    AxumPath(module_name): AxumPath<String>,
    State(service): State<FormsService>,
    Json(req): Json<AttachMenuRequest>,
) -> Response {
    match service.attach_menu(&module_name, &req.menu_file) {
        Ok(index) => Json(index.to_payload()).into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn create_form(State(service): State<FormsService>, Json(module): Json<ModuleIndex>) -> Response {
    let name = &module.key.name;
    if name.is_empty() || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') {
        return session_error(400, "Use letters, digits and underscores for a form name".into());
    }
    if service.get_or_load_module(name).is_ok() {
        return session_error(409, "A form with this name already exists".into());
    }
    let payload = module.clone().to_payload();
    if !service.create_module(module) {
        return session_error(409, "A form with this name already exists".into());
    }
    (StatusCode::CREATED, Json(payload)).into_response()
}

async fn sync_form_module(
    AxumPath(_module_name): AxumPath<String>,
    State(service): State<FormsService>,
    Json(module): Json<ModuleIndex>,
) -> Response {
    let payload = module.clone().to_payload();
    service.save_module(module);
    Json(payload).into_response()
}

fn try_open_as_report(file_path: &Path, file_name: &str) -> Option<Response> {
    let lower = file_name.to_ascii_lowercase();
    if lower.ends_with("_report.xml") || lower.ends_with("_rdf.xml") || lower.ends_with(".rdf") || lower.ends_with(".rpt") {
        if let Ok(report) = oracle_forms_xml::ReportsXmlParser::parse_file(file_path) {
            // Ensure report is accessible in samples_dir if uploaded to temp
            let samples_target = FormsService::samples_dir().join(file_name);
            if file_path != samples_target && !samples_target.exists() {
                let _ = std::fs::copy(file_path, &samples_target);
            }
            return Some(
                Json(serde_json::json!({
                    "isReport": true,
                    "reportName": file_name,
                    "report": report
                }))
                .into_response(),
            );
        }
    }
    None
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
            if let Some(raw_name) = field.file_name().map(|s| s.to_string()) {
                // The client controls this name. Keep only the final path component so an
                // upload named "../../x" cannot be written outside the upload directory.
                let file_name = match Path::new(&raw_name.replace('\\', "/"))
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_string)
                {
                    Some(n) if security::is_plain_file_name(&n) && !n.starts_with('.') => n,
                    _ => return upload_error(StatusCode::BAD_REQUEST, format!("Invalid upload file name '{}'", raw_name)),
                };
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
                if let Some(rep_resp) = try_open_as_report(&target_file, &file_name) {
                    return rep_resp;
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

                let fname = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if let Some(rep_resp) = try_open_as_report(&path, fname) {
                    return rep_resp;
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

/// Inspect report source and parameters without connecting to a database.
async fn get_report_definition(
    AxumPath(name): AxumPath<String>,
    State(service): State<FormsService>,
) -> Response {
    match service.load_report(&name) {
        Ok(report) => Json(report).into_response(),
        Err(error) => (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "error": error.to_string()
        }))).into_response(),
    }
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
        // Opened from Forms2XML: the export is the original file, byte for byte.
        assert_eq!(resp["mode"], "patchedOriginal");
        let original = std::fs::read_to_string("../../samples/orders_fmb.xml").unwrap();
        assert_eq!(xml_str, original);
        let _ = std::fs::remove_dir_all(cache);
    }

    #[tokio::test]
    async fn export_after_layout_edit_changes_only_that_item() {
        let (service, cache) = security_test_service("export-edit");
        let original = std::fs::read_to_string("../../samples/test_db_fmb.xml").unwrap();
        service.load_sample("test_db_fmb.xml").unwrap();
        let (status, body, _) = send(
            service.clone(),
            Request::builder()
                .method("POST")
                .uri("/api/forms/module/TEST_DB/item")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"blockName": "BLOCK5", "itemName": "TEXT_ITEM4", "x": 33, "y": 77}"#))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let (status, body, _) = send(
            service,
            Request::builder()
                .method("POST")
                .uri("/api/forms/export-xml")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"moduleName": "TEST_DB"}"#))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let resp: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resp["mode"], "patchedOriginal");
        let exported = resp["xml"].as_str().unwrap();
        // Exactly the two coordinates changed; every other byte is the original.
        let expected = original
            .replacen(r#"Name="TEXT_ITEM4" Justification="Start" DistanceBetweenRecords="4" Width="240" ItemType="Text Item" BackColor="white" XPosition="20" FontName="Segoe UI" YPosition="50""#,
                      r#"Name="TEXT_ITEM4" Justification="Start" DistanceBetweenRecords="4" Width="240" ItemType="Text Item" BackColor="white" XPosition="33" FontName="Segoe UI" YPosition="77""#, 1);
        assert_ne!(expected, original, "test fixture changed");
        assert_eq!(exported, expected);
        let _ = std::fs::remove_dir_all(cache);
    }

    #[tokio::test]
    async fn preview_requires_compile_credentials_without_explicit_offline_mode() {
        let (service, cache) = security_test_service("preview-credentials");
        let (status, body, _) = send(service, Request::builder().method("POST")
            .uri("/api/forms/oracle-preview").header("content-type", "application/json")
            .body(Body::from(r#"{"moduleName":"DO_NOT_SAVE_WITHOUT_CREDENTIALS"}"#)).unwrap()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body.contains("Database-connected compilation needs"), "{body}");
        let _ = std::fs::remove_dir_all(cache);
    }

    #[tokio::test]
    async fn live_schema_discovery_returns_columns_and_rejects_sql_text() {
        let (service, cache) = security_test_service("schema-discovery");
        for (table, expected) in [("test_items", StatusCode::OK), ("test_items; DROP TABLE orders", StatusCode::BAD_REQUEST)] {
            let payload = serde_json::json!({ "tableName": table, "backend": "sqlite" });
            let (status, body, _) = send(service.clone(), Request::builder().method("POST")
                .uri("/api/db/describe").header("content-type", "application/json")
                .body(Body::from(payload.to_string())).unwrap()).await;
            assert_eq!(status, expected, "{body}");
            if status == StatusCode::OK { assert!(body.contains("item_name"), "{body}"); }
        }
        let _ = std::fs::remove_dir_all(cache);
    }

    #[tokio::test]
    async fn diff_detects_added_canvases_program_units_and_unfocused_properties() {
        let (service, cache) = security_test_service("diff-coverage");
        let base = service.load_sample("orders_fmb.xml").unwrap();
        let mut changed = base.clone();
        changed.canvases.push(base.canvases[0].clone());
        let diff = oracle_forms_core::diff::FormDiffer::diff(&base, &changed);
        assert!(diff.has_differences());
        assert!(diff.model_changes.iter().any(|c| c.property == "canvases"));
        let mut changed = base.clone();
        changed.blocks[0].items[0].back_color = Some("red".into());
        let diff = oracle_forms_core::diff::FormDiffer::diff(&base, &changed);
        assert!(diff.has_differences());
        assert!(diff.model_changes.iter().any(|c| c.property == "blocks"));
        let mut changed = base.clone();
        let unit: ProgramUnitInfo = serde_json::from_value(serde_json::json!({
            "name": "NEW_PROCEDURE", "sourceText": "BEGIN NULL; END;"
        })).unwrap();
        changed.program_units.push(unit);
        let diff = oracle_forms_core::diff::FormDiffer::diff(&base, &changed);
        assert!(diff.model_changes.iter().any(|c| c.property == "programUnits"));
        let mut changed = base.clone();
        changed.parsed_at = "different ingestion time".into();
        changed.source_file = "another/location.xml".into();
        assert!(!oracle_forms_core::diff::FormDiffer::diff(&base, &changed).has_differences());
        let _ = std::fs::remove_dir_all(cache);
    }

    #[tokio::test]
    async fn diff_accepts_edited_model_against_named_sample_with_visual_models() {
        let (service, cache) = security_test_service("mixed-diff");
        let mut base = service.load_sample("orders_fmb.xml").unwrap();
        base.blocks[0].items[0].prompt = Some("Edited in browser".into());
        let body = serde_json::json!({ "base": base, "modifiedModule": "orders_fmb.xml", "includeModels": true });
        let (status, body, _) = send(service, Request::builder().method("POST")
            .uri("/api/forms/diff").header("content-type", "application/json")
            .body(Body::from(body.to_string())).unwrap()).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let result: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(!result["diff"]["blocksModified"].as_array().unwrap().is_empty());
        assert_eq!(result["base"]["blocks"][0]["items"][0]["prompt"], "Edited in browser");
        assert!(result["modified"]["canvases"].is_array());
        let _ = std::fs::remove_dir_all(cache);
    }

    #[tokio::test]
    async fn new_form_is_saved_and_duplicate_creation_is_rejected() {
        let (service, cache) = security_test_service("new-form");
        let mut module = service.load_sample("orders_fmb.xml").unwrap();
        module.key.name = "NEW_WIZARD_FORM".into();
        let request = || Request::builder().method("POST").uri("/api/forms/new")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&module).unwrap())).unwrap();
        let (status, body, _) = send(service.clone(), request()).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert!(service.get_module("new_wizard_form").is_some());
        let (status, _, _) = send(service, request()).await;
        assert_eq!(status, StatusCode::CONFLICT);
        let _ = std::fs::remove_dir_all(cache);
    }

    #[tokio::test]
    async fn structural_edits_are_exported_instead_of_silently_discarded() {
        let (service, cache) = security_test_service("structural-export");
        let mut module = service.load_sample("orders_fmb.xml").unwrap();
        let mut block = module.blocks[0].clone();
        block.name = "NEW_BLOCK".into();
        module.blocks.push(block);
        service.save_module(module.clone());
        let (xml, mode) = service.export_xml(&module.key.name).unwrap();
        assert_eq!(mode, crate::service::ExportMode::Summary);
        assert!(xml.contains("NEW_BLOCK"));
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
    async fn report_definition_is_available_without_database_credentials() {
        let cache = std::env::temp_dir().join(format!("forms-report-definition-{}", std::process::id()));
        let app = app_router(FormsService::new(&cache));
        let response = app.oneshot(Request::builder()
            .uri("/api/reports/orders_summary_report.xml/definition")
            .body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let report: oracle_forms_core::report::ReportDefinition = serde_json::from_slice(&body).unwrap();
        assert_eq!(report.name, "ORDERS_SUMMARY");
        assert!(!report.data.data_sources[0].select_sql.is_empty());
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

    // -----------------------------------------------------------------------
    // Security regression tests (cross-origin, DNS rebinding, path traversal).
    // -----------------------------------------------------------------------

    fn security_test_service(tag: &str) -> (FormsService, std::path::PathBuf) {
        let cache = std::env::temp_dir().join(format!("forms-sec-{}-{}", tag, std::process::id()));
        (FormsService::new(&cache), cache)
    }

    async fn send(service: FormsService, req: Request<Body>) -> (StatusCode, String, axum::http::HeaderMap) {
        let response = app_router(service).oneshot(req).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8_lossy(&body).to_string(), headers)
    }

    #[tokio::test]
    async fn source_endpoint_rejects_path_traversal() {
        let (service, cache) = security_test_service("traversal");
        service.load_sample("orders_fmb.xml").expect("sample loads");

        for path in [
            "../../../../../../../etc/hosts",
            "plsql/../../../../../../../etc/hosts",
            "..%2F..%2F..%2Fetc%2Fhosts",
            "/etc/hosts",
            "\\etc\\hosts",
        ] {
            let uri = format!("/api/module/ORDERS/source?path={}", path.replace('/', "%2F"));
            let (status, body, _) = send(
                service.clone(),
                Request::builder().uri(uri).body(Body::empty()).unwrap(),
            )
            .await;
            assert_eq!(status, StatusCode::NOT_FOUND, "path {path:?} returned {status}: {body}");
            assert!(!body.contains("localhost"), "path {path:?} leaked file contents");
        }
        let _ = std::fs::remove_dir_all(cache);
    }

    #[tokio::test]
    async fn open_sample_only_accepts_plain_sample_names() {
        let (service, cache) = security_test_service("sample-names");
        for name in ["..%2F..%2FCargo.toml", "%2Fetc%2Fhosts", "..%2Fsamples%2Forders_fmb.xml"] {
            let (status, _, _) = send(
                service.clone(),
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/open-sample/{}", name))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
            assert_eq!(status, StatusCode::NOT_FOUND, "sample name {name:?} was accepted");
        }
        let (status, _, _) = send(
            service.clone(),
            Request::builder()
                .method("POST")
                .uri("/api/open-sample/orders_fmb.xml")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "a real sample must still open");
        let _ = std::fs::remove_dir_all(cache);
    }

    #[tokio::test]
    async fn cross_origin_api_requests_are_rejected() {
        let (service, cache) = security_test_service("cors");
        let cases: Vec<(&str, &str)> = vec![
            ("origin", "https://evil.example"),
            ("origin", "null"),
            ("origin", "http://127.0.0.1:9999"), // another local app on a different port
            ("sec-fetch-site", "cross-site"),
        ];
        for (header_name, value) in cases {
            let (status, _, headers) = send(
                service.clone(),
                Request::builder()
                    .method("POST")
                    .uri("/api/open-sample/orders_fmb.xml")
                    .header("host", "127.0.0.1:7070")
                    .header(header_name, value)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{header_name}: {value} was allowed");
            assert!(headers.get("access-control-allow-origin").is_none());
        }
        let _ = std::fs::remove_dir_all(cache);
    }

    #[tokio::test]
    async fn same_origin_api_requests_are_allowed() {
        let (service, cache) = security_test_service("same-origin");
        let (status, body, headers) = send(
            service,
            Request::builder()
                .method("POST")
                .uri("/api/open-sample/orders_fmb.xml")
                .header("host", "127.0.0.1:7070")
                .header("origin", "http://127.0.0.1:7070")
                .header("sec-fetch-site", "same-origin")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(headers.get("access-control-allow-origin").is_none(), "no CORS headers expected");
        let _ = std::fs::remove_dir_all(cache);
    }

    #[tokio::test]
    async fn dns_rebinding_host_is_rejected() {
        let (service, cache) = security_test_service("rebinding");
        for host in ["evil.example", "evil.example:7070", "192.168.1.20:7070"] {
            let (status, _, _) = send(
                service.clone(),
                Request::builder()
                    .uri("/api/samples")
                    .header("host", host)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
            assert_eq!(status, StatusCode::MISDIRECTED_REQUEST, "Host {host} was accepted");
        }
        for host in ["localhost:7070", "127.0.0.1:7070", "[::1]:7070"] {
            let (status, _, _) = send(
                service.clone(),
                Request::builder()
                    .uri("/api/samples")
                    .header("host", host)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "loopback Host {host} was rejected");
        }
        let _ = std::fs::remove_dir_all(cache);
    }

    #[tokio::test]
    async fn upload_file_name_cannot_escape_upload_dir() {
        let marker = format!("ofw-escape-{}.xml", std::process::id());
        let escaped = std::env::temp_dir().join(&marker);
        let _ = std::fs::remove_file(&escaped);
        let body = format!(
            "--test-boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"../{}\"\r\nContent-Type: application/xml\r\n\r\n<Module><FormModule Name=\"ESC\"/></Module>\r\n--test-boundary--\r\n",
            marker
        );
        let _ = upload(body).await;
        assert!(!escaped.exists(), "upload was written outside the upload directory");
        let inside = std::env::temp_dir().join("forms-upload").join(&marker);
        let _ = std::fs::remove_file(inside);
    }

    #[tokio::test]
    async fn oracle_query_without_credentials_errors_instead_of_mock_rows() {
        let (service, cache) = security_test_service("no-mock-fallback");
        let (status, body, _) = send(
            service,
            Request::builder()
                .method("POST")
                .uri("/api/forms/query-block")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"moduleName": "orders_fmb.xml", "backend": "oracle-thin"}"#))
                .unwrap(),
        )
        .await;
        assert_ne!(status, StatusCode::OK, "must not return rows: {body}");
        assert!(body.to_ascii_lowercase().contains("username") || body.to_ascii_lowercase().contains("password"), "{body}");
        let _ = std::fs::remove_dir_all(cache);
    }

    #[tokio::test]
    async fn report_names_cannot_escape_samples_dir() {
        let (service, cache) = security_test_service("report-names");
        let (status, _, _) = send(
            service,
            Request::builder()
                .uri("/api/reports/..%2F..%2FCargo.toml/html")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let _ = std::fs::remove_dir_all(cache);
    }

    #[tokio::test]
    async fn live_session_runs_test_db_form_triggers_against_sqlite() {
        let (service, cache) = security_test_service("live-session");
        let start = |body: &'static str| {
            Request::builder()
                .method("POST")
                .uri("/api/sessions")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap()
        };
        let (status, body, _) = send(service.clone(), start(r#"{"moduleName": "test_db_fmb.xml", "backend": "sqlite"}"#)).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let state: serde_json::Value = serde_json::from_str(&body).unwrap();
        // WHEN-NEW-FORM-INSTANCE ran its SELECT ... INTO against test_items (item 1).
        let block5 = state["blocks"].as_array().unwrap().iter().find(|b| b["name"] == "BLOCK5").unwrap();
        assert_eq!(block5["records"][0]["values"]["TEXT_ITEM4"], "Widget A", "{state}");
        assert_eq!(state["readOnly"], true);

        // The "next item" button trigger moves to the next row via its IF/ELSIF + SELECT.
        let id = state["sessionId"].as_str().unwrap();
        let (status, body, _) = send(
            service.clone(),
            Request::builder()
                .method("POST")
                .uri(format!("/api/sessions/{}/events", id))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"kind": "button", "block": "BLOCK5", "item": "PUSH_BUTTON285"}"#))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let state: serde_json::Value = serde_json::from_str(&body).unwrap();
        let block5 = state["blocks"].as_array().unwrap().iter().find(|b| b["name"] == "BLOCK5").unwrap();
        assert_eq!(block5["records"][0]["values"]["TEXT_ITEM4"], "Gadget B", "{state}");

        let (status, _, _) = send(service, Request::builder().method("DELETE").uri(format!("/api/sessions/{}", id)).body(Body::empty()).unwrap()).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let _ = std::fs::remove_dir_all(cache);
    }

    #[tokio::test]
    async fn session_start_with_unknown_backend_is_rejected() {
        let (service, cache) = security_test_service("bad-backend");
        let (status, body, _) = send(
            service,
            Request::builder()
                .method("POST")
                .uri("/api/sessions")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"moduleName": "test_db_fmb.xml", "backend": "orcale"}"#))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        let _ = std::fs::remove_dir_all(cache);
    }
}
