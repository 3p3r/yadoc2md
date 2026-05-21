use std::sync::Arc;

use clap::Args;
use salvo::affix_state;
use salvo::cors::Cors;
use salvo::http::Method;
use salvo::oapi::extract::FormFile;
use salvo::oapi::{endpoint, ToSchema};
use salvo::prelude::*;
use salvo::size_limiter::max_size;
use serde::{Deserialize, Serialize};

use crate::config::{parse_byte_size, ConvertConfig};
use crate::convert::{convert, AppError};

pub const OPENAPI_JSON_PATH: &str = "/api-doc/openapi.json";
pub const SWAGGER_UI_PATH: &str = "/swagger-ui";

#[derive(Debug, Clone, Args)]
pub struct ServeArgs {
    #[command(flatten)]
    pub config: ConvertConfig,

    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    #[arg(long, default_value_t = 9876)]
    pub port: u16,

    /// Allowed CORS origins (repeatable). Defaults to * when omitted.
    #[arg(long = "cors")]
    pub cors: Vec<String>,

    /// Maximum HTTP request body size (defaults to --max-input-size when omitted).
    #[arg(long = "max-body-size", value_parser = parse_byte_size)]
    pub max_body_size: Option<usize>,
}

#[derive(Clone)]
pub struct AppState {
    pub config: ConvertConfig,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ErrorResponse {
    pub error: String,
}

/// HTTP-level outcome for upload conversion (mapped to status codes in handlers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadError {
    BadRequest(String),
    PayloadTooLarge(String),
    UnsupportedMediaType(String),
    UnprocessableEntity(String),
}

impl UploadError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::PayloadTooLarge(_) => StatusCode::PAYLOAD_TOO_LARGE,
            Self::UnsupportedMediaType(_) => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Self::UnprocessableEntity(_) => StatusCode::UNPROCESSABLE_ENTITY,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::BadRequest(m)
            | Self::PayloadTooLarge(m)
            | Self::UnsupportedMediaType(m)
            | Self::UnprocessableEntity(m) => m,
        }
    }
}

pub fn normalize_cors_origins(cors: &[String]) -> Vec<String> {
    if cors.is_empty() {
        vec!["*".into()]
    } else {
        cors.to_vec()
    }
}

pub fn convert_upload(
    filename: &str,
    data: &[u8],
    cfg: &ConvertConfig,
) -> Result<String, UploadError> {
    if data.len() > cfg.max_input_bytes {
        return Err(UploadError::PayloadTooLarge(format!(
            "upload exceeds --max-input-size ({} bytes)",
            cfg.max_input_bytes
        )));
    }

    convert(filename, data, cfg).map_err(|e| match e {
        AppError::InvalidInput(msg) => UploadError::UnsupportedMediaType(msg),
        AppError::InputTooLarge { max, got } => UploadError::PayloadTooLarge(format!(
            "input exceeds --max-input-size ({max} bytes, got {got})"
        )),
        other => UploadError::UnprocessableEntity(other.to_string()),
    })
}

pub fn build_router(state: Arc<AppState>, max_body: u64) -> Router {
    let api_router = Router::with_path("api")
        .hoop(affix_state::inject(state))
        .hoop(max_size(max_body))
        .push(Router::with_path("health").get(health))
        .push(Router::with_path("parse").post(parse_upload));

    let doc = OpenApi::new("yadoc2md API", env!("CARGO_PKG_VERSION")).merge_router(&api_router);

    Router::new()
        .get(root_redirect)
        .push(api_router)
        .unshift(doc.into_router(OPENAPI_JSON_PATH))
        .unshift(SwaggerUi::new(OPENAPI_JSON_PATH).into_router(SWAGGER_UI_PATH))
}

pub fn build_cors(origins: &[String]) -> impl Handler {
    let mut cors = Cors::new()
        .allow_methods(vec![Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers("*");

    if origins.len() == 1 && origins[0] == "*" {
        cors = cors.allow_origin("*");
    } else {
        for origin in origins {
            cors = cors.allow_origin(origin.as_str());
        }
    }

    cors.into_handler()
}

/// Build the HTTP service and listen address (testable without binding a socket).
pub fn prepare_service(args: &ServeArgs) -> (Service, String) {
    let max_body = args.max_body_size.unwrap_or(args.config.max_input_bytes);
    let cors_origins = normalize_cors_origins(&args.cors);

    let state = Arc::new(AppState {
        config: args.config.clone(),
    });

    let router = build_router(state, max_body as u64);
    let cors = build_cors(&cors_origins);
    let service = Service::new(router).hoop(cors);
    let addr = format!("{}:{}", args.host, args.port);
    (service, addr)
}

pub async fn run(args: ServeArgs) -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let (service, addr) = prepare_service(&args);
    tracing::info!(%addr, "listening");
    tracing::info!("OpenAPI spec at {OPENAPI_JSON_PATH}");
    tracing::info!("Swagger UI at {SWAGGER_UI_PATH}");
    let acceptor = TcpListener::new(addr).bind().await;
    Server::new(acceptor).serve(service).await;
    Ok(())
}

/// Redirect site root to Swagger UI.
#[handler]
async fn root_redirect() -> Redirect {
    Redirect::temporary(SWAGGER_UI_PATH)
}

/// Service health check.
#[endpoint(responses((status_code = 200, description = "Service is healthy", body = HealthResponse)))]
async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".into(),
    })
}

/// Convert an uploaded document to Markdown.
///
/// Send `multipart/form-data` with field `file`. Format is inferred from the filename extension.
#[endpoint(
    responses(
        (status_code = 200, description = "Converted markdown", body = String, content_type = "text/markdown; charset=utf-8"),
        (status_code = 400, description = "Bad request", body = ErrorResponse),
        (status_code = 413, description = "Payload too large", body = ErrorResponse),
        (status_code = 415, description = "Unsupported media type", body = ErrorResponse),
        (status_code = 422, description = "Conversion failed", body = ErrorResponse),
    )
)]
async fn parse_upload(file: FormFile, depot: &mut Depot, res: &mut Response) {
    let state = depot.obtain::<Arc<AppState>>().expect("AppState injected");

    let filename = file
        .name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "upload.bin".to_string());

    let data = match std::fs::read(file.path()) {
        Ok(d) => d,
        Err(e) => {
            write_error(res, UploadError::BadRequest(format!("failed to read upload: {e}")));
            return;
        }
    };

    match convert_upload(&filename, &data, &state.config) {
        Ok(markdown) => {
            res.headers_mut().insert(
                "Content-Type",
                "text/markdown; charset=utf-8".parse().unwrap(),
            );
            res.render(Text::Plain(markdown));
        }
        Err(err) => write_error(res, err),
    }
}

pub fn write_error(res: &mut Response, err: UploadError) {
    res.status_code(err.status_code());
    res.render(Json(ErrorResponse {
        error: err.message().to_string(),
    }));
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use salvo::http::StatusCode;
    use salvo::test::{ResponseExt, TestClient};

    use super::*;
    use crate::config::ConvertConfig;

    fn test_state() -> Arc<AppState> {
        Arc::new(AppState {
            config: ConvertConfig::default(),
        })
    }

    #[test]
    fn normalize_cors_defaults_to_wildcard() {
        assert_eq!(normalize_cors_origins(&[]), vec!["*"]);
    }

    #[test]
    fn normalize_cors_preserves_values() {
        let origins = vec!["http://a.com".into(), "http://b.com".into()];
        assert_eq!(normalize_cors_origins(&origins), origins);
    }

    #[test]
    fn upload_error_status_codes() {
        assert_eq!(
            UploadError::BadRequest("x".into()).status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            UploadError::PayloadTooLarge("x".into()).status_code(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
        assert_eq!(
            UploadError::UnsupportedMediaType("x".into()).status_code(),
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
        assert_eq!(
            UploadError::UnprocessableEntity("x".into()).status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[test]
    fn convert_upload_rejects_oversized_input() {
        let cfg = ConvertConfig {
            max_input_bytes: 4,
            ..Default::default()
        };
        let err = convert_upload("a.txt", b"12345", &cfg).unwrap_err();
        assert_eq!(
            err,
            UploadError::PayloadTooLarge(
                "upload exceeds --max-input-size (4 bytes)".to_string()
            )
        );
    }

    #[test]
    fn convert_upload_rejects_missing_extension() {
        let err = convert_upload("noext", b"hi", &ConvertConfig::default()).unwrap_err();
        assert!(matches!(err, UploadError::UnsupportedMediaType(_)));
    }

    #[test]
    fn convert_upload_converts_txt() {
        let md = convert_upload("f.txt", b"hello\n", &ConvertConfig::default()).unwrap();
        assert!(md.contains("hello"));
    }

    #[tokio::test]
    async fn root_redirects_to_swagger_ui() {
        let router = build_router(test_state(), 10 * 1024 * 1024);
        let res = TestClient::get("http://127.0.0.1/").send(router).await;
        assert_eq!(res.status_code, Some(StatusCode::TEMPORARY_REDIRECT));
        let location = res
            .headers
            .get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert_eq!(location, SWAGGER_UI_PATH);
    }

    #[tokio::test]
    async fn health_returns_ok_json() {
        let router = build_router(test_state(), 10 * 1024 * 1024);
        let mut res = TestClient::get("http://127.0.0.1/api/health")
            .send(router)
            .await;
        assert_eq!(res.status_code, Some(StatusCode::OK));
        let body: HealthResponse = res.take_json().await.unwrap();
        assert_eq!(body.status, "ok");
    }

    #[test]
    fn write_error_renders_json_body() {
        let mut res = Response::new();
        write_error(
            &mut res,
            UploadError::UnprocessableEntity("conversion failed".into()),
        );
        assert_eq!(res.status_code, Some(StatusCode::UNPROCESSABLE_ENTITY));
    }

    #[test]
    fn convert_upload_maps_anytomd_failure() {
        let data = b"body {";
        let err = convert_upload("bad.css", data, &ConvertConfig::default()).unwrap_err();
        assert!(matches!(err, UploadError::UnprocessableEntity(_)));
    }

    #[test]
    fn convert_upload_maps_input_too_large_from_convert() {
        let cfg = ConvertConfig {
            max_input_bytes: 2,
            ..Default::default()
        };
        let err = convert_upload("a.txt", b"abc", &cfg).unwrap_err();
        assert!(matches!(err, UploadError::PayloadTooLarge(_)));
    }

    #[test]
    fn prepare_service_uses_host_and_port() {
        let args = ServeArgs {
            config: ConvertConfig::default(),
            host: "127.0.0.1".into(),
            port: 9999,
            cors: vec!["http://example.com".into()],
            max_body_size: Some(1024),
        };
        let (_service, addr) = prepare_service(&args);
        assert_eq!(addr, "127.0.0.1:9999");
    }

    fn multipart_body(boundary: &str, filename: &str, data: &[u8]) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!(
                "Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
        body.extend_from_slice(data);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        body
    }

    #[tokio::test]
    async fn parse_upload_accepts_multipart_txt() {
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/sample.txt");
        let data = std::fs::read(&fixture).unwrap();
        let body = multipart_body("smokeboundary", "sample.txt", &data);
        let router = build_router(test_state(), 10 * 1024 * 1024);
        let mut res = TestClient::post("http://127.0.0.1/api/parse")
            .add_header(
                "content-type",
                "multipart/form-data; boundary=smokeboundary",
                true,
            )
            .body(body)
            .send(router)
            .await;
        assert_eq!(res.status_code, Some(StatusCode::OK));
        let text = res.take_string().await.unwrap();
        assert!(text.contains("This is a sample plain text file"));
    }

    #[tokio::test]
    async fn parse_upload_returns_415_for_unsupported_type() {
        let data = std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/sample.css"),
        )
        .unwrap();
        let body = multipart_body("smokeboundary", "sample.css", &data);
        let router = build_router(test_state(), 10 * 1024 * 1024);
        let mut res = TestClient::post("http://127.0.0.1/api/parse")
            .add_header(
                "content-type",
                "multipart/form-data; boundary=smokeboundary",
                true,
            )
            .body(body)
            .send(router)
            .await;
        assert!(
            matches!(
                res.status_code,
                Some(StatusCode::UNSUPPORTED_MEDIA_TYPE)
                    | Some(StatusCode::UNPROCESSABLE_ENTITY)
            )
        );
        let err: ErrorResponse = res.take_json().await.unwrap();
        assert!(err.error.contains("unsupported format") || err.error.contains("css"));
    }

    #[tokio::test]
    async fn cors_allows_configured_origin() {
        let api = Router::with_path("api")
            .hoop(affix_state::inject(test_state()))
            .push(Router::with_path("health").get(health));
        let service = Service::new(api).hoop(build_cors(&["http://example.com".into()]));
        let res = TestClient::options("http://127.0.0.1/api/health")
            .add_header("origin", "http://example.com", true)
            .add_header("access-control-request-method", "GET", true)
            .send(&service)
            .await;
        assert!(
            matches!(
                res.status_code,
                Some(StatusCode::OK) | Some(StatusCode::NO_CONTENT)
            )
        );
        let allow_origin = res
            .headers
            .get("access-control-allow-origin")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert_eq!(allow_origin, "http://example.com");
    }

    #[tokio::test]
    async fn cors_wildcard_allows_any_origin() {
        let api = Router::with_path("api").push(Router::with_path("health").get(health));
        let service = Service::new(api).hoop(build_cors(&["*".into()]));
        let res = TestClient::options("http://127.0.0.1/api/health")
            .add_header("origin", "http://anywhere.test", true)
            .add_header("access-control-request-method", "GET", true)
            .send(&service)
            .await;
        assert!(
            matches!(
                res.status_code,
                Some(StatusCode::OK) | Some(StatusCode::NO_CONTENT)
            )
        );
        let allow_origin = res
            .headers
            .get("access-control-allow-origin")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert_eq!(allow_origin, "*");
    }
}
