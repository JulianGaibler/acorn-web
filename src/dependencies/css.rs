use lightningcss::{
    rules::CssRule,
    stylesheet::{ParserOptions, StyleSheet},
    values::url::Url,
    visitor::{Visit, VisitTypes, Visitor},
};
use std::fs;
use std::path::PathBuf;

use crate::errors::{DependencyError, DependencyResult};

pub fn dependencies_from_file(source_path: &PathBuf) -> DependencyResult<Vec<String>> {
    let css_content = fs::read_to_string(source_path)?;
    dependencies_from_string(&css_content)
}
pub fn dependencies_from_string(css_content: &String) -> DependencyResult<Vec<String>> {
    // Parse the CSS using StyleSheet::parse
    let mut stylesheet = StyleSheet::parse(
        &css_content,
        ParserOptions {
            ..Default::default()
        },
    )
    .map_err(|e| DependencyError::CssParse {
        message: format!("{:?}", e),
    })?;

    // Create visitors to collect dependencies
    let mut url_visitor = UrlVisitor::new();
    let mut rule_visitor = RuleVisitor::new();

    // Visit the stylesheet to collect URL dependencies
    stylesheet
        .visit(&mut url_visitor)
        .map_err(|_| DependencyError::Extract {
            message: "URL visiting failed".to_string(),
        })?;

    // Visit the stylesheet to collect rule dependencies
    stylesheet
        .visit(&mut rule_visitor)
        .map_err(|_| DependencyError::Extract {
            message: "Rule visiting failed".to_string(),
        })?;

    // Combine and return all dependencies
    let mut dependencies: Vec<String> = url_visitor
        .dependencies
        .into_iter()
        .filter(|dep| !dep.is_empty())
        .collect();

    dependencies.extend(
        rule_visitor
            .dependencies
            .into_iter()
            .filter(|dep| !dep.is_empty()),
    );

    Ok(dependencies)
}

struct UrlVisitor {
    dependencies: Vec<String>,
}

impl UrlVisitor {
    fn new() -> Self {
        Self {
            dependencies: Vec::new(),
        }
    }

    fn add_dependency(&mut self, url: &str) {
        // Skip data URLs, HTTP(S) URLs, and other non-file protocols

        if url.starts_with("data:")
            || url.starts_with("http://")
            || url.starts_with("https://")
            || url.starts_with("//")
        {
            return;
        }

        // Remove URL fragments and query parameters
        let clean_url = url.split(['?', '#']).next().unwrap_or(url).to_string();

        if !self.dependencies.contains(&clean_url) {
            self.dependencies.push(clean_url);
        }
    }
}

impl<'i> Visitor<'i> for UrlVisitor {
    type Error = ();

    fn visit_url(&mut self, url: &mut Url<'i>) -> std::result::Result<(), ()> {
        let url_str = url.url.to_string();
        self.add_dependency(&url_str);
        Ok(())
    }

    fn visit_types(&self) -> VisitTypes {
        lightningcss::visit_types!(URLS)
    }
}

struct RuleVisitor {
    dependencies: Vec<String>,
}

impl RuleVisitor {
    fn new() -> Self {
        Self {
            dependencies: Vec::new(),
        }
    }

    fn add_dependency(&mut self, url: &str) {
        // Skip data URLs, HTTP(S) URLs, and other non-file protocols

        if url.starts_with("data:")
            || url.starts_with("http://")
            || url.starts_with("https://")
            || url.starts_with("//")
        {
            return;
        }

        // Remove URL fragments and query parameters
        let clean_url = url.split(['?', '#']).next().unwrap_or(url).to_string();

        if !self.dependencies.contains(&clean_url) {
            self.dependencies.push(clean_url);
        }
    }
}

impl<'i> Visitor<'i> for RuleVisitor {
    type Error = ();

    fn visit_rule(&mut self, rule: &mut CssRule<'i>) -> std::result::Result<(), ()> {
        if let CssRule::Import(import_rule) = rule {
            let url_str = import_rule.url.to_string();
            self.add_dependency(&url_str);
        }
        Ok(())
    }

    fn visit_types(&self) -> VisitTypes {
        lightningcss::visit_types!(RULES)
    }
}
