use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, State, multipart::MultipartRejection},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use bytes::Bytes;
use serde::Serialize;
use tower::limit::ConcurrencyLimitLayer;

use crate::{
    config::Config,
    error::ValiError,
    igc,
    utils::{md5_hex, to_utf8},
    validate::{IvshellOutput, run_ivshell},
};

const SERVER_NAME: &str = "Open Validation Server 3.03";

#[derive(Clone)]
pub struct AppState {
    config: Arc<Config>,
    modules: Arc<HashMap<String, (String, String)>>,
}

pub async fn serve(
    config: Config,
    modules: HashMap<String, (String, String)>,
) -> anyhow::Result<()> {
    let bind = config.bind.clone();
    let max_size = config.max_size.max(1024 * 1024);
    let max_concurrency = config.max_concurrency.max(1);

    tracing::info!(
        bind = %bind,
        max_size = config.max_size,
        max_concurrency,
        modules = modules.len(),
        "starting validation server"
    );

    let state = AppState {
        config: Arc::new(config),
        modules: Arc::new(modules),
    };

    let app = Router::new()
        .route("/ready", get(ready))
        .route("/api/vali/json", post(handle_vali))
        .route("/api/vali", post(handle_vali))
        .route("/api.php", post(handle_vali))
        .layer(DefaultBodyLimit::max(max_size.saturating_mul(2)))
        .layer(ConcurrencyLimitLayer::new(max_concurrency))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(address = %listener.local_addr()?, "listening");
    axum::serve(listener, app).await?;

    Ok(())
}

async fn ready() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        "ok\n",
    )
}

async fn handle_vali(
    state: State<AppState>,
    form: Result<Multipart, MultipartRejection>,
) -> ApiResponse {
    let started = Instant::now();
    let response = handle_vali_inner(state, form).await;

    log_validation(&response, started.elapsed());

    response
}

async fn handle_vali_inner(
    State(state): State<AppState>,
    form: Result<Multipart, MultipartRejection>,
) -> ApiResponse {
    let mut form = match form {
        Ok(form) => form,
        Err(error) => {
            tracing::warn!(
                status = %error.status(),
                body = %error.body_text(),
                "failed to parse multipart request"
            );
            return ApiResponse::error_response(ValiError::Request, String::new(), String::new());
        }
    };

    let mut file: Option<(String, Bytes)> = None;
    let mut fields: HashMap<String, String> = HashMap::new();

    loop {
        match form.next_field().await {
            Ok(Some(field)) => {
                let name = field.name().unwrap_or_default().to_string();
                let file_name = field.file_name().map(str::to_string);

                if name == "igcfile" {
                    let file_name = file_name.unwrap_or_default();
                    let igc_name = sanitize_igc_name(&file_name);

                    let bytes = match field.bytes().await {
                        Ok(bytes) => bytes,
                        Err(error) => {
                            tracing::warn!(%error, "failed to read uploaded IGC file");
                            return ApiResponse::error_response(
                                ValiError::Upload,
                                igc_name,
                                String::new(),
                            );
                        }
                    };

                    file = Some((file_name, bytes));
                } else {
                    let value = match field.text().await {
                        Ok(value) => value,
                        Err(error) => {
                            tracing::warn!(%error, field = %name, "failed to read form field");
                            return ApiResponse::error_response(
                                ValiError::Request,
                                String::new(),
                                String::new(),
                            );
                        }
                    };

                    fields.insert(name, value.trim().to_string());
                }
            }
            Ok(None) => break,
            Err(error) => {
                tracing::warn!(%error, "failed to parse multipart request");
                return ApiResponse::error_response(
                    ValiError::Request,
                    String::new(),
                    String::new(),
                );
            }
        }
    }

    let reference = fields.remove("ref").unwrap_or_default();
    let out = fields.remove("out").unwrap_or_default();
    let md5 = fields.remove("md5");

    let Some((file_name, data)) = file else {
        return ApiResponse::error_response(ValiError::Request, String::new(), reference);
    };

    if file_name.is_empty() {
        return ApiResponse::error_response(ValiError::Request, String::new(), reference);
    }

    let igc_name = sanitize_igc_name(&file_name);

    validate(
        &state.config,
        &state.modules,
        igc_name,
        reference,
        data,
        md5,
        &out,
    )
    .await
}

pub async fn validate_file(
    config: Config,
    modules: HashMap<String, (String, String)>,
    path: PathBuf,
) -> anyhow::Result<()> {
    let started = Instant::now();

    let data =
        std::fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let igc_name = sanitize_igc_name(&file_name);

    let response = validate(
        &config,
        &modules,
        igc_name,
        String::new(),
        data.into(),
        None,
        "vali",
    )
    .await;

    log_validation(&response, started.elapsed());
    println!("{}", serde_json::to_string(&response)?);

    Ok(())
}

async fn validate(
    config: &Config,
    modules: &HashMap<String, (String, String)>,
    igc_name: String,
    reference: String,
    data: Bytes,
    md5: Option<String>,
    out: &str,
) -> ApiResponse {
    if data.len() > config.max_size {
        return ApiResponse::error_response(
            ValiError::Size {
                max_size: config.max_size,
            },
            igc_name,
            reference,
        );
    }

    if let Some(md5) = md5
        && !md5.eq_ignore_ascii_case(&md5_hex(data.clone()))
    {
        return ApiResponse::error_response(ValiError::UploadHash, igc_name, reference);
    }

    let data = match igc::unpack(data, config.max_size) {
        Ok(data) => data,
        Err(error) => {
            return ApiResponse::error_response(error, igc_name, reference);
        }
    };

    match run_ivshell(config, modules, data).await {
        Ok(output) => {
            let include_output = match out {
                "vali" => true,
                "ncfail" => output.status == "IGC_FAILED_NC",
                _ => false,
            };

            ApiResponse::from_ivshell(output, igc_name, reference, include_output)
        }
        Err(error) => ApiResponse::error_response(error, igc_name, reference),
    }
}

fn log_validation(response: &ApiResponse, elapsed: Duration) {
    let elapsed_ms = elapsed.as_millis() as u64;

    if response.result == "ERROR" {
        if response.status == "ERR_SERVICE" {
            tracing::error!(
                igc = %response.igc,
                reference = %response.reference,
                status = %response.status,
                msg = %response.msg,
                elapsed_ms,
                "validation failed"
            );
        } else {
            tracing::warn!(
                igc = %response.igc,
                reference = %response.reference,
                status = %response.status,
                msg = %response.msg,
                elapsed_ms,
                "validation failed"
            );
        }
    } else {
        tracing::info!(
            igc = %response.igc,
            reference = %response.reference,
            result = %response.result,
            status = %response.status,
            elapsed_ms,
            "validation complete"
        );
    }
}

fn sanitize_igc_name(name: &str) -> String {
    // PHP strips quotes, converts slashes to backslashes and then takes the basename.
    let cleaned: String = name.chars().filter(|c| *c != '"' && *c != '\'').collect();
    let base = cleaned
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(cleaned.as_str());

    to_utf8(base.as_bytes())
}

#[derive(Debug, Serialize)]
struct ApiResponse {
    result: String,
    status: String,
    msg: String,
    igc: String,
    #[serde(rename = "ref")]
    reference: String,
    server: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<Vec<String>>,
}

impl IntoResponse for ApiResponse {
    fn into_response(self) -> axum::response::Response {
        (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/json; charset=utf-8"),
                (header::CACHE_CONTROL, "no-cache, must-revalidate"),
            ],
            Json(self),
        )
            .into_response()
    }
}

impl ApiResponse {
    fn from_ivshell(
        output: IvshellOutput,
        igc: String,
        reference: String,
        include_output: bool,
    ) -> Self {
        Self {
            result: output.result,
            status: output.status,
            msg: output.message,
            igc,
            reference,
            server: SERVER_NAME,
            output: include_output.then_some(output.lines),
        }
    }

    fn error_response(error: ValiError, igc: String, reference: String) -> Self {
        Self {
            result: "ERROR".to_string(),
            status: error.status().to_string(),
            msg: error.to_string(),
            igc,
            reference,
            server: SERVER_NAME,
            output: None,
        }
    }
}
