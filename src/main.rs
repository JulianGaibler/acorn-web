mod config;

use std::fs;

use anyhow::{Context, Result};
use clap::Parser;

use config::Config;
use mozcomp::transform_lib;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the Firefox root directory
    firefox_root: String,

    /// Path to the output directory
    #[arg(default_value = "output")]
    output: String,

    /// Path to the configuration file
    #[arg(default_value = "mozcomp.toml")]
    config: String,
}



fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Read and parse the config file
    let config_str = fs::read_to_string(&args.config)
        .with_context(|| format!("Failed to read config file: {:?}", &args.config))?;

    let config: Config = toml::from_str(&config_str)
        .with_context(|| "Failed to parse config file")?;

    // Call the transform_lib function with the parsed configuration
    transform_lib(
        std::path::Path::new(&args.firefox_root),
        &args.output,
        &config.jar_paths.iter().map(String::as_str).collect::<Vec<_>>(),
        &config.mozbuild_paths.iter().map(String::as_str).collect::<Vec<_>>(),
        &config.globals_stylesheets.iter().map(String::as_str).collect::<Vec<_>>(),
        &config.component_paths.iter().map(String::as_str).collect::<Vec<_>>()
    )
    .with_context(|| "Failed to transform library")?;
    Ok(())
}