```rust
use std::path::{Path, PathBuf};
use std::io::{self, Read};
use ignore::{WalkBuilder, overrides::OverrideBuilder};
use clap::{Parser, Subcommand};
use std::fmt::Write as FmtWrite;
use url::Url;
use git2::Repository;
use temp_dir::TempDir;
use std::error::Error;
use std::fs;
use tracing::{info, warn, error, debug, instrument};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

mod claude;
use claude::ClaudeClient;

/// Resolve a path or GitHub URL to a local directory path
#[instrument(skip_all, fields(path_or_url = %path_or_url), err)]
async fn resolve_path(path_or_url: &str) -> Result<(PathBuf, Option<TempDir>), Box<dyn Error>> {
    // Check if the input is a URL
    if let Ok(url) = Url::parse(path_or_url) {
        if url.scheme() == "https" && url.host_str() == Some("github.com") {
            info!("Detected GitHub URL, cloning repository");
            // Create a temporary directory
            let temp_dir = TempDir::new()?;
            let temp_path = temp_dir.path().to_path_buf();

            // Clone the repository
            debug!("Cloning repository to {}", temp_path.display());
            Repository::clone(path_or_url, &temp_path)?;
            info!("Successfully cloned repository");

            Ok((temp_path, Some(temp_dir)))
        } else {
            error!("Unsupported URL scheme or host");
            Err("Only GitHub URLs are supported".into())
        }
    } else {
        debug!("Input is a local path: {}", path_or_url);
        // It's a local path
        Ok((PathBuf::from(path_or_url), None))
    }
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Additional patterns to exclude (in .gitignore format)
    #[arg(short, long, value_delimiter = ',', global = true)]
    exclude: Option<Vec<String>>,

    /// Verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List files in a format suitable for prompts, including file contents
    Prompt {
        /// Directory path to process (default: current directory)
        #[arg(short, long, default_value = ".")]
        path: String,

        /// Maximum size in KB for files to include (default: 100)
        #[arg(short, long, default_value = "100")]
        max_size: u64,

        /// Maximum total output size in MB (default: 10)
        #[arg(short, long, default_value = "10")]
        total_size: u64,
    },
    /// List files in the directory
    List {
        /// Directory path to list (default: current directory)
        #[arg(short, long, default_value = ".")]
        path: String,
    },
    /// Generate README.md content using Claude and output to stdout
    Readme {
        /// Directory path to process (default: current directory)
        #[arg(short, long, default_value = ".")]
        path: String,

        /// Maximum size in KB for files to include (default: 100)
        #[arg(short, long, default_value = "100")]
        max_size: u64,

        /// Maximum total output size in MB (default: 10)
        #[arg(short, long, default_value = "10")]
        total_size: u64,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    
    // Initialize tracing based on verbosity flag
    let filter = if args.verbose {
        EnvFilter::from_default_env().add_directive("debug".parse()?)
    } else {
        EnvFilter::from_default_env().add_directive("info".parse()?)
    };
    
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .init();
    
    info!("Starting application");
    
    let exclude_patterns = args.exclude.unwrap_or_default();
    if !exclude_patterns.is_empty() {
        debug!("Exclude patterns: {:?}", exclude_patterns);
    }
    
    match &args.command {
        Commands::Prompt { path, max_size, total_size } => {
            info!("Running Prompt command");
            debug!(path = %path, max_size = %max_size, total_size = %total_size, "Prompt parameters");
            
            let (resolved_path, _temp_dir) = resolve_path(path).await?;
            validate_directory(&resolved_path)?;
            info!("Processing directory: {}", resolved_path.display());
            list_files_prompt(&resolved_path, &exclude_patterns, *max_size, *total_size, std::io::stdout())?;
            info!("Prompt command completed successfully");
            Ok::<(), Box<dyn std::error::Error>>(())
        }
        Commands::List { path } => {
            info!("Running List command");
            debug!(path = %path, "List parameters");
            
            let (resolved_path, _temp_dir) = resolve_path(path).await?;
            validate_directory(&resolved_path)?;
            info!("Listing files in: {}", resolved_path.display());
            list_files(&resolved_path, &exclude_patterns)?;
            info!("List command completed successfully");
            Ok::<(), Box<dyn std::error::Error>>(())
        }
        Commands::Readme { path, max_size, total_size } => {
            info!("Running Readme command");
            debug!(path = %path, max_size = %max_size, total_size = %total_size, "Readme parameters");
            
            let (resolved_path, _temp_dir) = resolve_path(path).await?;
            validate_directory(&resolved_path)?;
            info!("Processing directory: {}", resolved_path.display());

            // Capture the output to a string
            let mut output_content = Vec::new();
            list_files_prompt(&resolved_path, &exclude_patterns, *max_size, *total_size, &mut output_content)?;
            let files_content = String::from_utf8_lossy(&output_content).into_owned();
            debug!("Collected {} bytes of file content", files_content.len());

            // Send to Claude
            info!("Initializing Claude client");
            let client = ClaudeClient::new()?;

            // Read the system prompt from file
            info!("Reading system prompt from file");
            let mut system_prompt = String::new();
            fs::File::open("prompts/readme.txt")?
                .read_to_string(&mut system_prompt)?;

            info!("Sending request to Claude API");
            let readme_content = client.send_message(&system_prompt, &files_content)
                .await?;
            info!("Received response from Claude API ({} bytes)", readme_content.len());

            // Print to stdout
            print!("{}", readme_content);
            info!("Readme command completed successfully");
            Ok(())
        }
    }?;
    
    Ok(())
}

/// Format file contents for LLM consumption, including language detection
fn format_file_content(path: &Path, content: &str) -> String {
    let mut output = String::new();
    
    // Detect language from extension
    let lang = path.extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("txt");
    
    writeln!(&mut output, "File: {}", path.display()).unwrap();
    writeln!(&mut output, "```{}", lang).unwrap();
    output.push_str(content);
    writeln!(&mut output, "```\n").unwrap();
    
    output
}

/// List files in a format suitable for prompts
#[instrument(skip(dir, exclude_patterns, writer), fields(dir = %dir.display(), max_file_size_kb, max_total_size_mb), err)]
fn list_files_prompt<W: std::io::Write>(dir: &Path, exclude_patterns: &[String], max_file_size_kb: u64, max_total_size_mb: u64, mut writer: W) -> io::Result<()> {
    let max_file_size = max_file_size_kb * 1024;
    let max_total_size = max_total_size_mb * 1024 * 1024;
    let mut total_size = 0;
    
    // Create walker with exclusions
    let mut override_builder = OverrideBuilder::new(dir);
    for pattern in exclude_patterns {
        debug!("Adding exclude pattern: {}", pattern);
        override_builder.add(&format!("!{}", pattern))
            .map_err(|e| {
                error!("Invalid exclude pattern '{}': {}", pattern, e);
                io::Error::new(io::ErrorKind::InvalidInput, 
                    format!("Invalid exclude pattern '{}': {}", pattern, e))
            })?;
    }
    
    let override_matcher = override_builder.build()
        .map_err(|e| {
            error!("Failed to build override matcher: {}", e);
            io::Error::new(io::ErrorKind::Other, 
                format!("Failed to build override matcher: {}", e))
        })?;
    
    info!("Creating file walker");
    let walker = WalkBuilder::new(dir)
        .hidden(false)
        .git_ignore(true)
        .ignore(true)
        .git_global(true)
        .require_git(false)
        .overrides(override_matcher)
        .filter_entry(|e| {
            let file_name = e.file_name();
            let file_name_str = match file_name.to_str() {
                Some(s) => s,
                None => return true,
            };
            
            !is_build_executable(file_name_str)
        })
        .build();
    
    writeln!(writer, "Directory: {}\n", dir.display())?;
    
    let mut file_count = 0;
    let mut skipped_count = 0;
    
    for result in walker {
        match result {
            Ok(entry) => {
                if !entry.file_type().map_or(false, |ft| ft.is_file()) {
                    continue;
                }
                
                let path = entry.path();
                let metadata = path.metadata()?;
                let file_size = metadata.len();
                
                if file_size > max_file_size {
                    debug!("Skipping file (too large): {} ({} KB)", path.display(), file_size / 1024);
                    writeln!(writer, "Skipped (too large): {}\n", path.display())?;
                    skipped_count += 1;
                    continue;
                }
                
                if total_size + file_size > max_total_size {
                    info!("Reached total size limit of {} MB", max_total_size_mb);
                    writeln!(writer, "Reached total size limit of {} MB\n", max_total_size_mb)?;
                    break;
                }
                
                // Try to read the file contents
                match std::fs::read(path) {
                    Ok(bytes) => {
                        match String::from_utf8(bytes) {
                            Ok(content) => {
                                debug!("Including file: {} ({} KB)", path.display(), file_size / 1024);
                                write!(writer, "{}", format_file_content(path, &content))?;
                                total_size += file_size;
                                file_count += 1;
                            }
                            Err(_) => {
                                debug!("Skipping file (not UTF-8): {}", path.display());
                                writeln!(writer, "Skipped (not UTF-8): {}\n", path.display())?;
                                skipped_count += 1;
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Error reading {}: {}", path.display(), e);
                        writeln!(writer, "Error reading {}: {}\n", path.display(), e)?;
                        skipped_count += 1;
                    }
                };
            }
            Err(err) => {
                error!("Error accessing entry: {}", err);
                eprintln!("Error accessing entry: {}", err);
            }
        }
    }
    
    info!("Processed {} files ({} included, {} skipped, total size: {} KB)", 
          file_count + skipped_count, 
          file_count, 
          skipped_count,
          total_size / 1024);

    Ok(())
}

#[instrument(fields(path = %path.display()), err)]
fn validate_directory(path: &Path) -> io::Result<()> {
    if !path.exists() {
        error!("Path does not exist: {}", path.display());
        return Err(io::Error::new(io::ErrorKind::NotFound, "path does not exist"));
    }
    if !path.is_dir() {
        error!("Path is not a directory: {}", path.display());
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "path is not a directory"));
    }
    // Check if directory is readable by attempting to read its contents
    match path.read_dir() {
        Ok(_) => debug!("Directory is readable: {}", path.display()),
        Err(e) => {
            error!("Directory is not readable: {} ({})", path.display(), e);
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, format!("directory is not readable: {}", e)));
        }
    }
    info!("Directory is valid: {}", path.display());
    Ok(())
}

fn is_build_executable(file_name: &str) -> bool {
    // Common build tool executables and wrappers
    static BUILD_EXECUTABLES: [&str; 8] = [
        "mvnw",
        "mvnw.cmd",
        "gradlew",
        "gradlew.bat",
        "npm",
        "yarn",
        "pnpm",
        "cargo",
    ];
    
    BUILD_EXECUTABLES.contains(&file_name)
}

#[instrument(skip(dir, exclude_patterns), fields(dir = %dir.display()), err)]
fn list_files(dir: &Path, exclude_patterns: &[String]) -> io::Result<()> {
    // Create an override builder for additional exclude patterns
    let mut override_builder = OverrideBuilder::new(dir);
    
    // Add each exclude pattern
    for pattern in exclude_patterns {
        debug!("Adding exclude pattern: {}", pattern);
        override_builder.add(&format!("!{}", pattern))
            .map_err(|e| {
                error!("Invalid exclude pattern '{}': {}", pattern, e);
                io::Error::new(io::ErrorKind::InvalidInput, 
                    format!("Invalid exclude pattern '{}': {}", pattern, e))
            })?;
    }
    
    // Build the override matcher
    let override_matcher = override_builder.build()
        .map_err(|e| {
            error!("Failed to build override matcher: {}", e);
            io::Error::new