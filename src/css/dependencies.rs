use lightningcss::{
    dependencies::{Dependency, ImportDependency, UrlDependency},
    properties::Property,
    rules::CssRule,
    stylesheet::{ParserOptions, StyleSheet},
    values::{
        image::{self, Image},
        url::Url,
    },
    visitor::{Visit, VisitTypes, Visitor},
};
use std::fs;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

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
        println!("Found URL: {}", url_str);
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

pub fn parse_css_dependencies(source_path: &PathBuf) -> Result<Vec<String>> {
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
    .unwrap();

    // Create visitors to collect dependencies
    let mut url_visitor = UrlVisitor::new();
    let mut rule_visitor = RuleVisitor::new();

    // Visit the stylesheet to collect URL dependencies
    stylesheet
        .visit(&mut url_visitor)
        .map_err(|_| "URL visiting failed")?;

    // Visit the stylesheet to collect rule dependencies
    stylesheet
        .visit(&mut rule_visitor)
        .map_err(|_| "Rule visiting failed")?;

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
