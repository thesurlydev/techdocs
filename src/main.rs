use std::env;
use std::path::Path;
use std::process;
use std::io;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        eprintln!("Error: Expected exactly one argument (a local path)\nUsage: {} <path>", args[0]);
        process::exit(1);
    }

    let path = Path::new(&args[1]);
    
    match validate_directory(path) {
        Ok(_) => println!("Valid directory: {}", path.display()),
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
