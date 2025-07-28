use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use oxc::allocator::Allocator;
use oxc::ast::ast::{
    ClassElement, Expression, ImportDeclaration, PropertyKey, Statement, TaggedTemplateExpression,
    TemplateLiteral,
};
use oxc::parser::{Parser, ParserReturn};
use oxc::semantic::{SemanticBuilder, SemanticBuilderReturn};
use oxc::span::{SPAN, SourceType};
use oxc_codegen::Codegen;
use oxc_traverse::{ReusableTraverseCtx, Traverse, TraverseCtx};
use regex::Regex;

pub fn transform_js_urls(
    source_path: &PathBuf,
    url_replacements: HashMap<String, String>,
    css_replacements: Option<HashMap<String, String>>,
) -> Result<String, String> {
    // Step 1: Read source file
    let source_code =
        fs::read_to_string(source_path).map_err(|e| format!("Failed to read file: {}", e))?;

    // Step 2: Prepare allocator and parser
    let allocator = Allocator::default();
    let source_type = SourceType::default().with_module(true);
    let parser = Parser::new(&allocator, &source_code, source_type);
    let ParserReturn {
        mut program,
        errors: parser_errors,
        panicked,
        ..
    } = parser.parse();

    if panicked {
        for error in &parser_errors {
            println!("{error:?}");
            panic!("Parsing failed.");
        }
    }

    let SemanticBuilderReturn {
        semantic,
        errors: semantic_errors,
    } = SemanticBuilder::new()
        .with_check_syntax_error(true) // Enable extra syntax error checking
        .with_build_jsdoc(true) // Enable JSDoc parsing
        .with_cfg(true) // Build a Control Flow Graph
        .build(&program); // Produce the `Semantic`

    if !semantic_errors.is_empty() {
        for error in semantic_errors {
            eprintln!("{error:?}");
            panic!("Failed to build Semantic for Counter component.");
        }
    }
    let scoping = semantic.into_scoping();

    // if file ends with "moz-breadcrumb-group.mjs", print the AST
    // if source_path.ends_with("moz-breadcrumb-group.mjs") {
    //     println!("AST for file: {:?}", source_path);
    //     println!("{:#?}", program);
    // }

    let mut ctx = ReusableTraverseCtx::new((), scoping, &allocator);

    // Step 3: Traverse the AST to transform URLs
    if let Some(css_replacements) = css_replacements {
        let made_replacements =
            CssInlineTransformer::new(css_replacements).build(&mut program, &mut ctx);
        if made_replacements {
            ImportCssTransformer::new().build(&mut program, &mut ctx);
        }
    }
    UrlTransformer::new(url_replacements).build(&mut program, &mut ctx);
    // Step 4: Codegen back to JavaScript string
    let codegen = Codegen::new();
    let output = codegen.build(&program);

    Ok(output.code)
}

struct UrlTransformer {
    url_replacements: HashMap<String, String>,
}

impl UrlTransformer {
    fn new(url_replacements: HashMap<String, String>) -> Self {
        Self { url_replacements }
    }

    fn build<'a>(
        &mut self,
        program: &mut oxc::ast::ast::Program<'a>,
        ctx: &mut ReusableTraverseCtx<'a, ()>,
    ) {
        oxc_traverse::traverse_mut_with_ctx(self, program, ctx);
    }
}

impl<'a> Traverse<'a, ()> for UrlTransformer {
    fn enter_import_declaration(
        &mut self,
        node: &mut ImportDeclaration<'a>,
        ctx: &mut TraverseCtx<'a, ()>,
    ) {
        // replace node.source with the transformed URL
        let value = node.source.value.as_str();

        // ignore if value == "lit.all.mjs"
        if value == "lit.all.mjs" {
            return;
        }

        if let Some(replacement) = self.url_replacements.get(value) {
            node.source.value = ctx.ast.atom_from_strs_array([replacement.as_str()]);
        } else {
            panic!("URL replacement not found for: {}", value);
        }
    }
}

struct CssInlineTransformer {
    css_replacements: HashMap<String, String>,
    made_replacements: bool,
}

impl CssInlineTransformer {
    fn new(css_replacements: HashMap<String, String>) -> Self {
        Self {
            css_replacements,
            made_replacements: false,
        }
    }
    fn build<'a>(
        &mut self,
        program: &mut oxc::ast::ast::Program<'a>,
        ctx: &mut ReusableTraverseCtx<'a, ()>,
    ) -> bool {
        oxc_traverse::traverse_mut_with_ctx(self, program, ctx);
        self.made_replacements
    }

    fn extract_href_from_link_tag(&self, template_str: &str) -> Option<String> {
        let link_regex = Regex::new(r#"<link[^>]*href\s*=\s*["']([^"']+)["'][^>]*/?>"#).unwrap();
        if let Some(caps) = link_regex.captures(template_str) {
            caps.get(1).map(|m| m.as_str().to_string())
        } else {
            None
        }
    }

    fn remove_link_tag(&self, template_str: &str) -> String {
        let link_regex =
            Regex::new(r#"<link[^>]*rel\s*=\s*["']stylesheet["'][^>]*/?>\s*"#).unwrap();
        link_regex.replace_all(template_str, "").to_string()
    }
}

impl<'a> Traverse<'a, ()> for CssInlineTransformer {
    fn enter_class(&mut self, class: &mut oxc::ast::ast::Class<'a>, ctx: &mut TraverseCtx<'a, ()>) {
        let mut new_properties: Vec<ClassElement<'a>> = Vec::new(); // Collect new properties here to avoid multiple mutable borrows

        // Process all methods in the class
        for element in &mut class.body.body {
            let oxc::ast::ast::ClassElement::MethodDefinition(method_def) = element else {
                continue;
            };

            let value = &mut method_def.value;
            let Some(body) = &mut value.body else {
                continue;
            };

            // Process all statements in the method body
            for stmt in &mut body.statements {
                self.process_statement(stmt, ctx, &mut new_properties);
            }
        }

        // Append new properties to the class body after processing
        if !new_properties.is_empty() {
            class.body.body.extend(new_properties);
            self.made_replacements = true;
        }
    }

    // Also traverse into tagged template expressions to catch nested ones
    fn enter_tagged_template_expression(
        &mut self,
        tagged: &mut TaggedTemplateExpression<'a>,
        ctx: &mut TraverseCtx<'a, ()>,
    ) {
        // Check if this is an 'html' tagged template
        let Expression::Identifier(ident) = &tagged.tag else {
            return;
        };
        if ident.name != "html" {
            return;
        }

        self.process_html_template(&mut tagged.quasi, ctx);
    }
}

impl CssInlineTransformer {
    fn process_statement<'a>(
        &mut self,
        stmt: &mut Statement<'a>,
        ctx: &mut TraverseCtx<'a, ()>,
        new_properties: &mut Vec<ClassElement<'a>>,
    ) {
        match stmt {
            Statement::ReturnStatement(ret_stmt) => {
                if let Some(arg) = &mut ret_stmt.argument {
                    self.process_expression(arg, ctx, new_properties);
                }
            }
            Statement::VariableDeclaration(var_decl) => {
                for decl in &mut var_decl.declarations {
                    if let Some(init) = &mut decl.init {
                        self.process_expression(init, ctx, new_properties);
                    }
                }
            }
            Statement::ExpressionStatement(expr_stmt) => {
                self.process_expression(&mut expr_stmt.expression, ctx, new_properties);
            }
            // Add more statement types as needed
            _ => {}
        }
    }

    fn process_expression<'a>(
        &mut self,
        expr: &mut Expression<'a>,
        ctx: &mut TraverseCtx<'a, ()>,
        new_properties: &mut Vec<ClassElement<'a>>,
    ) {
        match expr {
            Expression::TaggedTemplateExpression(tagged) => {
                // Check if this is an 'html' tagged template
                let Expression::Identifier(ident) = &tagged.tag else {
                    return;
                };
                if ident.name != "html" {
                    return;
                }

                if self.process_html_template(&mut tagged.quasi, ctx) {
                    self.add_styles_property(ctx, new_properties);
                }
            }
            Expression::ConditionalExpression(cond) => {
                self.process_expression(&mut cond.test, ctx, new_properties);
                self.process_expression(&mut cond.consequent, ctx, new_properties);
                self.process_expression(&mut cond.alternate, ctx, new_properties);
            }
            Expression::BinaryExpression(bin) => {
                self.process_expression(&mut bin.left, ctx, new_properties);
                self.process_expression(&mut bin.right, ctx, new_properties);
            }
            Expression::LogicalExpression(logical) => {
                self.process_expression(&mut logical.left, ctx, new_properties);
                self.process_expression(&mut logical.right, ctx, new_properties);
            }
            Expression::AssignmentExpression(assign) => {
                self.process_expression(&mut assign.right, ctx, new_properties);
            }
            Expression::CallExpression(call) => {
                for arg in &mut call.arguments {
                    if let Some(expr) = arg.as_expression_mut() {
                        self.process_expression(expr, ctx, new_properties);
                    }
                }
            }
            // Add more expression types as needed
            _ => {}
        }
    }

    fn process_html_template<'a>(
        &mut self,
        template: &mut TemplateLiteral<'a>,
        ctx: &mut TraverseCtx<'a, ()>,
    ) -> bool {
        let mut found_replacement = false;

        for quasi in &mut template.quasis {
            let Some(cooked) = &quasi.value.cooked else {
                continue;
            };

            // Check for stylesheet link tags
            let link_tag_regex =
                Regex::new(r#"<link[\s\S]*?rel\s*=\s*[\"']stylesheet[\"'][\s\S]*?/?>"#).unwrap();
            if !link_tag_regex.is_match(cooked) {
                continue;
            }

            let Some(href) = self.extract_href_from_link_tag(cooked) else {
                continue;
            };
            if !self.css_replacements.contains_key(&href) {
                continue;
            }

            // Remove the link tag from this template element
            let new_content = self.remove_link_tag(cooked);

            quasi.value.cooked = Some(ctx.ast.atom_from_strs_array([new_content.as_str()]));
            quasi.value.raw = ctx.ast.atom_from_strs_array([new_content.as_str()]);

            found_replacement = true;
        }

        found_replacement
    }

    fn add_styles_property<'a>(
        &mut self,
        ctx: &mut TraverseCtx<'a, ()>,
        new_properties: &mut Vec<ClassElement<'a>>,
    ) {
        // Only add styles property once per class
        if new_properties.iter().any(|prop| {
            if let ClassElement::PropertyDefinition(prop_def) = prop {
                if let PropertyKey::Identifier(ident) = &prop_def.key {
                    return ident.name == "styles";
                }
            }
            false
        }) {
            return; // Already added styles property
        }

        // Combine all CSS replacements into one styles property
        let mut combined_css = String::new();
        for (href, css) in &self.css_replacements {
            combined_css.push_str(&format!("/* From {} */\n", href));
            combined_css.push_str(css);
            combined_css.push('\n');
        }

        if !combined_css.is_empty() {
            let template_element = ctx.ast.template_element(
                SPAN,
                oxc::ast::ast::TemplateElementValue {
                    cooked: Some(ctx.ast.atom_from_strs_array([combined_css.as_str()])),
                    raw: ctx.ast.atom_from_strs_array([combined_css.as_str()]),
                },
                true,
            );
            let mut quasis = ctx.ast.vec_with_capacity(1);
            quasis.push(template_element);
            let template_literal =
                ctx.ast
                    .template_literal(SPAN, quasis, ctx.ast.vec_with_capacity(0));
            let css_ident = ctx.ast.identifier_reference(SPAN, "css");
            let tagged_template_expression = ctx.ast.tagged_template_expression(
                SPAN,
                oxc::ast::ast::Expression::Identifier(ctx.ast.alloc(css_ident)),
                None::<oxc::allocator::Box<'_, oxc::ast::ast::TSTypeParameterInstantiation<'_>>>,
                template_literal,
            );
            let styles_ident = ctx.ast.identifier_reference(SPAN, "styles");
            let property_definition = ctx.ast.property_definition(
                SPAN,
                oxc::ast::ast::PropertyDefinitionType::PropertyDefinition,
                ctx.ast.vec_with_capacity(0), // decorators
                oxc::ast::ast::PropertyKey::Identifier(ctx.ast.alloc(styles_ident)),
                None::<oxc::allocator::Box<'_, oxc::ast::ast::TSTypeAnnotation<'_>>>, // type_annotation
                Some(oxc::ast::ast::Expression::TaggedTemplateExpression(
                    ctx.ast.alloc(tagged_template_expression),
                )),
                false, // computed
                false, // static
                false, // declare
                false, // override
                false, // optional
                false, // definite
                false, // readonly
                None,  // accessibility
            );
            new_properties.push(oxc::ast::ast::ClassElement::PropertyDefinition(
                ctx.ast.alloc(property_definition),
            ));
        }
    }
}

struct ImportCssTransformer;

impl ImportCssTransformer {
    fn new() -> Self {
        Self
    }

    fn build<'a>(
        &mut self,
        program: &mut oxc::ast::ast::Program<'a>,
        ctx: &mut ReusableTraverseCtx<'a, ()>,
    ) {
        oxc_traverse::traverse_mut_with_ctx(self, program, ctx);
    }
}

impl<'a> Traverse<'a, ()> for ImportCssTransformer {
    fn enter_import_declaration(
        &mut self,
        node: &mut ImportDeclaration<'a>,
        ctx: &mut TraverseCtx<'a, ()>,
    ) {
        let value = node.source.value.as_str();

        // Check if the import source ends with "lit.all.mjs"
        if !value.ends_with("lit.all.mjs") {
            return;
        }
        // Check if "css" is already in the specifiers
        let css_already_imported = node.specifiers.as_ref().map_or(false, |specs| {
            specs.iter().any(|spec| {
                if let oxc::ast::ast::ImportDeclarationSpecifier::ImportSpecifier(specific) = spec {
                    specific.imported.name() == "css"
                } else {
                    false
                }
            })
        });

        if !css_already_imported {
            // Add "css" to the specifiers
            let css_export_name = oxc::ast::ast::ModuleExportName::IdentifierName(
                ctx.ast.identifier_name(SPAN, "css")
            );
            let css_binding_ident = ctx.ast.binding_identifier(SPAN, "css");
            let import_specifier = ctx.ast.import_specifier(
                SPAN,
                css_export_name,
                css_binding_ident,
                oxc::ast::ast::ImportOrExportKind::Value,
            );
            node
                .specifiers
                .get_or_insert_with(|| ctx.ast.vec_with_capacity(0))
                .push(oxc::ast::ast::ImportDeclarationSpecifier::ImportSpecifier(
                    ctx.ast.alloc(import_specifier)
                ));
        }
    }
}
