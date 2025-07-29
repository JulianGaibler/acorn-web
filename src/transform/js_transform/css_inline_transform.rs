use ast::{ClassElement, Expression, PropertyKey, Statement, TemplateLiteral};
use oxc::{ast::ast, span::SPAN};
use oxc_traverse::{ReusableTraverseCtx, Traverse, TraverseCtx};
use regex::Regex;
use std::collections::HashMap;

pub struct CssInlineTransformer<'a> {
    css_replacements: &'a HashMap<String, String>,
    made_replacements: bool,
    referenced_hrefs: Vec<String>,
}

impl<'a> CssInlineTransformer<'a> {
    pub fn new(css_replacements: &'a HashMap<String, String>) -> Self {
        Self {
            css_replacements,
            made_replacements: false,
            referenced_hrefs: Vec::new(),
        }
    }
    pub fn build(
        &mut self,
        program: &mut ast::Program<'a>,
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

impl<'a> Traverse<'a, ()> for CssInlineTransformer<'a> {
    fn enter_class(&mut self, class: &mut ast::Class<'a>, ctx: &mut TraverseCtx<'a, ()>) {
        // get super_class name if it exists
        let super_class_name_string = if let Some(super_class) = &class.super_class {
            if let ast::Expression::Identifier(ident) = &super_class {
                Some(ident.name.to_string())
            } else {
                None
            }
        } else {
            None
        };
        let super_class_name = super_class_name_string.as_ref().map(|s| s.as_str());

        let mut new_properties: Vec<ClassElement<'a>> = Vec::new();

        // Clear referenced_hrefs for this class
        self.referenced_hrefs.clear();

        // Process all methods in the class
        for element in &mut class.body.body {
            let ast::ClassElement::MethodDefinition(method_def) = element else {
                continue;
            };

            let value = &mut method_def.value;
            let Some(body) = &mut value.body else {
                continue;
            };

            // Process all statements in the method body
            for stmt in &mut body.statements {
                self.process_statement(stmt, ctx, &mut new_properties, &super_class_name);
            }
        }

        // Append new properties to the class body after processing
        if !new_properties.is_empty() {
            class.body.body.extend(new_properties);
            self.made_replacements = true;
        }
    }
}

impl<'a> CssInlineTransformer<'a> {
    fn process_statement(
        &mut self,
        stmt: &mut Statement<'a>,
        ctx: &mut TraverseCtx<'a, ()>,
        new_properties: &mut Vec<ClassElement<'a>>,
        super_class_name: &Option<&str>,
    ) {
        match stmt {
            Statement::ReturnStatement(ret_stmt) => {
                if let Some(arg) = &mut ret_stmt.argument {
                    self.process_expression(arg, ctx, new_properties, super_class_name);
                }
            }
            Statement::VariableDeclaration(var_decl) => {
                for decl in &mut var_decl.declarations {
                    if let Some(init) = &mut decl.init {
                        self.process_expression(init, ctx, new_properties, super_class_name);
                    }
                }
            }
            Statement::ExpressionStatement(expr_stmt) => {
                self.process_expression(
                    &mut expr_stmt.expression,
                    ctx,
                    new_properties,
                    super_class_name,
                );
            }
            // Add more statement types as needed
            _ => {}
        }
    }

    fn process_expression(
        &mut self,
        expr: &mut Expression<'a>,
        ctx: &mut TraverseCtx<'a, ()>,
        new_properties: &mut Vec<ClassElement<'a>>,
        super_class_name: &Option<&str>,
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
                    self.add_styles_property(ctx, new_properties, super_class_name);
                }
            }
            Expression::ConditionalExpression(cond) => {
                self.process_expression(&mut cond.test, ctx, new_properties, super_class_name);
                self.process_expression(
                    &mut cond.consequent,
                    ctx,
                    new_properties,
                    super_class_name,
                );
                self.process_expression(&mut cond.alternate, ctx, new_properties, super_class_name);
            }
            Expression::BinaryExpression(bin) => {
                self.process_expression(&mut bin.left, ctx, new_properties, super_class_name);
                self.process_expression(&mut bin.right, ctx, new_properties, super_class_name);
            }
            Expression::LogicalExpression(logical) => {
                self.process_expression(&mut logical.left, ctx, new_properties, super_class_name);
                self.process_expression(&mut logical.right, ctx, new_properties, super_class_name);
            }
            Expression::AssignmentExpression(assign) => {
                self.process_expression(&mut assign.right, ctx, new_properties, super_class_name);
            }
            Expression::CallExpression(call) => {
                for arg in &mut call.arguments {
                    if let Some(expr) = arg.as_expression_mut() {
                        self.process_expression(expr, ctx, new_properties, super_class_name);
                    }
                }
            }
            // Add more expression types as needed
            _ => {}
        }
    }

    fn process_html_template(
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
                Regex::new(r#"<link[\s\S]*?rel\s*=\s*[\"']stylesheet[\"'][\s\S]*/?>"#).unwrap();
            if !link_tag_regex.is_match(cooked) {
                continue;
            }

            let Some(href) = self.extract_href_from_link_tag(cooked) else {
                continue;
            };
            if !self.css_replacements.contains_key(&href) {
                continue;
            }

            // Track referenced hrefs for this class
            if !self.referenced_hrefs.contains(&href) {
                self.referenced_hrefs.push(href.clone());
            }

            // Remove the link tag from this template element
            let new_content = self.remove_link_tag(cooked);

            quasi.value.cooked = Some(ctx.ast.atom_from_strs_array([new_content.as_str()]));
            quasi.value.raw = ctx.ast.atom_from_strs_array([new_content.as_str()]);

            found_replacement = true;
        }

        found_replacement
    }

    fn add_styles_property(
        &mut self,
        ctx: &mut TraverseCtx<'a, ()>,
        new_properties: &mut Vec<ClassElement<'a>>,
        super_class_name: &Option<&str>,
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

        // Combine only referenced CSS replacements into one styles property
        let mut combined_css = String::new();
        for href in &self.referenced_hrefs {
            if let Some(css) = self.css_replacements.get(href) {
                combined_css.push_str(&format!("/* From {} */\n", href));
                combined_css.push_str(css);
                combined_css.push('\n');
            }
        }

        if !combined_css.is_empty() {
            // Build the tagged template expression for the new styles
            let template_element = ctx.ast.template_element(
                SPAN,
                ast::TemplateElementValue {
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
                ast::Expression::Identifier(ctx.ast.alloc(css_ident)),
                None::<oxc::allocator::Box<'_, ast::TSTypeParameterInstantiation<'_>>>,
                template_literal,
            );

            // Build the array expression: [ ...(SuperClass.styles ?? []), css`...` ]
            let mut array_elements = ctx.ast.vec_with_capacity(2);
            if let Some(super_class) = super_class_name {
                // use &str directly
                let base_ident = ctx
                    .ast
                    .alloc_identifier_reference(SPAN, ctx.ast.atom(super_class));
                let styles_ident = ctx.ast.identifier_name(SPAN, "styles");
                let static_member = ctx.ast.static_member_expression(
                    SPAN,
                    ast::Expression::Identifier(base_ident),
                    styles_ident,
                    false,
                );
                let empty_array = ctx.ast.array_expression(SPAN, ctx.ast.vec_with_capacity(0));
                let logical_expr = ctx.ast.logical_expression(
                    SPAN,
                    ast::Expression::StaticMemberExpression(ctx.ast.alloc(static_member)),
                    ast::LogicalOperator::Coalesce,
                    ast::Expression::ArrayExpression(ctx.ast.alloc(empty_array)),
                );
                let paren_expr = ctx.ast.parenthesized_expression(
                    SPAN,
                    ast::Expression::LogicalExpression(ctx.ast.alloc(logical_expr)),
                );
                let spread_element = ctx.ast.spread_element(
                    SPAN,
                    ast::Expression::ParenthesizedExpression(ctx.ast.alloc(paren_expr)),
                );
                array_elements.push(ast::ArrayExpressionElement::SpreadElement(
                    ctx.ast.alloc(spread_element),
                ));
            }
            array_elements.push(ast::ArrayExpressionElement::TaggedTemplateExpression(
                ctx.ast.alloc(tagged_template_expression),
            ));
            let array_expr = ctx.ast.array_expression(SPAN, array_elements);

            // Build the property definition
            let styles_ident = ctx.ast.identifier_reference(SPAN, "styles");
            let property_definition = ctx.ast.property_definition(
                SPAN,
                ast::PropertyDefinitionType::PropertyDefinition,
                ctx.ast.vec_with_capacity(0), // decorators
                ast::PropertyKey::Identifier(ctx.ast.alloc(styles_ident)),
                None::<oxc::allocator::Box<'_, ast::TSTypeAnnotation<'_>>>, // type_annotation
                Some(ast::Expression::ArrayExpression(ctx.ast.alloc(array_expr))),
                false, // computed
                true,  // static
                false, // declare
                false, // override
                false, // optional
                false, // definite
                false, // readonly
                None,  // accessibility
            );
            new_properties.push(ast::ClassElement::PropertyDefinition(
                ctx.ast.alloc(property_definition),
            ));
        }
    }
}
