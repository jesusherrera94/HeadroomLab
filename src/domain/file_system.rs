use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileSystemError {
    InvalidName(String),
    AlreadyExists(String),
    NotFound(String),
    Io(String),
}

impl fmt::Display for FileSystemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FileSystemError::InvalidName(name) => {
                write!(f, "\"{name}\" is not a valid name")
            }
            FileSystemError::AlreadyExists(name) => {
                write!(f, "\"{name}\" already exists here")
            }
            FileSystemError::NotFound(path) => write!(f, "\"{path}\" no longer exists"),
            FileSystemError::Io(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for FileSystemError {}

pub fn validate_entry_name(name: &str) -> Result<(), FileSystemError> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed == "."
        || trimmed == ".."
        || trimmed.contains('/')
        || trimmed.contains('\\')
    {
        return Err(FileSystemError::InvalidName(name.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ordinary_names() {
        assert!(validate_entry_name("effect.cpp").is_ok());
        assert!(validate_entry_name(".gitignore").is_ok());
        assert!(validate_entry_name("My Folder").is_ok());
    }

    #[test]
    fn rejects_empty_and_dot_names() {
        assert!(validate_entry_name("").is_err());
        assert!(validate_entry_name("   ").is_err());
        assert!(validate_entry_name(".").is_err());
        assert!(validate_entry_name("..").is_err());
    }

    #[test]
    fn rejects_path_separators() {
        assert!(validate_entry_name("a/b").is_err());
        assert!(validate_entry_name("a\\b").is_err());
        assert!(validate_entry_name("/etc/passwd").is_err());
    }
}
