use std::io::Read;
use clap::{Parser, Subcommand};
// use claude_client::claude::ClaudeClient; // Not needed anymore
use techdocs::{
    list_files, list_files_prompt, resolve_path, validate_directory, generate_readme,
    Result as TechDocsResult,
};

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
    let exclude_patterns = args.exclude.unwrap_or_default();

    match args.command {
        Commands::List { path_or_url } => {
            let (path, _temp_dir) = resolve_path(&path_or_url).await?;
            validate_directory(&path)?;
            list_files(&path, &exclude_patterns)?;
        }
        Commands::Prompt {
            path_or_url,
            max_file_size_kb,
            max_total_size_mb,
        } => {
            let (path, _temp_dir) = resolve_path(&path_or_url).await?;
            validate_directory(&path)?;
            list_files_prompt(
                &path,
                &exclude_patterns,
                max_file_size_kb,
                max_total_size_mb,
                std::io::stdout(),
            )?;
        }
        Commands::Readme { path_or_url } => {
            let (path, _temp_dir) = resolve_path(&path_or_url).await?;
            validate_directory(&path)?;

            // Load system prompt
            let mut system_prompt = String::new();
            std::fs::File::open("prompts/readme.txt")?
                .read_to_string(&mut system_prompt)?;

            // Generate file list with prompt
            let mut file_list = Vec::new();
            list_files_prompt(&path, &exclude_patterns, 100, 10, &mut file_list)?;

            // Generate README using Claude
            let readme = generate_readme(&system_prompt, &String::from_utf8_lossy(&file_list))
                .await?;

            println!("{}", readme);
        }
    }

    Ok(())
}
