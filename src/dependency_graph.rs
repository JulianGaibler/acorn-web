use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::{Directed, Direction, Graph};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::utils::file_utils;

/// Represents the type of a file in the dependency graph.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FileType {
    /// A JavaScript with a moz-component
    JsComponent,
    /// A regular JavaScript file
    JsFile,
    /// A CSS file
    CssFile,
    /// Any other file type that we just copy over without processing
    OpaqueFile,
}

/// Represents where a file should be placed in the output distribution.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TargetLocation {
    /// for the specific component foldeer
    Component(String),
    /// Global CSS folder
    CssGlobal,
    /// Asset file folder
    Asset,
    /// Dependency file folder
    Dependency,
    /// Omitted from output
    Omit,
}

/// Node representing a file in the dependency graph.
#[derive(Debug, Clone)]
pub struct FileNode {
    /// Path to the file
    pub path: PathBuf,
    /// Type of the file
    pub file_type: FileType,
    /// Where the file should be placed in the output
    pub target_location: TargetLocation,
}

/// Edge representing an import relationship between files.
#[derive(Debug, Clone)]
pub struct ImportEdge {
    /// The import statement string
    pub import_statement: String,
}

/// The main dependency graph structure, holding files and their relationships.
pub struct DependencyGraph {
    /// The underlying petgraph graph
    graph: Graph<FileNode, ImportEdge, Directed>,
    /// Maps file paths to node indices for quick lookup
    path_to_index: HashMap<PathBuf, NodeIndex>,
}

impl FileNode {
    /// Get the output (dist) path for this file, or None if omitted.
    pub fn get_dist_path(&self) -> Option<PathBuf> {
        // early return if the target location is Omit
        if matches!(self.target_location, TargetLocation::Omit) {
            return None;
        }
        let filename = match self.path.file_name() {
            Some(name) => name.to_string_lossy().to_owned(),
            None => return None,
        };

        match &self.target_location {
            TargetLocation::Component(name) => {
                Some(PathBuf::from(format!("components/{}/{}", name, filename)))
            }
            TargetLocation::CssGlobal => Some(PathBuf::from(format!("styles/{}", filename))),
            TargetLocation::Asset => Some(PathBuf::from(format!("assets/{}", filename))),
            TargetLocation::Dependency => Some(PathBuf::from(format!("dependencies/{}", filename))),
            TargetLocation::Omit => None,
        }
    }
}

impl DependencyGraph {
    /// Create a new empty dependency graph.
    pub fn new() -> Self {
        Self {
            graph: Graph::new(),
            path_to_index: HashMap::new(),
        }
    }

    /// Add a file to the graph. If the file already exists, keeps the original
    /// FileType and TargetLocation and returns the existing NodeIndex.
    /// Returns the NodeIndex for the file.
    pub fn add_file(
        &mut self,
        path: PathBuf,
        file_type: FileType,
        target_location: TargetLocation,
    ) -> NodeIndex {
        // Check if file already exists
        if let Some(&existing_index) = self.path_to_index.get(&path) {
            return existing_index;
        }

        // Create new node
        let node = FileNode {
            path: path.clone(),
            file_type,
            target_location,
        };

        let index = self.graph.add_node(node);
        self.path_to_index.insert(path, index);
        index
    }

    /// Add a dependency (import) between two files. Both files must already exist in the graph.
    /// Returns the EdgeIndex for the new dependency, or an error if either file doesn't exist.
    pub fn add_dependency(
        &mut self,
        from_file: &PathBuf,
        to_file: &PathBuf,
        import_statement: &str,
    ) -> Result<EdgeIndex, DependencyGraphError> {
        let from_idx = self
            .path_to_index
            .get(from_file)
            .ok_or_else(|| DependencyGraphError::SourceFileNotFound(from_file.clone()))?;

        let to_idx = self
            .path_to_index
            .get(to_file)
            .ok_or_else(|| DependencyGraphError::TargetFileNotFound(to_file.clone()))?;

        // If the target is Omit and the source is not a JsComponent, mark as Dependency
        let from_file_type = self.graph[*from_idx].file_type.clone();
        let to_target_location_is_omit =
            self.graph[*to_idx].target_location == TargetLocation::Omit;
        if from_file_type != FileType::JsComponent && to_target_location_is_omit {
            self.graph[*to_idx].target_location = TargetLocation::Dependency;
        }

        let edge = ImportEdge {
            import_statement: import_statement.to_string(),
        };

        Ok(self.graph.add_edge(*from_idx, *to_idx, edge))
    }

    /// Get a file node by its path, if it exists in the graph.
    pub fn get_file(&self, file_path: &PathBuf) -> Option<&FileNode> {
        self.path_to_index
            .get(file_path)
            .map(|&idx| &self.graph[idx])
    }

    /// Get an iterator over all files in the graph.
    pub fn all_files(&self) -> impl Iterator<Item = &FileNode> {
        self.graph.node_weights()
    }

    /// Check if the graph has any circular dependencies.
    pub fn has_cycles(&self) -> bool {
        petgraph::algo::is_cyclic_directed(&self.graph)
    }

    /// Get the number of files in the graph.
    pub fn file_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Get the number of dependencies (edges) in the graph.
    pub fn dependency_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Print a debug representation of the entire dependency graph to stdout.
    pub fn debug_print(&self) {
        println!("=== Dependency Graph Debug ===");
        println!(
            "Files: {}, Dependencies: {}",
            self.file_count(),
            self.dependency_count()
        );

        if self.has_cycles() {
            println!("⚠️  WARNING: Circular dependencies detected!");
        }

        println!();

        // Print all files grouped by target location
        self.debug_print_files_by_target();

        println!();

        // Print dependency tree
        self.debug_print_dependency_tree();

        println!("=== End Debug ===");
    }

    /// Print files grouped by their target location to stdout.
    fn debug_print_files_by_target(&self) {
        use std::collections::BTreeMap;

        let mut by_target: BTreeMap<String, Vec<&FileNode>> = BTreeMap::new();

        for file in self.all_files() {
            let target_key = match &file.target_location {
                TargetLocation::Component(name) => format!("Component({})", name),
                TargetLocation::CssGlobal => "CssGlobal".to_string(),
                TargetLocation::Asset => "Asset".to_string(),
                TargetLocation::Dependency => "Dependency".to_string(),
                TargetLocation::Omit => "Omit".to_string(),
            };
            by_target.entry(target_key).or_default().push(file);
        }

        println!("📁 Files by Target Location:");
        for (target, files) in by_target {
            println!("  {} ({} files):", target, files.len());
            for file in files {
                let file_type_icon = match file.file_type {
                    FileType::JsComponent => "🧩",
                    FileType::JsFile => "📜",
                    FileType::CssFile => "🎨",
                    FileType::OpaqueFile => "📄",
                };
                println!(
                    "    {} {} ({})",
                    file_type_icon,
                    file.path.display(),
                    format!("{:?}", file.file_type)
                );
            }
        }
    }

    /// Print the dependency tree showing relationships to stdout.
    fn debug_print_dependency_tree(&self) {
        println!("🌳 Files with dependencies:");

        // Print only files that have at least one dependency (outgoing edge)
        for (node_idx, file) in self.all_files_with_index() {
            // Get direct dependencies (outgoing edges)
            let mut dependencies: Vec<_> = self
                .graph
                .edges_directed(node_idx, Direction::Outgoing)
                .collect();
            if dependencies.is_empty() {
                continue;
            }

            let file_type_icon = match file.file_type {
                FileType::JsComponent => "🧩",
                FileType::JsFile => "📜",
                FileType::CssFile => "🎨",
                FileType::OpaqueFile => "📄",
            };
            println!("└─ {} {}", file_type_icon, file.path.display());

            dependencies.sort_by(|a, b| {
                let a_node = &self.graph[a.target()];
                let b_node = &self.graph[b.target()];
                a_node.path.cmp(&b_node.path)
            });

            for edge in dependencies {
                let target_node = &self.graph[edge.target()];
                let import_stmt = &edge.weight().import_statement;
                let target_icon = match target_node.file_type {
                    FileType::JsComponent => "🧩",
                    FileType::JsFile => "📜",
                    FileType::CssFile => "🎨",
                    FileType::OpaqueFile => "📄",
                };
                println!("    └─ 📎 \"{}\"", import_stmt);
                println!("      -> {} {}", target_icon, target_node.path.display());
            }
        }
    }

    /// Get all files in the graph, with their node indices.
    fn all_files_with_index(&self) -> Vec<(NodeIndex, &FileNode)> {
        self.graph
            .node_indices()
            .map(|idx| (idx, &self.graph[idx]))
            .collect()
    }

    /// Get all outgoing dependencies from a file.
    /// Returns a vector of (target_file_path, import_statement) tuples.
    pub fn get_file_dependencies(
        &self,
        file_path: &PathBuf,
    ) -> Result<Vec<(PathBuf, String)>, DependencyGraphError> {
        let node_idx = self
            .path_to_index
            .get(file_path)
            .ok_or_else(|| DependencyGraphError::FileNotFound(file_path.clone()))?;

        let dependencies = self
            .graph
            .edges_directed(*node_idx, Direction::Outgoing)
            .map(|edge| {
                let target_node = &self.graph[edge.target()];
                (
                    target_node.path.clone(),
                    edge.weight().import_statement.clone(),
                )
            })
            .collect();

        Ok(dependencies)
    }

    /// Helper function to get dependencies and compute relative paths for import replacement.
    pub fn get_dependencies_and_relative_paths(
        &self,
        query_path: &PathBuf,
        relative_from_path: &PathBuf,
    ) -> Result<HashMap<String, String>, DependencyGraphError> {
        let current_file = self
            .get_file(relative_from_path)
            .ok_or_else(|| DependencyGraphError::FileNotFound(relative_from_path.clone()))?;

        let current_dist_path = current_file
            .get_dist_path()
            .ok_or_else(|| DependencyGraphError::FileNotFound(relative_from_path.clone()))?;

        let dependencies = self.get_file_dependencies(query_path)?;
        let mut replacements = HashMap::new();

        for (target_path, original_import) in dependencies {
            let target_file = self
                .get_file(&target_path)
                .ok_or_else(|| DependencyGraphError::TargetFileNotFound(target_path.clone()))?;

            if let Some(target_dist_path) = target_file.get_dist_path() {
                let relative_path =
                    file_utils::compute_relative_path(&current_dist_path, &target_dist_path);
                replacements.insert(original_import, relative_path);
            }
        }

        Ok(replacements)
    }

    /// Get all import statements that need to be replaced for a specific file.
    /// Returns a map of (original_import_statement, new_relative_path).
    pub fn get_import_replacements(
        &self,
        file_path: &PathBuf,
    ) -> Result<HashMap<String, String>, DependencyGraphError> {
        self.get_dependencies_and_relative_paths(file_path, file_path)
    }

    /// Get all outgoing CSS file imports from a file.
    /// Returns a vector of (import_statement, css_file_path) tuples.
    pub(crate) fn get_css_imports(&self, path: &PathBuf) -> Vec<(String, PathBuf)> {
        let node_idx = match self.path_to_index.get(path) {
            Some(&idx) => idx,
            None => return vec![], // Return empty if the file is not found
        };

        self.graph
            .edges_directed(node_idx, Direction::Outgoing)
            .filter_map(|edge| {
                let target_node = &self.graph[edge.target()];
                if matches!(target_node.file_type, FileType::CssFile) {
                    Some((
                        edge.weight().import_statement.clone(),
                        target_node.path.clone(),
                    ))
                } else {
                    None
                }
            })
            .collect()
    }
}

/// Errors that can occur when working with the dependency graph.
#[derive(Debug, thiserror::Error)]
pub enum DependencyGraphError {
    /// File not found in the graph
    #[error("File not found in graph: {0}")]
    FileNotFound(PathBuf),
    /// Source file for dependency not found
    #[error("Cannot add dependency: source file '{0}' not found")]
    SourceFileNotFound(PathBuf),
    /// Target file for dependency not found
    #[error("Cannot add dependency: target file '{0}' not found")]
    TargetFileNotFound(PathBuf),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_add_file_and_get_file() {
        let mut graph = DependencyGraph::new();
        let path = PathBuf::from("src/foo.js");
        let idx = graph.add_file(path.clone(), FileType::JsFile, TargetLocation::Dependency);
        let node = graph.get_file(&path).unwrap();
        assert_eq!(node.path, path);
        assert_eq!(node.file_type, FileType::JsFile);
        assert_eq!(node.target_location, TargetLocation::Dependency);
        // Adding the same file again should return the same index
        let idx2 = graph.add_file(path.clone(), FileType::CssFile, TargetLocation::Asset);
        assert_eq!(idx, idx2);
    }

    #[test]
    fn test_add_dependency_and_cycle_detection() {
        let mut graph = DependencyGraph::new();
        let a = PathBuf::from("a.js");
        let b = PathBuf::from("b.js");
        graph.add_file(a.clone(), FileType::JsFile, TargetLocation::Dependency);
        graph.add_file(b.clone(), FileType::JsFile, TargetLocation::Dependency);
        let edge = graph.add_dependency(&a, &b, "./b.js");
        assert!(edge.is_ok());
        assert!(!graph.has_cycles());
        // Add a cycle
        let edge2 = graph.add_dependency(&b, &a, "./a.js");
        assert!(edge2.is_ok());
        assert!(graph.has_cycles());
    }

    #[test]
    fn test_get_dist_path_omit() {
        let node = FileNode {
            path: PathBuf::from("foo.js"),
            file_type: FileType::JsFile,
            target_location: TargetLocation::Omit,
        };
        assert_eq!(node.get_dist_path(), None);
    }

    #[test]
    fn test_get_import_replacements_empty() {
        let mut graph = DependencyGraph::new();
        let path = PathBuf::from("foo.js");
        graph.add_file(path.clone(), FileType::JsFile, TargetLocation::Dependency);
        let replacements = graph.get_import_replacements(&path).unwrap();
        assert!(replacements.is_empty());
    }
}
