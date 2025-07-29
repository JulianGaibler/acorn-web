use lightningcss::{
    printer::PrinterOptions,
    stylesheet::{ParserOptions, StyleSheet},
};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::{
    errors::{TransformError, TransformResult},
    transform::css_transform::{ImportReplacer, UrlReplacer},
};

pub fn transform_from_file(
    source_path: &PathBuf,
    url_replacements: &HashMap<String, String>,
) -> TransformResult<String> {
    let css_content = fs::read_to_string(source_path)?;
    transform_from_string(&css_content, url_replacements)
}

pub fn transform_from_string(
    css_content: &str,
    url_replacements: &HashMap<String, String>,
) -> TransformResult<String> {
    // Parse the CSS using StyleSheet::parse

    let mut stylesheet = StyleSheet::parse(
        &css_content,
        ParserOptions {
            ..Default::default()
        },
    )
    .map_err(|e| TransformError::CssParse {
        message: format!("{:?}", e),
    })?;

    // Use UrlReplacer to mutate the stylesheet in place
    UrlReplacer::new(url_replacements).build(&mut stylesheet)?;
    ImportReplacer::new(url_replacements).build(&mut stylesheet)?;

    // Serialize the transformed stylesheet back to CSS
    let result =
        stylesheet
            .to_css(PrinterOptions::default())
            .map_err(|e| TransformError::CssSerialize {
                message: format!("{:?}", e),
            })?;

    Ok(result.code)
}
