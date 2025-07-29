use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub globals_stylesheets: Vec<String>,
    pub jar_paths: Vec<String>,
    pub mozbuild_paths: Vec<String>,
    pub component_paths: Vec<String>,
}
