use thiserror::Error;

/// Error type for file utility operations.
#[derive(Debug, Error)]
pub enum Error {
    /// IO error wrapper
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// Custom error with a message
    #[error("Custom error: {0}")]
    Custom(String),
}

/// Result type for file utility operations.
pub type Result<T> = std::result::Result<T, Error>;
use std::env;
use std::path::Component;
use std::path::{Path, PathBuf};

/// Returns a PathBuf that is relative to the current working directory (CWD).
/// If the given path cannot be made relative, it returns the original path.
///
/// # Arguments
/// * `path` - The path to make relative to the current working directory.
pub fn make_relative_to_cwd(path: &PathBuf) -> PathBuf {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let relative_path = pathdiff::diff_paths(&path, &cwd).unwrap_or(path.clone());
    normalize_path(&relative_path)
}

/// Compute the relative path from one file to another.
/// Both paths should be the dist paths (where files will be located).
///
/// # Arguments
/// * `from_path` - The source file path.
/// * `to_path` - The target file path.
///
/// # Returns
/// A string representing the relative path from `from_path` to `to_path`.
pub fn compute_relative_path(from_path: &Path, to_path: &Path) -> String {
    // Get the directory containing the from_path file
    let from_dir = from_path.parent().unwrap_or(Path::new(""));

    match pathdiff::diff_paths(to_path, from_dir) {
        Some(relative_path) => {
            let rel_str = relative_path.to_string_lossy().replace('\\', "/");
            if !rel_str.starts_with('.') {
                // If the path does not start with '.' or '/', it's a same-folder or subfolder import
                format!("./{}", rel_str)
            } else {
                rel_str
            }
        }
        None => {
            // Fallback: use absolute path if relative path computation fails
            to_path.to_string_lossy().replace('\\', "/")
        }
    }
}

/// Create the standard output directories (components, styles, assets, dependencies) inside the given output directory.
///
/// # Arguments
/// * `output_dir` - The base output directory.
///
/// # Returns
/// Result indicating success or error.
pub fn create_output_directories(output_dir: &Path) -> Result<()> {
    let dirs = ["components", "styles", "assets", "dependencies"];

    for dir in &dirs {
        let dir_path = output_dir.join(dir);
        std::fs::create_dir_all(&dir_path).map_err(|e| {
            Error::Custom(format!("Failed to create directory: {:?}: {e}", dir_path))
        })?;
    }

    Ok(())
}

/// Ensures that the given directory exists, creating it if necessary.
///
/// # Arguments
/// * `path` - The directory path to check or create.
///
/// # Returns
/// Result indicating success or error.
pub fn ensure_directory_exists(path: &Path) -> Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)
            .map_err(|e| Error::Custom(format!("Failed to create directory: {:?}: {e}", path)))?;
    }
    Ok(())
}

/// Normalizes a path by resolving `.` and `..` components, removing redundant elements.
///
/// # Arguments
/// * `path` - The path to normalize.
///
/// # Returns
/// A normalized PathBuf.
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut stack = Vec::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if let Some(last) = stack.last() {
                    match last {
                        Component::Normal(_) => {
                            stack.pop();
                        }
                        Component::RootDir | Component::Prefix(_) => {
                            // Don't pop root or prefix, keep parent
                            stack.push(Component::ParentDir);
                        }
                        _ => stack.push(Component::ParentDir),
                    }
                } else {
                    stack.push(Component::ParentDir);
                }
            }
            other => stack.push(other),
        }
    }

    stack.iter().map(|c| c.as_os_str()).collect()
}

/// Removes the given directory and all its contents, if it exists.
///
/// # Arguments
/// * `output_dir` - The directory to clear.
///
/// # Returns
/// Result indicating success or error.
pub(crate) fn clear_directory(output_dir: &Path) -> Result<()> {
    if output_dir.exists() {
        std::fs::remove_dir_all(output_dir).map_err(|e| {
            Error::Custom(format!("Failed to clear directory: {:?}: {e}", output_dir))
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_make_relative_to_cwd_returns_original_if_cwd_is_path() {
        let cwd = std::env::current_dir().unwrap();
        let rel = make_relative_to_cwd(&cwd);
        // Should be "." or empty, depending on normalization
        assert!(rel == PathBuf::from(".") || rel == PathBuf::from(""));
    }

    #[test]
    fn test_make_relative_to_cwd_with_absolute_path() {
        let cwd = std::env::current_dir().unwrap();
        let file = cwd.join("foo/bar.txt");
        let rel = make_relative_to_cwd(&file);
        assert_eq!(rel, PathBuf::from("foo/bar.txt"));
    }

    #[test]
    fn test_compute_relative_path_same_folder() {
        let from = Path::new("/tmp/a/b/file1.txt");
        let to = Path::new("/tmp/a/b/file2.txt");
        let rel = compute_relative_path(from, to);
        assert_eq!(rel, "./file2.txt");
    }

    #[test]
    fn test_compute_relative_path_parent_folder() {
        let from = Path::new("/tmp/a/b/c/file1.txt");
        let to = Path::new("/tmp/a/b/file2.txt");
        let rel = compute_relative_path(from, to);
        assert_eq!(rel, "../file2.txt");
    }

    #[test]
    fn test_normalize_path_simple() {
        let p = Path::new("foo/./bar/../baz");
        let norm = normalize_path(p);
        assert_eq!(norm, PathBuf::from("foo/baz"));
    }

    #[test]
    fn test_normalize_path_leading_parents() {
        let p = Path::new("../../foo/bar");
        let norm = normalize_path(p);
        assert_eq!(norm, PathBuf::from("../../foo/bar"));
    }

    #[test]
    fn test_normalize_path_more_middle_parents() {
        let p = Path::new("../../../test/../../../test2");
        let norm = normalize_path(p);
        assert_eq!(norm, PathBuf::from("../../../../../test2"));
    }
}
