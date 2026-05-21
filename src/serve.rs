use std::sync::Arc;

use clap::Args;
use salvo::affix_state;
use salvo::cors::Cors;
use salvo::http::Method;
use salvo::oapi::extract::FormFile;
use salvo::oapi::{endpoint, ToSchema};
use salvo::prelude::*;
use salvo::size_limiter::max_size;
use serde::Serialize;

use crate::config::{parse_byte_size, ConvertConfig};
use crate::convert::{convert, AppError};

const OPENAPI_JSON_PATH: &str = "/api-doc/openapi.json";
const SWAGGER_UI_PATH: &str = "/swagger-ui";

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
struct AppState {
    config: ConvertConfig,
}

#[derive(Debug, Serialize, ToSchema)]
struct HealthResponse {
    status: String,
}

#[derive(Debug, Serialize, ToSchema)]
struct ErrorResponse {
    error: String,
}

pub async fn run(args: ServeArgs) -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let max_body = args.max_body_size.unwrap_or(args.config.max_input_bytes);
    let cors_origins: Vec<String> = if args.cors.is_empty() {
        vec!["*".into()]
    } else {
        args.cors.clone()
    };

    let state = Arc::new(AppState {
        config: args.config,
    });

    let api_router = Router::with_path("api")
        .hoop(affix_state::inject(state))
        .hoop(max_size(max_body as u64))
        .push(Router::with_path("health").get(health))
        .push(Router::with_path("parse").post(parse_upload));

    let doc = OpenApi::new("yadoc2md API", env!("CARGO_PKG_VERSION")).merge_router(&api_router);

    let router = Router::new()
        .push(api_router)
        .unshift(doc.into_router(OPENAPI_JSON_PATH))
        .unshift(SwaggerUi::new(OPENAPI_JSON_PATH).into_router(SWAGGER_UI_PATH));

    let cors = build_cors(&cors_origins);
    let service = Service::new(router).hoop(cors);

    let addr = format!("{}:{}", args.host, args.port);
    tracing::info!(%addr, "listening");
    tracing::info!("OpenAPI spec at {OPENAPI_JSON_PATH}");
    tracing::info!("Swagger UI at {SWAGGER_UI_PATH}");
    let acceptor = TcpListener::new(addr).bind().await;
    Server::new(acceptor).serve(service).await;
    Ok(())
}

fn build_cors(origins: &[String]) -> impl Handler {
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
            json_error(
                res,
                StatusCode::BAD_REQUEST,
                &format!("failed to read upload: {e}"),
            );
            return;
        }
    };

    if data.len() > state.config.max_input_bytes {
        json_error(
            res,
            StatusCode::PAYLOAD_TOO_LARGE,
            &format!(
                "upload exceeds --max-input-size ({} bytes)",
                state.config.max_input_bytes
            ),
        );
        return;
    }

    match convert(&filename, &data, &state.config) {
        Ok(markdown) => {
            res.headers_mut().insert(
                "Content-Type",
                "text/markdown; charset=utf-8".parse().unwrap(),
            );
            res.render(Text::Plain(markdown));
        }
        Err(AppError::InvalidInput(msg)) => {
            json_error(res, StatusCode::UNSUPPORTED_MEDIA_TYPE, &msg);
        }
        Err(e) => {
            json_error(res, StatusCode::UNPROCESSABLE_ENTITY, &e.to_string());
        }
    }
}

fn json_error(res: &mut Response, status: StatusCode, message: &str) {
    res.status_code(status);
    res.render(Json(ErrorResponse {
        error: message.to_string(),
    }));
}
