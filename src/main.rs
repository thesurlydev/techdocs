use std::env;
use std::path::Path;
use std::process;
use std::io;
use ignore::WalkBuilder;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        eprintln!("Error: Expected exactly one argument (a local path)\nUsage: {} <path>", args[0]);
        process::exit(1);
    }

    let path = Path::new(&args[1]);
    
    match validate_directory(path) {
        Ok(_) => {
            match list_files(path) {
                Ok(_) => (),
                Err(e) => {
                    eprintln!("Error listing files: {}", e);
                    process::exit(1);
                }
            }
        },
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    }
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

fn list_files(dir: &Path) -> io::Result<()> {
    let walker = WalkBuilder::new(dir)
        .hidden(false)     // Show hidden files
        .git_ignore(true)  // Respect .gitignore files
        .ignore(true)      // Use standard ignore patterns
        .git_global(true)  // Use global gitignore
        .require_git(false) // Don't require git repo
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
