use thiserror::Error;
#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Custom error: {0}")]
    Custom(String),
}

pub type Result<T> = std::result::Result<T, Error>;
use std::env;
use std::path::{Path, PathBuf};

/// Returns a PathBuf that is relative to the current working directory (CWD).
/// If the given path cannot be made relative, it returns the original path.
pub fn make_relative_to_cwd(path: &PathBuf) -> PathBuf {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let relative_path = pathdiff::diff_paths(&path, &cwd).unwrap_or(path.clone());
    normalize_path(&relative_path)
}

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

pub fn copy_file_if_newer(src: &Path, dest: &Path) -> Result<bool> {
    if !src.exists() {
        return Ok(false);
    }

    // Create parent directory if it doesn't exist
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Error::Custom(format!("Failed to create directory: {:?}: {e}", parent)))?;
    }

    // Check if we need to copy (dest doesn't exist or src is newer)
    let should_copy = if dest.exists() {
        let src_modified = src
            .metadata()
            .map_err(|e| Error::Custom(format!("Failed to get metadata for: {:?}: {e}", src)))?
            .modified()
            .map_err(|e| {
                Error::Custom(format!("Failed to get modified time for: {:?}: {e}", src))
            })?;

        let dest_modified = dest
            .metadata()
            .map_err(|e| Error::Custom(format!("Failed to get metadata for: {:?}: {e}", dest)))?
            .modified()
            .map_err(|e| {
                Error::Custom(format!("Failed to get modified time for: {:?}: {e}", dest))
            })?;

        src_modified > dest_modified
    } else {
        true
    };

    if should_copy {
        std::fs::copy(src, dest).map_err(|e| {
            Error::Custom(format!(
                "Failed to copy file from {:?} to {:?}: {e}",
                src, dest
            ))
        })?;
    }

    Ok(should_copy)
}

pub fn ensure_directory_exists(path: &Path) -> Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)
            .map_err(|e| Error::Custom(format!("Failed to create directory: {:?}: {e}", path)))?;
    }
    Ok(())
}

pub fn get_relative_path(from: &Path, to: &Path) -> Option<PathBuf> {
    pathdiff::diff_paths(to, from)
}

pub fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    let mut starts_with_parent = false;

    for component in path.components() {
        match component {
            std::path::Component::CurDir => {
                // Skip current directory references
            }
            std::path::Component::ParentDir => {
                if components.is_empty() {
                    // Keep leading ".." components
                    starts_with_parent = true;
                } else {
                    // Handle parent directory references
                    components.pop();
                }
            }
            other => {
                components.push(other);
            }
        }
    }

    let mut normalized_path: PathBuf = components.iter().collect();
    if starts_with_parent {
        normalized_path = PathBuf::from("..").join(normalized_path);
    }

    normalized_path
}

pub fn is_text_file(path: &Path) -> bool {
    if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
        matches!(
            extension.to_lowercase().as_str(),
            "js" | "mjs"
                | "ts"
                | "tsx"
                | "css"
                | "scss"
                | "less"
                | "html"
                | "htm"
                | "xml"
                | "json"
                | "txt"
                | "md"
        )
    } else {
        false
    }
}

pub fn is_image_file(path: &Path) -> bool {
    if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
        matches!(
            extension.to_lowercase().as_str(),
            "svg" | "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico"
        )
    } else {
        false
    }
}

pub fn read_file_with_fallback_encoding(path: &Path) -> Result<String> {
    // First try UTF-8
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(_) => {
            // Fallback: read as bytes and try to convert
            let bytes = std::fs::read(path).map_err(|e| {
                Error::Custom(format!("Failed to read file as bytes: {:?}: {e}", path))
            })?;

            // Try to decode as UTF-8, replacing invalid sequences
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        }
    }
}

pub(crate) fn clear_directory(output_dir: &Path) -> Result<()> {
    if output_dir.exists() {
        std::fs::remove_dir_all(output_dir).map_err(|e| {
            Error::Custom(format!("Failed to clear directory: {:?}: {e}", output_dir))
        })?;
    }
    Ok(())
}
