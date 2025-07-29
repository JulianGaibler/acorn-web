use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::{Directed, Direction, Graph};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FileType {
    JsComponent,
    JsFile,
    CssFile,
    OpaqueFile,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TargetLocation {
    Component(String),
    CssGlobal,
    Asset,
    Dependency,
    Omit,
}

#[derive(Debug, Clone)]
pub struct FileNode {
    pub path: PathBuf,
    pub file_type: FileType,
    pub target_location: TargetLocation,
}

#[derive(Debug, Clone)]
pub struct ImportEdge {
    pub import_statement: String,
}

pub struct DependencyGraph {
    graph: Graph<FileNode, ImportEdge, Directed>,
    path_to_index: HashMap<PathBuf, NodeIndex>,
}

impl FileNode {
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
    /// Create a new empty dependency graph
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

    /// Add a dependency between two files. Both files must already exist in the graph.
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

        // Check omit->dependency condition
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

    /// Add a file that depends on an existing file in one operation.
    /// This is a convenience method for the common case of discovering dependencies.
    pub fn add_dependent_file(
        &mut self,
        dependent_path: PathBuf,
        dependent_file_type: FileType,
        dependent_target_location: TargetLocation,
        depends_on: &PathBuf,
        import_statement: &str,
    ) -> Result<(NodeIndex, EdgeIndex), DependencyGraphError> {
        // Add the dependent file first
        let dependent_idx = self.add_file(
            dependent_path.clone(),
            dependent_file_type,
            dependent_target_location,
        );

        // Add the dependency
        let edge_idx = self.add_dependency(&dependent_path, depends_on, import_statement)?;

        Ok((dependent_idx, edge_idx))
    }

    /// Get a file node by its path
    pub fn get_file(&self, file_path: &PathBuf) -> Option<&FileNode> {
        self.path_to_index
            .get(file_path)
            .map(|&idx| &self.graph[idx])
    }

    /// Check if a file exists in the graph
    pub fn contains_file(&self, file_path: &PathBuf) -> bool {
        self.path_to_index.contains_key(file_path)
    }

    /// Get all files in the graph
    pub fn all_files(&self) -> impl Iterator<Item = &FileNode> {
        self.graph.node_weights()
    }

    /// Check if the graph has any circular dependencies
    pub fn has_cycles(&self) -> bool {
        petgraph::algo::is_cyclic_directed(&self.graph)
    }

    /// Get a topological ordering of the files (useful for processing order)
    /// Returns an error if the graph contains cycles
    pub fn topological_sort(&self) -> Result<Vec<&FileNode>, DependencyGraphError> {
        petgraph::algo::toposort(&self.graph, None)
            .map(|indices| indices.iter().map(|&idx| &self.graph[idx]).collect())
            .map_err(|_| DependencyGraphError::CircularDependency)
    }

    /// Get the number of files in the graph
    pub fn file_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Get the number of dependencies in the graph
    pub fn dependency_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Update the import statement for a specific dependency
    pub fn update_import_statement(
        &mut self,
        from_file: &PathBuf,
        to_file: &PathBuf,
        new_import_statement: String,
    ) -> Result<(), DependencyGraphError> {
        let from_idx = self
            .path_to_index
            .get(from_file)
            .ok_or_else(|| DependencyGraphError::SourceFileNotFound(from_file.clone()))?;

        let to_idx = self
            .path_to_index
            .get(to_file)
            .ok_or_else(|| DependencyGraphError::TargetFileNotFound(to_file.clone()))?;

        // Find the edge between these two nodes
        if let Some(edge_idx) = self.graph.find_edge(*from_idx, *to_idx) {
            if let Some(edge_weight) = self.graph.edge_weight_mut(edge_idx) {
                edge_weight.import_statement = new_import_statement;
                Ok(())
            } else {
                Err(DependencyGraphError::EdgeNotFound(
                    from_file.clone(),
                    to_file.clone(),
                ))
            }
        } else {
            Err(DependencyGraphError::EdgeNotFound(
                from_file.clone(),
                to_file.clone(),
            ))
        }
    }

    /// Remove a file and all its dependencies from the graph
    pub fn remove_file(&mut self, file_path: &PathBuf) -> Result<FileNode, DependencyGraphError> {
        let node_idx = self
            .path_to_index
            .remove(file_path)
            .ok_or_else(|| DependencyGraphError::FileNotFound(file_path.clone()))?;

        let node = self
            .graph
            .remove_node(node_idx)
            .ok_or_else(|| DependencyGraphError::FileNotFound(file_path.clone()))?;

        Ok(node)
    }

    /// Print a debug representation of the entire dependency graph
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

    /// Print files grouped by their target location
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

    /// Print the dependency tree showing relationships
    fn debug_print_dependency_tree(&self) {
        println!("🌳 Dependency Tree:");

        // Find root nodes (files with no incoming dependencies)
        let mut root_nodes = Vec::new();
        for (node_idx, file) in self.all_files_with_index() {
            let incoming_edges: Vec<_> = self
                .graph
                .edges_directed(node_idx, Direction::Incoming)
                .collect();
            if incoming_edges.is_empty() {
                root_nodes.push((node_idx, file));
            }
        }

        if root_nodes.is_empty() {
            println!(
                "  ⚠️  No root nodes found (all files have dependencies - possible circular deps)"
            );
            return;
        }

        // Print each root and its dependencies
        for (root_idx, root_file) in root_nodes {
            self.debug_print_node_tree(
                root_idx,
                root_file,
                0,
                &mut std::collections::HashSet::new(),
            );
        }
    }

    /// Recursively print a node and its dependencies with indentation
    fn debug_print_node_tree(
        &self,
        node_idx: NodeIndex,
        file: &FileNode,
        depth: usize,
        visited: &mut std::collections::HashSet<NodeIndex>,
    ) {
        let indent = "  ".repeat(depth);
        let file_type_icon = match file.file_type {
            FileType::JsComponent => "🧩",
            FileType::JsFile => "📜",
            FileType::CssFile => "🎨",
            FileType::OpaqueFile => "📄",
        };

        let cycle_marker = if visited.contains(&node_idx) {
            " 🔄"
        } else {
            ""
        };

        println!(
            "{}└─ {} {}{}",
            indent,
            file_type_icon,
            file.path.display(),
            cycle_marker
        );

        // Avoid infinite recursion in case of cycles
        if visited.contains(&node_idx) {
            return;
        }
        visited.insert(node_idx);

        // Print dependencies
        let mut dependencies: Vec<_> = self
            .graph
            .edges_directed(node_idx, Direction::Outgoing)
            .collect();

        // Sort dependencies by path for consistent output
        dependencies.sort_by(|a, b| {
            let a_node = &self.graph[a.target()];
            let b_node = &self.graph[b.target()];
            a_node.path.cmp(&b_node.path)
        });

        for (i, edge) in dependencies.iter().enumerate() {
            let target_node = &self.graph[edge.target()];
            let import_stmt = &edge.weight().import_statement;
            let is_last = i == dependencies.len() - 1;

            println!("{}  └─ 📎 \"{}\"", indent, import_stmt);
            self.debug_print_node_tree(edge.target(), target_node, depth + 2, visited);
        }

        visited.remove(&node_idx);
    }

    /// Print a compact summary of the graph
    pub fn debug_print_summary(&self) {
        println!("📊 Graph Summary:");
        println!(
            "  Files: {}, Dependencies: {}",
            self.file_count(),
            self.dependency_count()
        );

        let mut type_counts = std::collections::HashMap::new();
        let mut target_counts = std::collections::HashMap::new();

        for file in self.all_files() {
            *type_counts.entry(&file.file_type).or_insert(0) += 1;
            *target_counts.entry(&file.target_location).or_insert(0) += 1;
        }

        println!("  File Types:");
        for (file_type, count) in type_counts {
            let icon = match file_type {
                FileType::JsComponent => "🧩",
                FileType::JsFile => "📜",
                FileType::CssFile => "🎨",
                FileType::OpaqueFile => "📄",
            };
            println!("    {} {:?}: {}", icon, file_type, count);
        }

        println!("  Target Locations:");
        for (target, count) in target_counts {
            println!("    {:?}: {}", target, count);
        }

        if self.has_cycles() {
            println!("  ⚠️  Circular dependencies detected!");
        } else {
            println!("  ✅ No circular dependencies");
        }
    }

    fn all_files_with_index(&self) -> Vec<(NodeIndex, &FileNode)> {
        self.graph
            .node_indices()
            .map(|idx| (idx, &self.graph[idx]))
            .collect()
    }

    /// Get all outgoing dependencies from a file
    /// Returns a vector of (target_file_path, import_statement) tuples
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

    /// Get the FileNode for a file by path (returns owned data for easier manipulation)
    pub fn get_file_node(&self, file_path: &PathBuf) -> Option<FileNode> {
        self.path_to_index
            .get(file_path)
            .map(|&idx| self.graph[idx].clone())
    }

    /// Helper function to get dependencies and compute relative paths
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
                let relative_path = compute_relative_path(&current_dist_path, &target_dist_path);
                replacements.insert(original_import, relative_path);
            }
        }

        Ok(replacements)
    }

    /// Get all import statements that need to be replaced for a specific file
    /// Returns a vector of (original_import_statement, new_relative_path) tuples
    pub fn get_import_replacements(
        &self,
        file_path: &PathBuf,
    ) -> Result<HashMap<String, String>, DependencyGraphError> {
        self.get_dependencies_and_relative_paths(file_path, file_path)
    }

    pub(crate) fn get_omitted_imports(&self, path: &PathBuf) -> Vec<(String, PathBuf)> {
        let node_idx = match self.path_to_index.get(path) {
            Some(&idx) => idx,
            None => return vec![], // Return empty if the file is not found
        };

        self.graph
            .edges_directed(node_idx, Direction::Outgoing)
            .filter_map(|edge| {
                let target_node = &self.graph[edge.target()];
                if matches!(target_node.target_location, TargetLocation::Omit) {
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

#[derive(Debug, thiserror::Error)]
pub enum DependencyGraphError {
    #[error("File not found in graph: {0}")]
    FileNotFound(PathBuf),
    #[error("Dependency edge not found between '{0}' and '{1}'")]
    EdgeNotFound(PathBuf, PathBuf),
    #[error("Circular dependency detected in graph")]
    CircularDependency,
    #[error("Cannot add dependency: source file '{0}' not found")]
    SourceFileNotFound(PathBuf),
    #[error("Cannot add dependency: target file '{0}' not found")]
    TargetFileNotFound(PathBuf),
}

/// Compute the relative path from one file to another
/// Both paths should be the dist paths (where files will be located)
fn compute_relative_path(from_path: &Path, to_path: &Path) -> String {
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

/// Replace the path in an import statement with a new path
/// This handles common JavaScript import patterns:
/// - import { foo } from './old/path'
/// - import foo from './old/path'
/// - import './old/path'
/// - require('./old/path')
fn replace_import_path(original_import: &str, new_path: &str) -> String {
    use regex::Regex;

    // Pattern to match import/require statements and capture the path
    let patterns = [
        // import ... from 'path' or import ... from "path"
        r#"(import\s+.*?\s+from\s+)(['"])(.*?)(['"])"#,
        // import 'path' or import "path"
        r#"(import\s+)(['"])(.*?)(['"])"#,
        // require('path') or require("path")
        r#"(require\s*\(\s*)(['"])(.*?)(['"])\s*\)"#,
    ];

    for pattern in &patterns {
        if let Ok(re) = Regex::new(pattern) {
            if let Some(captures) = re.captures(original_import) {
                // Preserve the quote style (single or double quotes)
                let quote_char = captures.get(2).unwrap().as_str();
                return re
                    .replace(original_import, |caps: &regex::Captures| {
                        format!(
                            "{}{}{}{}",
                            caps.get(1).unwrap().as_str(),
                            quote_char,
                            new_path,
                            quote_char
                        )
                    })
                    .to_string();
            }
        }
    }

    // If no pattern matched, return the original import
    // This shouldn't happen if your import statements are well-formed
    original_import.to_string()
}
