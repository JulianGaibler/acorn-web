use thiserror::Error;
#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Glob error: {0}")]
    Glob(#[from] glob::PatternError),
    #[error("Glob walk error: {0}")]
    GlobWalk(#[from] glob::GlobError),
    #[error("Custom error: {0}")]
    Custom(String),
}

pub type Result<T> = std::result::Result<T, Error>;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

mod dependencies;
mod dependency_graph;
mod errors;
mod transform;
mod utils;

use dependency_graph::{DependencyGraph, FileType};
use glob::glob;
use utils::{file_utils, jar_resolver};

use crate::{dependency_graph::TargetLocation, utils::path_finder::PathFinder};
use std::collections::HashSet;

pub fn transform_lib(
    firefox_root: &Path,
    output_path: &str,
    jar_paths: &[&str],
    mozbuild_paths: &[&str],
    global_stylesheets: &[&str],
    component_paths: &[&str],
) -> Result<()> {
    // Parse JAR mappings for chrome:// URL resolution
    let jr = jar_resolver::JarResolver::new(firefox_root, jar_paths, mozbuild_paths, None)
        .map_err(|e| Error::Custom(format!("Failed to parse JAR mappings: {e}")))?;

    let pf = utils::path_finder::PathFinder::new(jr);

    let output_dir = Path::new(output_path);

    file_utils::ensure_directory_exists(output_dir)
        .map_err(|e| Error::Custom(format!("Failed to ensure directory exists: {e}")))?;
    file_utils::clear_directory(output_dir)
        .map_err(|e| Error::Custom(format!("Failed to clear directory: {e}")))?;

    // Create output directories
    file_utils::create_output_directories(output_dir)
        .map_err(|e| Error::Custom(format!("Failed to create output directories: {e}")))?;

    // Initialize dependency graph
    let mut dep_graph = DependencyGraph::new();

    // Process components first
    println!("Processing components...");
    process_components(firefox_root, component_paths, &mut dep_graph)?;

    // Process global stylesheets
    println!("Processing global stylesheets...");
    process_global_stylesheets(firefox_root, global_stylesheets, &mut dep_graph)?;

    // Process all dependencies recursively
    println!("Processing dependencies...");
    process_dependencies(&mut dep_graph, &pf)?;
    dep_graph.debug_print();

    // Transform and write all files
    println!("Transforming and writing files...");
    transform_and_write_files(&mut dep_graph, &output_dir)?;

    // println!("Library transformation completed successfully!");
    Ok(())
}

fn process_components(
    firefox_root: &Path,
    component_paths: &[&str],
    dep_graph: &mut DependencyGraph,
) -> Result<()> {
    for pattern in component_paths {
        let full_pattern = firefox_root.join(pattern.trim_start_matches('/'));
        let full_pattern_str = full_pattern.to_string_lossy();

        let files: Vec<PathBuf> = glob(&full_pattern_str)
            .map_err(Error::from)?
            .filter_map(|r| r.ok())
            .collect();

        for file_path in files {
            let file_name = file_path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            // Ignore .ts, .tsx, .css files
            if file_name.ends_with(".ts") || file_name.ends_with(".css") {
                continue;
            }

            let file_type =
                if file_name.ends_with(".stories.mjs") || file_name.ends_with(".story.mjs") {
                    FileType::JsFile
                } else if file_name.ends_with(".mjs") {
                    FileType::JsComponent
                } else {
                    FileType::OpaqueFile
                };

            // Get the name of the folder the file is directly in
            let component_name = file_path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|s| s.to_str())
                .ok_or_else(|| {
                    Error::Custom(format!(
                        "Failed to determine component folder for {:?}",
                        file_path
                    ))
                })?;

            dep_graph.add_file(
                file_path.clone(),
                file_type,
                TargetLocation::Component(component_name.to_string()),
            );
        }
    }
    Ok(())
}

fn process_global_stylesheets(
    firefox_root: &Path,
    stylesheet_paths: &[&str],
    dep_graph: &mut DependencyGraph,
) -> Result<()> {
    for pattern in stylesheet_paths {
        let full_pattern = firefox_root.join(pattern.trim_start_matches('/'));
        let full_pattern_str = full_pattern.to_string_lossy();

        let files: Vec<PathBuf> = glob(&full_pattern_str)
            .map_err(Error::from)?
            .filter_map(|r| r.ok())
            .collect();

        for file_path in files {
            dep_graph.add_file(
                file_path.clone(),
                FileType::CssFile,
                TargetLocation::CssGlobal,
            );
        }
    }
    Ok(())
}

fn process_dependencies(dep_graph: &mut DependencyGraph, path_finder: &PathFinder) -> Result<()> {
    let mut processed: HashSet<PathBuf> = HashSet::new();
    let mut to_process: Vec<dependency_graph::FileNode> = dep_graph.all_files().cloned().collect();

    while let Some(file) = to_process.pop() {
        if !processed.insert(file.path.clone()) {
            // Already processed this file, skip to avoid cycles
            continue;
        }

        let deps = match file.file_type {
            FileType::JsComponent | FileType::JsFile => {
                dependencies::js::dependencies_from_file(&file.path).map_err(|e| {
                    Error::Custom(format!(
                        "Failed to parse JS dependencies for {:?}: {}",
                        file.path, e
                    ))
                })?
            }
            FileType::CssFile => {
                dependencies::css::dependencies_from_file(&file.path).map_err(|e| {
                    Error::Custom(format!(
                        "Failed to parse CSS dependencies for {:?}: {}",
                        file.path, e
                    ))
                })?
            }
            _ => vec![],
        };

        // debug print for css files
        if file.file_type == FileType::CssFile {
            println!("Processing CSS file: {:?} - {:#?}", file.path, deps);
        }

        for dep in deps {
            println!("Processing dependency: {}", dep);
            // Resolve the dependency path
            let resolved_path = match path_finder.get_path(&file.path, &dep) {
                Ok(p) => p,
                Err(e) => {
                    println!(
                        "Failed to resolve path for dependency '{}': {:?}",
                        &file.path.display(),
                        e
                    );
                    continue;
                }
            };

            // Determine file type and target location
            let dep_file_type = match Path::new(&dep).extension().and_then(|s| s.to_str()) {
                Some("css") => FileType::CssFile,
                Some("js") | Some("mjs") => FileType::JsFile,
                _ => FileType::OpaqueFile,
            };

            let dep_target_location = match (
                &file.file_type,
                Path::new(&dep).extension().and_then(|s| s.to_str()),
            ) {
                (FileType::JsComponent, Some("css")) => TargetLocation::Omit,
                (_, Some("png") | Some("jpg") | Some("jpeg") | Some("svg")) => {
                    TargetLocation::Asset
                }
                _ => TargetLocation::Dependency,
            };

            println!(
                "Resolved dependency: {} -> {:?} (type: {:?}, target: {:?})",
                dep, resolved_path, dep_file_type, dep_target_location
            );

            // Add file to dependency graph; if it is new, push to to_process
            dep_graph.add_file(resolved_path.clone(), dep_file_type, dep_target_location);
            dep_graph
                .add_dependency(&file.path, &resolved_path, &dep)
                .map_err(|e| Error::Custom(format!("Failed to add dependency: {e}")))?;

            // Only process if not already processed and not already queued
            if !processed.contains(&resolved_path)
                && !to_process.iter().any(|f| f.path == resolved_path)
            {
                if let Some(node) = dep_graph.get_file(&resolved_path) {
                    to_process.push(node.clone());
                }
            }
        }
    }

    Ok(())
}

fn transform_and_write_files(dep_graph: &mut DependencyGraph, output_dir: &Path) -> Result<()> {
    // get an iterator over all files in the dependency graph
    let files = dep_graph
        .all_files()
        .filter(|f| f.target_location != TargetLocation::Omit);

    for file in files {
        // Perform transformation and writing logic here

        let output_path = match file.get_dist_path() {
            Some(path) => output_dir.join(path),
            None => {
                println!("Skipping file with no output path: {:?}", file.path);
                continue;
            }
        };

        // Ensure the parent directory exists before writing/copying
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                Error::Custom(format!("Failed to create directory: {:?}: {e}", parent))
            })?;
        }

        match file.file_type {
            FileType::JsComponent | FileType::JsFile => {
                let relative_imports = dep_graph.get_import_replacements(&file.path).unwrap();

                // if FileType::JsComponent, call dep_graph.get_omitted_imports(&file.path) and pass it as css_replacements, oterwise None
                let css_replacements = if file.file_type == FileType::JsComponent {
                    let omitted_imports = dep_graph.get_omitted_imports(&file.path);
                    // omitted imports is a Vec<(String, PathBuf)> of css files. We load the files, trnsform them like any other css file,
                    // and then return a HashMap<String, String> where the key is the original path and the value is the transformed CSS code.
                    let mut css_replacements = HashMap::new();
                    for (original_path, css_path) in omitted_imports {
                        let r_i = dep_graph
                            .get_dependencies_and_relative_paths(&css_path, &file.path)
                            .unwrap();
                        let css_code = transform::css::transform_from_file(&css_path, &r_i)
                            .map_err(|e| {
                                Error::Custom(format!(
                                    "Failed to transform CSS file: {:?}: {}",
                                    css_path, e
                                ))
                            })?;
                        css_replacements.insert(original_path, css_code);
                    }
                    Some(css_replacements)
                } else {
                    None
                };

                let transformed_code = transform::js::transform_from_file(
                    &file.path,
                    &relative_imports,
                    css_replacements.as_ref(),
                )
                .map_err(|e| {
                    Error::Custom(format!(
                        "Failed to transform JS file: {:?}: {}",
                        file.path, e
                    ))
                })?;
                std::fs::write(&output_path, transformed_code).map_err(|e| {
                    Error::Custom(format!("Failed to write JS file: {:?}: {e}", file.path))
                })?;
            }
            FileType::CssFile => {
                let relative_imports = dep_graph.get_import_replacements(&file.path).unwrap();
                let transformed_code =
                    transform::css::transform_from_file(&file.path, &relative_imports).map_err(
                        |e| {
                            Error::Custom(format!(
                                "Failed to transform CSS file: {:?}: {e}",
                                file.path
                            ))
                        },
                    )?;
                std::fs::write(&output_path, transformed_code).map_err(|e| {
                    Error::Custom(format!("Failed to write CSS file: {:?}: {e}", file.path))
                })?;
            }
            _ => {
                // other files are copied as is
                std::fs::copy(&file.path, &output_path).map_err(|e| {
                    Error::Custom(format!("Failed to copy file: {:?}: {e}", file.path))
                })?;
            }
        }
    }

    Ok(())
}
