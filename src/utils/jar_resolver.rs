use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum JarResolverError {
    #[error("Invalid chrome URL: {0}")]
    InvalidChromeUrl(String),

    #[error("No mapping found for chrome URL: {0}")]
    NoMappingFound(String),

    #[error("Unknown ifdef condition: {0}")]
    UnknownIfdefCondition(String),

    #[error("Unmatched #endif directive")]
    UnmatchedEndif,

    #[error("Include file not found: {0}")]
    IncludeFileNotFound(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub struct JarResolver {
    mappings: HashMap<String, PathBuf>,
}

impl JarResolver {
    pub fn new(
        firefox_dir: &Path,
        jar_paths: &[&str],
        mozbuild_paths: &[&str],
        ifdef_config: Option<HashMap<String, bool>>,
    ) -> Result<Self, JarResolverError> {
        let mut mappings = HashMap::new();
        let mut chrome_registrations = HashMap::new();

        let mut default_ifdef_config = HashMap::new();
        default_ifdef_config.insert("MOZILLA_OFFICIAL".to_string(), true);
        default_ifdef_config.insert("ANDROID".to_string(), false);
        default_ifdef_config.insert("MOZ_GLEAN_ANDROID".to_string(), false);
        default_ifdef_config.insert("MOZ_FENNEC".to_string(), false);
        default_ifdef_config.insert("XP_MACOSX".to_string(), false);
        default_ifdef_config.insert("RELEASE_OR_BETA".to_string(), true);

        if let Some(config) = ifdef_config {
            default_ifdef_config.extend(config);
        }

        // Process jar.mn files
        for jar_path in jar_paths {
            let full_jar_path = firefox_dir.join(jar_path);

            if !full_jar_path.exists() {
                eprintln!(
                    "Warning: jar.mn file not found: {}",
                    full_jar_path.display()
                );
                continue;
            }

            match fs::read_to_string(&full_jar_path) {
                Ok(content) => {
                    // Process includes first
                    let processed_content =
                        process_includes(&content, &full_jar_path, firefox_dir)?;

                    if let Err(e) = parse_jar_file(
                        &processed_content,
                        jar_path,
                        firefox_dir,
                        &mut mappings,
                        &mut chrome_registrations,
                        &default_ifdef_config,
                    ) {
                        eprintln!(
                            "Error parsing jar.mn file {}: {}",
                            full_jar_path.display(),
                            e
                        );
                    }
                }
                Err(e) => {
                    eprintln!(
                        "Error reading jar.mn file {}: {}",
                        full_jar_path.display(),
                        e
                    );
                }
            }
        }

        // Process moz.build files for resource URLs
        for mozbuild_path in mozbuild_paths {
            let full_mozbuild_path = firefox_dir.join(mozbuild_path);

            if !full_mozbuild_path.exists() {
                eprintln!(
                    "Warning: moz.build file not found: {}",
                    full_mozbuild_path.display()
                );
                continue;
            }

            match fs::read_to_string(&full_mozbuild_path) {
                Ok(content) => {
                    if let Err(e) =
                        parse_mozbuild_file(&content, mozbuild_path, firefox_dir, &mut mappings)
                    {
                        eprintln!(
                            "Error parsing moz.build file {}: {}",
                            full_mozbuild_path.display(),
                            e
                        );
                    }
                }
                Err(e) => {
                    eprintln!(
                        "Error reading moz.build file {}: {}",
                        full_mozbuild_path.display(),
                        e
                    );
                }
            }
        }

        Ok(JarResolver { mappings })
    }

    pub fn is_internal_url(&self, url: &str) -> bool {
        url.starts_with("chrome://") || url.starts_with("resource://")
    }

    pub fn resolve_path(&self, url: &str) -> Result<PathBuf, JarResolverError> {
        if !self.is_internal_url(url) {
            return Err(JarResolverError::InvalidChromeUrl(url.to_string()));
        }

        self.mappings
            .get(url)
            .cloned()
            .ok_or_else(|| JarResolverError::NoMappingFound(url.to_string()))
    }
}

fn process_includes(
    content: &str,
    jar_file_path: &Path,
    firefox_dir: &Path,
) -> Result<String, JarResolverError> {
    let mut result = String::new();
    let jar_dir = jar_file_path.parent().unwrap_or(Path::new(""));

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("#include ") {
            let include_path = trimmed.strip_prefix("#include ").unwrap().trim();

            // Resolve the include path relative to the current jar file
            let full_include_path = if include_path.starts_with('/') {
                firefox_dir.join(include_path.strip_prefix('/').unwrap_or(include_path))
            } else {
                jar_dir.join(include_path)
            };

            // Read and include the file content
            match fs::read_to_string(&full_include_path) {
                Ok(include_content) => {
                    // Recursively process includes in the included file
                    let processed_include =
                        process_includes(&include_content, &full_include_path, firefox_dir)?;
                    result.push_str(&processed_include);
                    result.push('\n');
                }
                Err(_) => {
                    return Err(JarResolverError::IncludeFileNotFound(
                        full_include_path.display().to_string(),
                    ));
                }
            }
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }

    Ok(result)
}

fn parse_mozbuild_file(
    content: &str,
    mozbuild_path: &str,
    firefox_dir: &Path,
    mappings: &mut HashMap<String, PathBuf>,
) -> Result<(), JarResolverError> {
    let mozbuild_dir = Path::new(mozbuild_path).parent().unwrap_or(Path::new(""));
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();

        // Look for CONTENT_ACCESSIBLE_FILES
        if line.starts_with("CONTENT_ACCESSIBLE_FILES")
            && (line.contains("+=") || line.contains("="))
        {
            // Check if the list starts on the same line or next line
            if line.contains('[') {
                // List starts on the same line
                let start_bracket = line.find('[').unwrap();
                let after_bracket = &line[start_bracket + 1..];

                if after_bracket.trim().starts_with(']') {
                    // Empty list on same line
                    i += 1;
                    continue;
                }

                // Parse items starting from this line
                i = parse_content_accessible_files(&lines, i, mozbuild_dir, firefox_dir, mappings)?;
            } else {
                // Look for opening bracket on subsequent lines
                i += 1;
                while i < lines.len() {
                    let next_line = lines[i].trim();
                    if next_line.contains('[') {
                        i = parse_content_accessible_files(
                            &lines,
                            i,
                            mozbuild_dir,
                            firefox_dir,
                            mappings,
                        )?;
                        break;
                    }
                    i += 1;
                }
            }
        } else {
            i += 1;
        }
    }

    Ok(())
}

fn parse_content_accessible_files(
    lines: &[&str],
    start_idx: usize,
    mozbuild_dir: &Path,
    firefox_dir: &Path,
    mappings: &mut HashMap<String, PathBuf>,
) -> Result<usize, JarResolverError> {
    let mut i = start_idx;
    let mut in_list = false;

    while i < lines.len() {
        let line = lines[i].trim();

        if line.contains('[') {
            in_list = true;
            // Process any files on the same line after the bracket
            if let Some(bracket_pos) = line.find('[') {
                let after_bracket = &line[bracket_pos + 1..];
                if let Some(close_bracket) = after_bracket.find(']') {
                    // Complete list on one line
                    let files_str = &after_bracket[..close_bracket];
                    parse_file_list(files_str, mozbuild_dir, firefox_dir, mappings)?;
                    return Ok(i + 1);
                } else {
                    // List continues on next lines
                    let files_str = after_bracket;
                    parse_file_list(files_str, mozbuild_dir, firefox_dir, mappings)?;
                }
            }
        } else if in_list {
            if line.contains(']') {
                // End of list
                if let Some(close_bracket) = line.find(']') {
                    let files_str = &line[..close_bracket];
                    parse_file_list(files_str, mozbuild_dir, firefox_dir, mappings)?;
                }
                return Ok(i + 1);
            } else {
                // Continue parsing files
                parse_file_list(line, mozbuild_dir, firefox_dir, mappings)?;
            }
        }

        i += 1;
    }

    Ok(i)
}

fn parse_file_list(
    files_str: &str,
    mozbuild_dir: &Path,
    firefox_dir: &Path,
    mappings: &mut HashMap<String, PathBuf>,
) -> Result<(), JarResolverError> {
    // Split by comma and clean up each file path
    for file_part in files_str.split(',') {
        let file_path = file_part.trim().trim_matches('"').trim_matches('\'');

        if file_path.is_empty() {
            continue;
        }

        // Build the full source path
        let source_path = mozbuild_dir.join(file_path);
        let full_source_path = firefox_dir.join(&source_path);

        // Make the path relative to the current working directory
        let rel_source_path = super::file_utils::make_relative_to_cwd(&full_source_path);

        // Extract filename for the resource URL
        if let Some(filename) = Path::new(file_path).file_name() {
            if let Some(filename_str) = filename.to_str() {
                let resource_url = format!("resource://content-accessible/{}", filename_str);
                mappings.insert(resource_url, rel_source_path);
            }
        }
    }

    Ok(())
}

// Update parse_jar_file and related functions to use PathBuf for mappings
fn parse_jar_file(
    content: &str,
    jar_path: &str,
    firefox_dir: &Path,
    mappings: &mut HashMap<String, PathBuf>,
    chrome_registrations: &mut HashMap<String, ChromeRegistration>,
    ifdef_config: &HashMap<String, bool>,
) -> Result<(), JarResolverError> {
    let lines: Vec<&str> = content.lines().collect();
    let jar_dir = Path::new(jar_path).parent().unwrap_or(Path::new(""));
    let mut current_jar: Option<String> = None;
    let mut ifdef_stack = Vec::new();
    let mut currently_included = true;

    for line in lines {
        let line = line.trim();

        // Skip empty lines and handle comments/preprocessor directives
        if line.is_empty() || line.starts_with('#') {
            if line.starts_with("#ifdef ") || line.starts_with("#ifndef ") {
                let is_ifdef = line.starts_with("#ifdef ");
                let condition = if is_ifdef {
                    line.strip_prefix("#ifdef ").unwrap().trim()
                } else {
                    line.strip_prefix("#ifndef ").unwrap().trim()
                };

                let condition_value = ifdef_config.get(condition).ok_or_else(|| {
                    JarResolverError::UnknownIfdefCondition(condition.to_string())
                })?;

                let should_include = if is_ifdef {
                    *condition_value
                } else {
                    !*condition_value
                };

                ifdef_stack.push(currently_included);
                currently_included = currently_included && should_include;
            } else if line == "#endif" {
                if ifdef_stack.is_empty() {
                    return Err(JarResolverError::UnmatchedEndif);
                }
                currently_included = ifdef_stack.pop().unwrap();
            }
            continue;
        }

        // Skip if currently excluded by ifdef
        if !currently_included {
            continue;
        }

        // Skip lines starting with * (marked as special)
        if line.starts_with('*') {
            continue;
        }

        // Check if this is a jar declaration
        if line.ends_with(".jar:") {
            current_jar = Some(line.strip_suffix(':').unwrap().to_string());
            continue;
        }

        // Handle chrome registration lines (starting with %)
        if line.starts_with('%') {
            parse_registration_line(line, jar_dir, chrome_registrations)?;
            continue;
        }

        // Handle file mapping lines
        if current_jar.is_some() && line.contains('/') {
            parse_file_line(
                line,
                jar_dir,
                firefox_dir,
                current_jar.as_ref().unwrap(),
                mappings,
                chrome_registrations,
            )?;
        }
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct ChromeRegistration {
    registration_type: String,
    package_name: String,
    provider_name: String,
    path: String,
    flags: Vec<String>,
}

fn parse_registration_line(
    line: &str,
    _jar_dir: &Path, // Marked as unused with an underscore
    chrome_registrations: &mut HashMap<String, ChromeRegistration>,
) -> Result<(), JarResolverError> {
    // Remove the leading %
    let line = line.strip_prefix('%').unwrap().trim();
    let parts: Vec<&str> = line.split_whitespace().collect();

    if parts.len() < 3 {
        return Ok(()); // Skip malformed registration lines
    }

    let registration_type = parts[0].to_string(); // e.g., "content", "skin", "locale"
    let package_name = parts[1].to_string(); // e.g., "global", "browser"  

    // For content/skin/locale registrations, the format can be:
    // content packagename path [flags...]
    // skin packagename skinname path [flags...]
    // locale packagename localename path [flags...]

    let (provider_name, path, flags) = if registration_type == "content" {
        // content global %content/global/ contentaccessible=yes
        if parts.len() < 3 {
            return Ok(());
        }
        let path = parts[2].to_string();
        let flags = parts
            .get(3..)
            .unwrap_or(&[])
            .iter()
            .map(|s| s.to_string())
            .collect();
        (String::new(), path, flags)
    } else {
        // skin/locale format: type package provider path [flags...]
        if parts.len() < 4 {
            return Ok(());
        }
        let provider_name = parts[2].to_string();
        let path = parts[3].to_string();
        let flags = parts
            .get(4..)
            .unwrap_or(&[])
            .iter()
            .map(|s| s.to_string())
            .collect();
        (provider_name, path, flags)
    };

    let registration = ChromeRegistration {
        registration_type: registration_type.clone(),
        package_name: package_name.clone(),
        provider_name,
        path: path.clone(),
        flags,
    };

    // Use registration type and package as key
    let key = format!("{}:{}", registration_type, package_name);
    chrome_registrations.insert(key, registration);

    Ok(())
}

fn parse_file_line(
    line: &str,
    jar_dir: &Path,
    firefox_dir: &Path,
    current_jar: &str,
    mappings: &mut HashMap<String, PathBuf>,
    chrome_registrations: &HashMap<String, ChromeRegistration>,
) -> Result<(), JarResolverError> {
    let line = line.trim();

    // Parse the line format: destination_path (source_path) or just destination_path
    let (destination, source) = if line.contains('(') && line.contains(')') {
        // Extract source path from parentheses
        let start = line.find('(').unwrap();
        let end = line.find(')').unwrap();
        let destination = line[..start].trim();
        let source = line[start + 1..end].trim();
        (destination, Some(source))
    } else {
        // No source specified, use destination as source
        (line, None)
    };

    // Determine the actual source path
    let source_path = if let Some(src) = source {
        if src.starts_with('/') {
            // Absolute path from Firefox root
            PathBuf::from(src.strip_prefix('/').unwrap_or(src))
        } else {
            // Relative to jar directory
            jar_dir.join(src)
        }
    } else {
        // Same directory omission - extract filename from destination
        let filename = Path::new(destination)
            .file_name()
            .ok_or_else(|| JarResolverError::InvalidChromeUrl(destination.to_string()))?;
        jar_dir.join(filename)
    };

    // Build chrome URL from destination path
    if let Some(chrome_url) = build_chrome_url(destination, chrome_registrations) {
        let full_source_path = firefox_dir.join(&source_path);
        // Make the path relative to the current working directory
        let rel_source_path = super::file_utils::make_relative_to_cwd(&full_source_path);
        mappings.insert(chrome_url, rel_source_path);
    }

    Ok(())
}

fn build_chrome_url(
    destination: &str,
    chrome_registrations: &HashMap<String, ChromeRegistration>,
) -> Option<String> {
    // Parse destination path to find chrome registration
    let parts: Vec<&str> = destination.split('/').collect();
    if parts.is_empty() {
        return None;
    }

    let chrome_type = parts[0]; // e.g., "skin", "content", "locale"

    // Look for matching chrome registration by type
    for registration in chrome_registrations.values() {
        if registration.registration_type == chrome_type {
            // The destination path should match the registration path pattern
            // For example: if registration path is %skin/classic/browser/
            // and destination is skin/classic/browser/monitor-border.png
            // then the chrome URL should be chrome://browser/skin/monitor-border.png

            let reg_path = registration.path.trim_start_matches('%');
            let reg_path = reg_path.trim_end_matches('/');

            // Check if the destination path starts with the registration path
            if destination.starts_with(reg_path) {
                // Extract the relative path after the registration path
                let relative_path = destination
                    .strip_prefix(reg_path)
                    .unwrap_or("")
                    .trim_start_matches('/')
                    .to_string();

                // Build the chrome URL: chrome://package/type/relative_path
                return Some(format!(
                    "chrome://{}/{}/{}",
                    registration.package_name, chrome_type, relative_path
                ));
            }
        }
    }

    None
}
