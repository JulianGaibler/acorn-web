use std::path::PathBuf;

use oxc::{
    allocator::Allocator,
    ast::ast::{ImportDeclaration, StringLiteral, TemplateElement},
    ast_visit::Visit,
    parser::{Parser, ParserReturn},
    span::SourceType,
};

use crate::errors::{DependencyError, DependencyResult};

pub fn dependencies_from_file(source_path: &PathBuf) -> DependencyResult<Vec<String>> {
    let source_text = std::fs::read_to_string(source_path)?;
    let source_type = SourceType::from_path(source_path).unwrap();
    dependencies_from_string(&source_text, source_type)
}

pub fn dependencies_from_string(
    source_text: &String,
    source_type: SourceType,
) -> DependencyResult<Vec<String>> {
    // Memory arena where AST nodes are allocated.
    let allocator = Allocator::default();

    let ParserReturn {
        program,
        errors: parser_errors,
        panicked,
        ..
    } = Parser::new(&allocator, source_text, source_type).parse();

    if panicked {
        return Err(DependencyError::JsPanicParse);
    }

    if !parser_errors.is_empty() {
        let error_messages: Vec<String> =
            parser_errors.iter().map(|e| format!("{:?}", e)).collect();
        return Err(DependencyError::JsParse {
            message: format!("Parser errors: {}", error_messages.join(", ")),
        });
    }

    let mut visitor = DependencyVisitor::new();
    visitor.visit_program(&program);

    let dependencies: Vec<String> = visitor
        .dependencies
        .into_iter()
        .filter(|dep| !dep.is_empty())
        .collect();

    Ok(dependencies)
}

struct DependencyVisitor {
    dependencies: Vec<String>,
}

impl DependencyVisitor {
    fn new() -> Self {
        Self {
            dependencies: Vec::new(),
        }
    }

    fn extract_string_literal(&mut self, literal: &StringLiteral) {
        self.dependencies.push(literal.value.to_string());
    }

    fn extract_css_links_from_html(&mut self, html_content: &str) {
        // Use the same regex pattern as in transform.rs
        let link_tag_regex = regex::Regex::new(
            r#"<link[^>]*rel\s*=\s*[\"']stylesheet[\"'][^>]*href\s*=\s*[\"']([^\"']+)[\"'][^>]*/?>"#
        ).unwrap();

        for captures in link_tag_regex.captures_iter(html_content) {
            if let Some(href_match) = captures.get(1) {
                let href = href_match.as_str().trim();
                if !href.is_empty() {
                    self.dependencies.push(href.to_string());
                }
            }
        }
    }

    fn extract_any_link_from_html(&mut self, html_content: &str) {
        let url_regex =
            regex::Regex::new(r#"(?:src|href|iconsrc)\s*=\s*[\"']([^\"']+\.[a-zA-Z0-9]+)[\"']"#)
                .unwrap();
        for captures in url_regex.captures_iter(html_content) {
            if let Some(url_match) = captures.get(1) {
                let url = url_match.as_str().trim();
                // Only allow relative paths or chrome:// or resource://
                if (url.starts_with("chrome://") || url.starts_with("resource://"))
                    || (!url.starts_with("http://")
                        && !url.starts_with("https://")
                        && !url.starts_with("www."))
                {
                    self.dependencies.push(url.to_string());
                }
            }
        }
    }
}

impl<'a> Visit<'a> for DependencyVisitor {
    fn visit_import_declaration(&mut self, decl: &ImportDeclaration<'a>) {
        self.extract_string_literal(&decl.source);
    }

    fn visit_template_element(&mut self, element: &TemplateElement<'a>) {
        // If the template element contains HTML, extract CSS links
        let value = &element.value;
        self.extract_css_links_from_html(&value.raw);
        self.extract_any_link_from_html(&value.raw);
    }

    fn visit_string_literal(&mut self, it: &StringLiteral<'a>) {
        if it.value.starts_with("chrome://") || it.value.starts_with("resource://") {
            self.extract_string_literal(it);
        }
    }
}
