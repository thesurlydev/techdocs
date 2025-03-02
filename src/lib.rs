use std::path::{Path, PathBuf};
use std::io::{self, Read};
use ignore::{WalkBuilder, overrides::OverrideBuilder};
use url::Url;
use git2::Repository;
use temp_dir::TempDir;
use std::fs;
use std::error::Error as StdError;
use claude_client::claude::ClaudeClient;


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
    #[error("Claude client error: {0}")]
    ClaudeClient(String),
    #[error("{0}")]
    Other(#[from] Box<dyn StdError>),
}

pub type Result<T> = std::result::Result<T, TechDocsError>;

/// Resolve a path or GitHub URL to a local directory path
pub async fn resolve_path(path_or_url: &str) -> Result<(PathBuf, Option<TempDir>)> {
    // Check if the input is a URL
    if let Ok(url) = Url::parse(path_or_url) {
        if url.scheme() == "https" && url.host_str() == Some("github.com") {
            // Create a temporary directory
            let temp_dir = TempDir::new()?;
            let temp_path = temp_dir.path().to_path_buf();

            // Clone the repository
            Repository::clone(path_or_url, &temp_path)?;

            Ok((temp_path, Some(temp_dir)))
        } else {
            Err(TechDocsError::Url("Only GitHub URLs are supported".into()))
        }
    } else {
        // It's a local path
        Ok((PathBuf::from(path_or_url), None))
    }
}

pub fn validate_directory(path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Path does not exist"
        ));
    }
    if !path.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Path is not a directory"
        ));
    }
    Ok(())
}

pub fn is_build_executable(file_name: &str) -> bool {
    let build_executables = [
        "target", "node_modules", "build", "dist", "out", "bin",
        "Debug", "Release", ".git", ".idea", ".vscode"
    ];
    build_executables.iter().any(|&x| file_name.contains(x))
}

/// Format file contents for LLM consumption, including language detection
pub fn format_file_content(path: &Path, content: &str) -> String {
    let extension = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("txt");
    
    format!("```{}\n{}\n```", extension, content)
}

/// List files in a format suitable for prompts
pub fn list_files_prompt<W: io::Write>(
    dir: &Path,
    exclude_patterns: &[String],
    max_file_size_kb: u64,
    max_total_size_mb: u64,
    mut writer: W,
) -> Result<()> {
    let mut override_builder = OverrideBuilder::new(dir);
    for pattern in exclude_patterns {
        override_builder.add(pattern)?;
    }
    let overrides = override_builder.build()?;

    let max_file_size = max_file_size_kb * 1024;
    let max_total_size = max_total_size_mb * 1024 * 1024;
    let mut total_size = 0;

    let walker = WalkBuilder::new(dir)
        .standard_filters(true)
        .overrides(overrides)
        .build();

    for entry in walker {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            let file_name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            if is_build_executable(file_name) {
                continue;
            }

            let metadata = entry.metadata()?;
            let file_size = metadata.len();

            if file_size > max_file_size {
                continue;
            }

            if total_size + file_size > max_total_size {
                writeln!(writer, "Warning: Total size limit reached, some files omitted.")?;
                break;
            }

            total_size += file_size;

            let mut content = Vec::new();
            fs::File::open(path)?.read_to_end(&mut content)?;
            let content_str = String::from_utf8_lossy(&content);

            writeln!(writer, "\nFile: {}", path.display())?;
            writeln!(writer, "{}", format_file_content(path, &content_str))?;
        }
    }

    Ok(())
}

/// List files in the directory
pub fn list_files(dir: &Path, exclude_patterns: &[String]) -> Result<()> {
    let mut override_builder = OverrideBuilder::new(dir);
    for pattern in exclude_patterns {
        override_builder.add(pattern)?;
    }
    let overrides = override_builder.build()?;

    let walker = WalkBuilder::new(dir)
        .standard_filters(true)
        .overrides(overrides)
        .build();

    for entry in walker {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            let file_name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            if is_build_executable(file_name) {
                continue;
            }

            println!("{}", path.display());
        }
    }

    Ok(())
}

/// Generate a README.md file using Claude AI based on the codebase content
/// 
/// # Arguments
/// * `system_prompt` - The system prompt to use for Claude
/// * `files_content` - The content of the files to analyze
/// 
/// # Returns
/// A string containing the generated README.md content
pub async fn generate_readme(system_prompt: &str, files_content: &str) -> Result<String> {
    // Initialize Claude client
    let client = ClaudeClient::new()
        .map_err(|e| TechDocsError::ClaudeClient(e.to_string()))?;
    
    // Send request to Claude
    let readme_content = client
        .send_message(
            None, // Use default model
            system_prompt,
            files_content
        )
        .await
        .map_err(|e| TechDocsError::ClaudeClient(e.to_string()))?;
    
    Ok(readme_content)
}
