use lightningcss::{
    printer::PrinterOptions,
    stylesheet::{ParserOptions, StyleSheet},
    values::url::Url,
    visitor::{Visit, VisitTypes, Visitor},
};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CssTransformError {
    #[error("Failed to read CSS file: {0}")]
    FileRead(#[from] std::io::Error),
    #[error("Failed to parse CSS: {0}")]
    Parse(String),
    #[error("URL '{url}' not found in replacement map")]
    UrlNotFound { url: String },
    #[error("Failed to transform CSS: {0}")]
    Transform(String),
    #[error("Failed to serialize CSS: {0}")]
    Serialize(String),
}

type Result<T> = std::result::Result<T, CssTransformError>;

pub fn transform_css_urls(
    source_path: &PathBuf,
    url_replacements: HashMap<String, String>,
) -> Result<String> {
    // Read the CSS file
    let css_content = fs::read_to_string(source_path)?;

    // Parse the CSS using StyleSheet::parse
    let mut stylesheet = StyleSheet::parse(
        &css_content,
        ParserOptions {
            filename: source_path.to_string_lossy().to_string(),
            ..Default::default()
        },
    )
    .map_err(|e| CssTransformError::Parse(format!("{:?}", e)))?;

    // Create a visitor to transform URLs
    let mut url_transformer = UrlTransformer::new(url_replacements);

    // Visit the stylesheet to transform URLs
    stylesheet
        .visit(&mut url_transformer)
        .map_err(|e| CssTransformError::Transform(format!("{:?}", e)))?;

    // Serialize the transformed stylesheet back to CSS
    let result = stylesheet
        .to_css(PrinterOptions::default())
        .map_err(|e| CssTransformError::Serialize(format!("{:?}", e)))?;

    Ok(result.code)
}

struct UrlTransformer {
    url_replacements: HashMap<String, String>,
}

impl UrlTransformer {
    fn new(url_replacements: HashMap<String, String>) -> Self {
        Self { url_replacements }
    }

    fn should_transform_url(&self, url: &str) -> bool {
        // Skip data URLs, HTTP(S) URLs, and other non-file protocols
        !url.starts_with("data:")
            && !url.starts_with("http://")
            && !url.starts_with("https://")
            && !url.starts_with("//")
    }

    fn clean_url(&self, url: &str) -> String {
        // Remove URL fragments and query parameters
        url.split(['?', '#']).next().unwrap_or(url).to_string()
    }
}

impl<'i> Visitor<'i> for UrlTransformer {
    type Error = CssTransformError;

    fn visit_url(&mut self, url: &mut Url<'i>) -> std::result::Result<(), Self::Error> {
        let url_str = url.url.to_string();

        if self.should_transform_url(&url_str) {
            let clean_url = self.clean_url(&url_str);

            if let Some(replacement) = self.url_replacements.get(&clean_url) {
                // Create a new URL with the replacement value
                url.url = replacement.clone().into();
            } else {
                return Err(CssTransformError::UrlNotFound { url: clean_url });
            }
        }

        Ok(())
    }

    fn visit_rule(
        &mut self,
        rule: &mut lightningcss::rules::CssRule<'i>,
    ) -> std::result::Result<(), Self::Error> {
        // process import rules
        if let lightningcss::rules::CssRule::Import(import_rule) = rule {
            match import_rule.url.as_ref() {
                url => {
                    let url_str = url.to_string();
                    if self.should_transform_url(&url_str) {
                        let clean_url = self.clean_url(&url_str);
                        if let Some(replacement) = self.url_replacements.get(&clean_url) {
                            import_rule.url = replacement.clone().into();
                        } else {
                            return Err(CssTransformError::UrlNotFound { url: clean_url });
                        }
                    }
                }
                _ => (),
            }
        }

        Ok(())
    }

    fn visit_types(&self) -> VisitTypes {
        lightningcss::visit_types!(URLS | RULES)
    }
}
