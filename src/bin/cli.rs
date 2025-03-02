use std::io::Read;
use clap::{Parser, Subcommand};
use techdocs::{
    claude::ClaudeClient,
    list_files, list_files_prompt, resolve_path, validate_directory,
    Result as TechDocsResult,
};
use tracing::{info, debug, error, instrument, Level};
use tracing_subscriber::{FmtSubscriber, EnvFilter};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Additional patterns to exclude (in .gitignore format)
    #[arg(short, long, value_delimiter = ',', global = true)]
    exclude: Option<Vec<String>>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info", global = true)]
    log_level: Level,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all files in a directory
    List {
        /// Path to directory or GitHub repository URL
        path_or_url: String,
    },
    /// Generate a prompt for README generation
    Prompt {
        /// Path to directory or GitHub repository URL
        path_or_url: String,
        /// Maximum file size in KB (default: 100)
        #[arg(long, default_value = "100")]
        max_file_size_kb: u64,
        /// Maximum total size in MB (default: 10)
        #[arg(long, default_value = "10")]
        max_total_size_mb: u64,
    },
    /// Generate a README for a directory
    Readme {
        /// Path to directory or GitHub repository URL
        path_or_url: String,
    },
}

#[tokio::main]
async fn main() -> TechDocsResult<()> {
    // Load environment variables from .env file
    dotenv::dotenv().ok();

    let args = Args::parse();
    
    // Initialize the tracing subscriber
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .with_max_level(args.log_level)
        .finish();
    
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");
    
    info!("Starting techdocs with log level: {}", args.log_level);
    
    let exclude_patterns = args.exclude.unwrap_or_default();
    debug!("Exclude patterns: {:?}", exclude_patterns);

    match args.command {
        Commands::List { path_or_url } => {
            list_command(&path_or_url, &exclude_patterns).await?;
        }
        Commands::Prompt {
            path_or_url,
            max_file_size_kb,
            max_total_size_mb,
        } => {
            prompt_command(&path_or_url, &exclude_patterns, max_file_size_kb, max_total_size_mb).await?;
        }
        Commands::Readme { path_or_url } => {
            readme_command(&path_or_url, &exclude_patterns).await?;
        }
    }

    info!("Command completed successfully");
    Ok(())
}

#[instrument(skip(exclude_patterns))]
async fn list_command(path_or_url: &str, exclude_patterns: &[String]) -> TechDocsResult<()> {
    info!("Listing files for path: {}", path_or_url);
    let (path, _temp_dir) = resolve_path(path_or_url).await?;
    debug!("Resolved path: {:?}", path);
    
    validate_directory(&path)?;
    info!("Directory validated");
    
    list_files(&path, exclude_patterns)?;
    Ok(())
}

#[instrument(skip(exclude_patterns))]
async fn prompt_command(
    path_or_url: &str, 
    exclude_patterns: &[String], 
    max_file_size_kb: u64, 
    max_total_size_mb: u64
) -> TechDocsResult<()> {
    info!(
        "Generating prompt for path: {} (max file size: {}KB, max total size: {}MB)",
        path_or_url, max_file_size_kb, max_total_size_mb
    );
    
    let (path, _temp_dir) = resolve_path(path_or_url).await?;
    debug!("Resolved path: {:?}", path);
    
    validate_directory(&path)?;
    info!("Directory validated");
    
    list_files_prompt(
        &path,
        exclude_patterns,
        max_file_size_kb,
        max_total_size_mb,
        std::io::stdout(),
    )?;
    
    Ok(())
}

#[instrument(skip(exclude_patterns))]
async fn readme_command(path_or_url: &str, exclude_patterns: &[String]) -> TechDocsResult<()> {
    info!("Generating README for path: {}", path_or_url);
    
    let (path, _temp_dir) = resolve_path(path_or_url).await?;
    debug!("Resolved path: {:?}", path);
    
    validate_directory(&path)?;
    info!("Directory validated");

    // Load system prompt
    debug!("Loading system prompt from prompts/readme.txt");
    let mut system_prompt = String::new();
    match std::fs::File::open("prompts/readme.txt") {
        Ok(mut file) => {
            file.read_to_string(&mut system_prompt)?;
            debug!("System prompt loaded, length: {} chars", system_prompt.len());
        },
        Err(e) => {
            error!("Failed to open system prompt file: {}", e);
            return Err(e.into());
        }
    }

    // Generate file list with prompt
    info!("Collecting file list for README generation");
    let mut file_list = Vec::new();
    list_files_prompt(&path, exclude_patterns, 100, 10, &mut file_list)?;
    debug!("File list generated, size: {} bytes", file_list.len());

    // Generate README using Claude
    info!("Initializing Claude client");
    let client = ClaudeClient::new()?;
    
    info!("Sending request to Claude for README generation");
    let readme = client
        .generate_readme(&system_prompt, &String::from_utf8_lossy(&file_list))
        .await?;
    info!("README generated successfully, length: {} chars", readme.len());

    println!("{}", readme);
    
    Ok(())
}