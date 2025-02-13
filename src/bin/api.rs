use std::io::Read;
use std::net::SocketAddr;
use axum::{self,
    routing::{get, post},
    http::StatusCode,
    Json, Router,
    extract::State,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use techdocs::{
    claude::ClaudeClient,
    list_files_prompt, resolve_path,
    Result as TechDocsResult,
};

#[derive(Clone)]
struct AppState {
    claude_client: Arc<ClaudeClient>,
    readme_prompt: String,
}

#[derive(Debug, Deserialize)]
struct GenerateReadmeRequest {
    path_or_url: String,
    exclude_patterns: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
struct GenerateReadmeResponse {
    readme: String,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

async fn health_check() -> StatusCode {
    StatusCode::OK
}

async fn generate_readme(
    State(state): State<AppState>,
    Json(request): Json<GenerateReadmeRequest>,
) -> Result<Json<GenerateReadmeResponse>, (StatusCode, Json<ErrorResponse>)> {
    let exclude_patterns = request.exclude_patterns.unwrap_or_default();

    // Resolve path (local or GitHub URL)
    let (path, _temp_dir) = resolve_path(&request.path_or_url)
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
        })?;

    // Generate file list with prompt
    let mut file_list = Vec::new();
    list_files_prompt(
        &path,
        &exclude_patterns,
        100,  // max file size in KB
        10,   // max total size in MB
        &mut file_list,
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    // Generate README using Claude
    let readme = state
        .claude_client
        .generate_readme(&state.readme_prompt, &String::from_utf8_lossy(&file_list))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
        })?;

    Ok(Json(GenerateReadmeResponse { readme }))
}

#[tokio::main]
async fn main() -> TechDocsResult<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "techdocs=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Initialize Claude client
    let claude_client = Arc::new(ClaudeClient::new()?);

    // Load README prompt
    let mut readme_prompt = String::new();
    std::fs::File::open("prompts/readme.txt")?
        .read_to_string(&mut readme_prompt)?;

    // Create app state
    let state = AppState {
        claude_client,
        readme_prompt,
    };

    // Build router
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/generate", post(generate_readme))
        .with_state(state)
        .layer(TraceLayer::new_for_http());

    // Start server
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("Listening on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
