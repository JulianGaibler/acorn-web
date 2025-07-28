use anyhow::{Context, Result};
use std::path::PathBuf;

use oxc::{
    allocator::Allocator,
    ast::ast::{
        Argument, CallExpression, ExportAllDeclaration, ExportNamedDeclaration, Expression, ImportDeclaration, StringLiteral, TaggedTemplateExpression, TemplateElement, TemplateLiteral
    },
    ast_visit::Visit,
    parser::{Parser, ParserReturn},
    span::SourceType,
};

pub fn parse_js_dependencies(
    source_path: &PathBuf,
) -> Result<Vec<String>> {
    // load the file content
    let source_text = std::fs::read_to_string(source_path)
        .with_context(|| format!("Failed to read file: {:?}", source_path))?;

    // Memory arena where AST nodes are allocated.
    let allocator = Allocator::default();
    // Infer source type (TS/JS/ESM/JSX/etc) based on file extension
    let source_type = SourceType::from_path(source_path).unwrap();
    let mut errors = Vec::new();

    let ParserReturn {
        program,
        errors: parser_errors,
        panicked,
        ..
    } = Parser::new(&allocator, &source_text, source_type).parse();
    errors.extend(parser_errors);

    if panicked {
        for error in &errors {
            println!("{error:?}");
            panic!("Parsing failed.");
        }
    }


    let mut visitor = DependencyVisitor::new();
    visitor.visit_program(&program);

    let dependencies: Vec<String> = visitor
        .dependencies
        .into_iter()
        .filter(|dep| !dep.is_empty())
        .collect();

    if source_path.ends_with("moz-breadcrumb-group.mjs") {
        println!("Dependencies found in {}: {:?}", source_path.display(), dependencies);
    }
    
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

    fn process_template_literal(&mut self, template: &TemplateLiteral) {
        // Reconstruct the template literal content
        let mut html_content = String::new();

        for (i, quasi) in template.quasis.iter().enumerate() {
            html_content.push_str(&quasi.value.raw);

            // Add placeholder for expressions (we can't evaluate them, so use placeholder)
            if i < template.expressions.len() {
                html_content.push_str("${...}");
            }
        }

        // Extract CSS links using the updated regex
        self.extract_css_links_from_html(&html_content);
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
    }
}