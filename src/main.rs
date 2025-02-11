use std::path::Path;
use std::io;
use ignore::{WalkBuilder, overrides::OverrideBuilder};
use clap::{Parser, Subcommand};
use std::fmt::Write as FmtWrite;

mod claude;
use claude::ClaudeClient;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Additional patterns to exclude (in .gitignore format)
    #[arg(short, long, value_delimiter = ',', global = true)]
    exclude: Option<Vec<String>>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List files in a format suitable for prompts, including file contents
    Prompt {
        /// Directory path to process
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
        /// Directory path to list
        path: String,
    },
    /// Generate README.md content using Claude and output to stdout
    Readme {
        /// Directory path to process
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
    let exclude_patterns = args.exclude.unwrap_or_default();
    
    match &args.command {
        Commands::Prompt { path, max_size, total_size } => {
            let path = Path::new(path);
            validate_directory(path)?;
            list_files_prompt(path, &exclude_patterns, *max_size, *total_size, std::io::stdout())?;
            Ok::<(), Box<dyn std::error::Error>>(())
        }
        Commands::List { path } => {
            let path = Path::new(path);
            validate_directory(path)?;
            list_files(path, &exclude_patterns)?;
            Ok::<(), Box<dyn std::error::Error>>(())
        }
        Commands::Readme { path, max_size, total_size } => {
            let path = Path::new(path);
            validate_directory(path)?;

            // Capture the output to a string
            let mut output_content = Vec::new();
            list_files_prompt(path, &exclude_patterns, *max_size, *total_size, &mut output_content)?;
            let files_content = String::from_utf8_lossy(&output_content).into_owned();

            // Send to Claude
            let client = ClaudeClient::new()?;

            let system_prompt = "You are a technical documentation expert. Your task is to create a concise but informative README.md file in markdown format based on the codebase content provided. Include:
1. Project name and brief description
2. Key features
3. Installation instructions if relevant
4. Basic usage examples
5. Project structure overview

Be concise and focus on the most important aspects. Use proper markdown formatting.

IMPORTANT: Output ONLY the markdown content. Do not include any other text, explanations, or metadata.";

            let readme_content = client.send_message(system_prompt, &files_content)
                .await?;

            // Print to stdout
            print!("{}", readme_content);
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
fn list_files_prompt<W: std::io::Write>(dir: &Path, exclude_patterns: &[String], max_file_size_kb: u64, max_total_size_mb: u64, mut writer: W) -> io::Result<()> {
    let max_file_size = max_file_size_kb * 1024;
    let max_total_size = max_total_size_mb * 1024 * 1024;
    let mut total_size = 0;
    // Create walker with exclusions
    let mut override_builder = OverrideBuilder::new(dir);
    for pattern in exclude_patterns {
        override_builder.add(&format!("!{}", pattern))
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, 
                format!("Invalid exclude pattern '{}': {}", pattern, e)))?;
    }
    
    let override_matcher = override_builder.build()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, 
            format!("Failed to build override matcher: {}", e)))?;
    
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
                    writeln!(writer, "Skipped (too large): {}\n", path.display())?;
                    continue;
                }
                
                if total_size + file_size > max_total_size {
                    writeln!(writer, "Reached total size limit of {} MB\n", max_total_size_mb)?;
                    break;
                }
                
                // Try to read the file contents
                match std::fs::read(path) {
                    Ok(bytes) => {
                        match String::from_utf8(bytes) {
                            Ok(content) => {
                                write!(writer, "{}", format_file_content(path, &content))?;
                                total_size += file_size;
                            }
                            Err(_) => {
                                writeln!(writer, "Skipped (not UTF-8): {}\n", path.display())?;
                            }
                        }
                    }
                    Err(e) => {
                        writeln!(writer, "Error reading {}: {}\n", path.display(), e)?;
                    }
                };
            }
            Err(err) => eprintln!("Error accessing entry: {}", err),
        }
    }
    

    Ok(())
}

fn validate_directory(path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "path does not exist"));
    }
    if !path.is_dir() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "path is not a directory"));
    }
    // Check if directory is readable by attempting to read its contents
    match path.read_dir() {
        Ok(_) => (),
        Err(e) => return Err(io::Error::new(io::ErrorKind::PermissionDenied, format!("directory is not readable: {}", e))),
    }
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

fn list_files(dir: &Path, exclude_patterns: &[String]) -> io::Result<()> {
    // Create an override builder for additional exclude patterns
    let mut override_builder = OverrideBuilder::new(dir);
    
    // Add each exclude pattern
    for pattern in exclude_patterns {
        override_builder.add(&format!("!{}", pattern))
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, 
                format!("Invalid exclude pattern '{}': {}", pattern, e)))?;
    }
    
    // Build the override matcher
    let override_matcher = override_builder.build()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, 
            format!("Failed to build override matcher: {}", e)))?;
    let walker = WalkBuilder::new(dir)
        .hidden(false)     // Show hidden files
        .git_ignore(true)  // Respect .gitignore files
        .ignore(true)      // Use standard ignore patterns
        .git_global(true)  // Use global gitignore
        .require_git(false) // Don't require git repo
        .overrides(override_matcher) // Add our custom exclude patterns
        .filter_entry(|e| {
            let file_name = e.file_name();
            let file_name_str = match file_name.to_str() {
                Some(s) => s,
                None => return true, // Keep entries with invalid UTF-8 names
            };

            // Skip SCM directories
            if file_name_str == ".git" || file_name_str == ".svn" || file_name_str == ".hg" {
                return false;
            }

            // Skip build executables
            if is_build_executable(file_name_str) {
                return false;
            }

            true
        })
        .build();

    for result in walker {
        match result {
            Ok(entry) => {
                if entry.file_type().map_or(false, |ft| ft.is_file()) {
                    println!("{}", entry.path().display());
                }
            }
            Err(err) => eprintln!("Error accessing entry: {}", err),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_validate_directory_with_valid_dir() {
        let temp_dir = TempDir::new().unwrap();
        assert!(validate_directory(temp_dir.path()).is_ok());
    }

    #[test]
    fn test_validate_directory_with_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "test").unwrap();
        
        let result = validate_directory(&file_path);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn test_validate_directory_with_nonexistent_path() {
        let path = Path::new("/nonexistent/path/that/should/not/exist");
        let result = validate_directory(path);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
    }
}
