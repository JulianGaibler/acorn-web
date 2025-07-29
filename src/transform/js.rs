use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use oxc::allocator::Allocator;
use oxc::parser::{Parser, ParserReturn};
use oxc::semantic::{SemanticBuilder, SemanticBuilderReturn};
use oxc::span::SourceType;
use oxc_codegen::Codegen;
use oxc_traverse::ReusableTraverseCtx;

use crate::errors::{TransformError, TransformResult};
use crate::transform::js_transform::{
    CssInlineTransformer, IconTemplateImportTransformer, ImportCssTransformer, UrlTransformer,
};

pub fn transform_from_file(
    source_path: &PathBuf,
    url_replacements: &HashMap<String, String>,
    css_replacements: Option<&HashMap<String, String>>,
) -> TransformResult<String> {
    let source_code = fs::read_to_string(source_path)?;
    transform_from_string(&source_code, url_replacements, css_replacements)
}

pub fn transform_from_string(
    source_code: &str,
    url_replacements: &HashMap<String, String>,
    css_replacements: Option<&HashMap<String, String>>,
) -> TransformResult<String> {
    // Prepare allocator and parser
    let allocator = Allocator::default();
    let source_type = SourceType::default().with_module(true);
    let parser = Parser::new(&allocator, &source_code, source_type);
    let ParserReturn {
        mut program,
        errors: _parser_errors,
        panicked,
        ..
    } = parser.parse();

    if panicked {
        return Err(TransformError::JsPanicParse);
    }

    let SemanticBuilderReturn {
        semantic,
        errors: semantic_errors,
    } = SemanticBuilder::new()
        .with_check_syntax_error(true) // Enable extra syntax error checking
        .with_build_jsdoc(false) // Enable JSDoc parsing
        .with_cfg(false) // Build a Control Flow Graph
        .build(&program); // Produce the `Semantic`

    if !semantic_errors.is_empty() {
        let error_messages: Vec<String> =
            semantic_errors.iter().map(|e| format!("{:?}", e)).collect();
        return Err(TransformError::JsParse {
            message: format!("Semantic errors: {}", error_messages.join(", ")),
        });
    }
    let scoping = semantic.into_scoping();

    let mut ctx = ReusableTraverseCtx::new((), scoping, &allocator);

    // Traverse the AST to transform URLs
    if let Some(css_replacements) = css_replacements {
        let made_replacements =
            CssInlineTransformer::new(css_replacements).build(&mut program, &mut ctx);
        if made_replacements {
            ImportCssTransformer::new().build(&mut program, &mut ctx);
        }
    }
    UrlTransformer::new(url_replacements).build(&mut program, &mut ctx);
    IconTemplateImportTransformer::new(url_replacements).build(&mut program, &mut ctx);
    // Codegen back to JavaScript string
    let codegen = Codegen::new();
    let output = codegen.build(&program);

    // replace tabs with 2 spaces
    let output = output.code.replace("\t", "  ");

    Ok(output)
}
