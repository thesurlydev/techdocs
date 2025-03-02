use std::path::{Path, PathBuf};
use std::io::{self, Read};
use ignore::{WalkBuilder, overrides::OverrideBuilder};
use url::Url;
use git2::Repository;
use temp_dir::TempDir;
use std::fs;
use std::error::Error as StdError;
use tracing::{debug, error, info, instrument, warn};

pub mod claude;

#[derive(Debug, thiserror::Error)]
pub enum TechDocsError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Git error: {0}")]
    Git(#[from] git2::Error),
    #[error("Claude error: {0}")]
    Claude(String),
    #[error("Invalid URL: {0}")]
    Url(String),
    #[error("Ignore error: {0}")]
    Ignore(#[from] ignore::Error),
    #[error("{0}")]
    Other(#[from] Box<dyn StdError>),
}

pub type Result<T> = std::result::Result<T, TechDocsError>;

/// Resolve a path or GitHub URL to a local directory path
#[instrument(skip_all, fields(path_or_url = %path_or_url))]
pub async fn resolve_path(path_or_url: &str) -> Result<(PathBuf, Option<TempDir>)> {
    debug!("Resolving path or URL");
    // Check if the input is a URL
    if let Ok(url) = Url::parse(path_or_url) {
        if url.scheme() == "https" && url.host_str() == Some("github.com") {
            info!("Cloning GitHub repository");
            // Create a temporary directory
            let temp_dir = TempDir::new().map_err(|e| {
                error!(?e, "Failed to create temporary directory");
                e
            })?;
            let temp_path = temp_dir.path().to_path_buf();

            // Clone the repository
            Repository::clone(path_or_url, &temp_path).map_err(|e| {
                error!(?e, "Failed to clone repository");
                e
            })?;
            
            info!(temp_path = %temp_path.display(), "Successfully cloned repository");
            Ok((temp_path, Some(temp_dir)))
        } else {
            error!("Unsupported URL scheme or host");
            Err(TechDocsError::Url("Only GitHub URLs are supported".into()))
        }
    } else {
        info!(path = %path_or_url, "Using local path");
        Ok((PathBuf::from(path_or_url), None))
    }
}

#[instrument(skip(path), fields(path = %path.display()))]
pub fn validate_directory(path: &Path) -> io::Result<()> {
    debug!("Validating directory");
    if !path.exists() {
        error!("Path does not exist");
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Path does not exist"
        ));
    }
    if !path.is_dir() {
        error!("Path is not a directory");
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Path is not a directory"
        ));
    }
    info!("Directory validated successfully");
    Ok(())
}

#[instrument(level = "debug")]
pub fn is_build_executable(file_name: &str) -> bool {
    let build_executables = [
        "target", "node_modules", "build", "dist", "out", "bin",
        "Debug", "Release", ".git", ".idea", ".vscode"
    ];
    let result = build_executables.iter().any(|&x| file_name.contains(x));
    debug!(is_build_executable = result);
    result
}

/// Format file contents for LLM consumption, including language detection
#[instrument(skip(content), fields(path = %path.display()))]
pub fn format_file_content(path: &Path, content: &str) -> String {
    let extension = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("txt");
    
    debug!(extension, "Formatting file content");
    format!("```{}\n{}\n```", extension, content)
}

/// List files in a format suitable for prompts
#[instrument(skip(dir, exclude_patterns, writer), fields(dir = %dir.display(), max_file_size_kb, max_total_size_mb))]
pub fn list_files_prompt<W: io::Write>(
    dir: &Path,
    exclude_patterns: &[String],
    max_file_size_kb: u64,
    max_total_size_mb: u64,
    mut writer: W,
) -> Result<()> {
    info!("Listing files for prompt");
    debug!(exclude_patterns = ?exclude_patterns, "Building overrides");
    
    let mut override_builder = OverrideBuilder::new(dir);
    for pattern in exclude_patterns {
        override_builder.add(pattern).map_err(|e| {
            error!(pattern = %pattern, ?e, "Failed to add override pattern");
            e
        })?;
    }
    let overrides = override_builder.build()?;

    let max_file_size = max_file_size_kb * 1024;
    let max_total_size = max_total_size_mb * 1024 * 1024;
    let mut total_size = 0;
    let mut file_count = 0;

    debug!("Creating file walker");
    let walker = WalkBuilder::new(dir)
        .standard_filters(true)
        .overrides(overrides)
        .build();

    for entry in walker {
        let entry = entry.map_err(|e| {
            error!(?e, "Error walking directory");
            e
        })?;
        let path = entry.path();

        if path.is_file() {
            let file_name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            if is_build_executable(file_name) {
                debug!(file = %path.display(), "Skipping build/executable file");
                continue;
            }

            let metadata = entry.metadata().map_err(|e| {
                error!(file = %path.display(), ?e, "Failed to get file metadata");
                e
            })?;
            let file_size = metadata.len();

            if file_size > max_file_size {
                debug!(file = %path.display(), file_size, max_file_size, "Skipping file exceeding size limit");
                continue;
            }

            if total_size + file_size > max_total_size {
                warn!(total_size, max_total_size, "Total size limit reached");
                writeln!(writer, "Warning: Total size limit reached, some files omitted.")?;
                break;
            }

            total_size += file_size;
            file_count += 1;
            debug!(file = %path.display(), file_size, "Processing file");

            let mut content = Vec::new();
            fs::File::open(path).map_err(|e| {
                error!(file = %path.display(), ?e, "Failed to open file");
                e
            })?.read_to_end(&mut content).map_err(|e| {
                error!(file = %path.display(), ?e, "Failed to read file");
                e
            })?;
            let content_str = String::from_utf8_lossy(&content);

            writeln!(writer, "\nFile: {}", path.display())?;
            writeln!(writer, "{}", format_file_content(path, &content_str))?;
        }
    }

    info!(file_count, total_size, "Finished listing files for prompt");
    Ok(())
}

/// List files in the directory
#[instrument(skip(dir, exclude_patterns), fields(dir = %dir.display()))]
pub fn list_files(dir: &Path, exclude_patterns: &[String]) -> Result<()> {
    info!("Listing files in directory");
    debug!(exclude_patterns = ?exclude_patterns, "Building overrides");
    
    let mut override_builder = OverrideBuilder::new(dir);
    for pattern in exclude_patterns {
        override_builder.add(pattern).map_err(|e| {
            error!(pattern = %pattern, ?e, "Failed to add override pattern");
            e
        })?;
    }
    let overrides = override_builder.build()?;

    debug!("Creating file walker");
    let walker = WalkBuilder::new(dir)
        .standard_filters(true)
        .overrides(overrides)
        .build();

    let mut file_count = 0;
    for entry in walker {
        let entry = entry.map_err(|e| {
            error!(?e, "Error walking directory");
            e
        })?;
        let path = entry.path();

        if path.is_file() {
            let file_name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            if is_build_executable(file_name) {
                debug!(file = %path.display(), "Skipping build/executable file");
                continue;
            }

            file_count += 1;
            debug!(file = %path.display(), "Listing file");
            println!("{}", path.display());
        }
    }

    info!(file_count, "Finished listing files");
    Ok(())
}