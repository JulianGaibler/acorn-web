use oxc::ast::ast::{
    ArrayExpression, Expression, ObjectProperty, PropertyKey, TaggedTemplateExpression,
    TemplateLiteral,
};
use oxc::span::SPAN;
use oxc_traverse::{ReusableTraverseCtx, Traverse, TraverseCtx};
use regex::Regex;
use std::collections::HashMap;

pub struct IconTemplateImportTransformer<'a> {
    path_replacements: &'a HashMap<String, String>,
    made_replacements: bool,
}

impl<'a> IconTemplateImportTransformer<'a> {
    pub fn new(path_replacements: &'a HashMap<String, String>) -> Self {
        Self {
            path_replacements,
            made_replacements: false,
        }
    }

    pub fn build(
        &mut self,
        program: &mut oxc::ast::ast::Program<'a>,
        ctx: &mut ReusableTraverseCtx<'a, ()>,
    ) -> bool {
        oxc_traverse::traverse_mut_with_ctx(self, program, ctx);
        self.made_replacements
    }
}

impl<'a> Traverse<'a, ()> for IconTemplateImportTransformer<'a> {
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

    fn enter_array_expression(
        &mut self,
        array: &mut ArrayExpression<'a>,
        ctx: &mut TraverseCtx<'a, ()>,
    ) {
        // Check each element in the array for string literals that match our replacements
        for element in array.elements.iter_mut() {
            if let oxc::ast::ast::ArrayExpressionElement::StringLiteral(string_literal) = element {
                let string_value = string_literal.value.as_str();

                if let Some(replacement_path) = self.path_replacements.get(string_value) {
                    // Replace the string literal with new URL(replacement_path, import.meta.url).href
                    let url_expression = self.create_url_expression(replacement_path, ctx);

                    // Replace the array element with the new expression
                    *element = oxc::ast::ast::ArrayExpressionElement::from(url_expression);
                    self.made_replacements = true;
                }
            }
        }
    }

    fn enter_object_property(
        &mut self,
        property: &mut ObjectProperty<'a>,
        ctx: &mut TraverseCtx<'a, ()>,
    ) {
        // Check the property value for string literals that match our replacements
        if let Expression::StringLiteral(string_literal) = &property.value {
            let string_value = string_literal.value.as_str();

            if let Some(replacement_path) = self.path_replacements.get(string_value) {
                // Replace the string literal with new URL(replacement_path, import.meta.url).href
                let url_expression = self.create_url_expression(replacement_path, ctx);

                // Replace the property value with the new expression
                property.value = url_expression;
                self.made_replacements = true;
            }
        }

        // Also check the property key if it's a string literal (less common but possible)
        if let PropertyKey::StringLiteral(string_literal) = &property.key {
            let string_value = string_literal.value.as_str();

            if let Some(replacement_path) = self.path_replacements.get(string_value) {
                // Replace the string literal key with new URL(replacement_path, import.meta.url).href
                let url_expression = self.create_url_expression(replacement_path, ctx);

                // Create a computed property key from the expression
                property.key = PropertyKey::from(url_expression);
                property.computed = true; // Mark as computed since we're using an expression
                self.made_replacements = true;
            }
        }
    }
}

impl<'a> IconTemplateImportTransformer<'a> {
    fn process_html_template(
        &mut self,
        template: &mut TemplateLiteral<'a>,
        ctx: &mut TraverseCtx<'a, ()>,
    ) {
        // Pattern to match src="chrome://..." or iconsrc="chrome://..." attributes
        let src_regex = Regex::new(r#"(src|iconsrc)\s*=\s*["']([^"']+)["']"#).unwrap();

        // To support multiple replacements, iterate until no more matches are found
        let mut idx = 0;
        while idx < template.quasis.len() {
            let quasi = &template.quasis[idx];
            let Some(cooked) = &quasi.value.cooked else {
                idx += 1;
                continue;
            };

            // Find all matches in this quasi
            let mut cooked_str = cooked.as_ref();
            let mut found = false;
            while let Some(caps) = src_regex.captures(cooked_str) {
                let full_match = caps.get(0).unwrap();
                let src_value = caps.get(2).unwrap().as_str();
                if let Some(replacement_path) = self.path_replacements.get(src_value) {
                    let before_src = cooked_str[..full_match.start()].to_string();
                    let after_src = cooked_str[full_match.end()..].to_string();
                    self.replace_src_with_url_expression(
                        template,
                        idx,
                        &before_src,
                        &after_src,
                        replacement_path,
                        ctx,
                    );
                    self.made_replacements = true;
                    // After insertion, the current quasi is split, so move to the next quasi after the inserted one
                    idx += 2;
                    found = true;
                    break;
                } else {
                    // If no replacement, skip this match and continue searching
                    cooked_str = &cooked_str[full_match.end()..];
                }
            }
            if !found {
                idx += 1;
            }
        }
    }

    fn replace_src_with_url_expression(
        &mut self,
        template: &mut TemplateLiteral<'a>,
        quasi_index: usize,
        before_src: &str,
        after_src: &str,
        replacement_path: &str,
        ctx: &mut TraverseCtx<'a, ()>,
    ) {
        // Create the new template structure:
        // 1. First quasi: content before src + 'src="'
        // 2. Expression: new URL(...)
        // 3. Second quasi: '"' + content after src + rest

        let new_before = format!("{}src=\"", before_src);
        let new_after = format!("\"{}", after_src);

        // Update current quasi to be the "before" part
        let current_quasi = &mut template.quasis[quasi_index];
        let is_tail = current_quasi.tail;

        current_quasi.value.cooked = Some(ctx.ast.atom_from_strs_array([new_before.as_str()]));
        current_quasi.value.raw = ctx.ast.atom_from_strs_array([new_before.as_str()]);
        current_quasi.tail = false;

        // Create the new URL expression: new URL('./relative/path', import.meta.url)
        let url_expression = self.create_url_expression(replacement_path, ctx);

        // Create the "after" template element
        let after_element = ctx.ast.template_element(
            SPAN,
            oxc::ast::ast::TemplateElementValue {
                cooked: Some(ctx.ast.atom_from_strs_array([new_after.as_str()])),
                raw: ctx.ast.atom_from_strs_array([new_after.as_str()]),
            },
            is_tail, // Inherit the tail status from the original quasi
        );

        // Insert the new expression and template element
        template.expressions.insert(quasi_index, url_expression);
        template.quasis.insert(quasi_index + 1, after_element);
    }

    fn create_url_expression(
        &self,
        replacement_path: &str,
        ctx: &mut TraverseCtx<'a, ()>,
    ) -> Expression<'a> {
        // Create: new URL('./relative/path', import.meta.url).href

        // Create the URL identifier
        let url_ident = ctx.ast.identifier_reference(SPAN, "URL");

        // Create the first argument: string literal with the replacement path
        let path_atom = ctx.ast.atom_from_strs_array([replacement_path]);
        let path_literal = ctx.ast.string_literal(SPAN, path_atom, None);
        let path_arg = oxc::ast::ast::Argument::StringLiteral(ctx.ast.alloc(path_literal));

        // Create import.meta.url
        let import_ident = ctx.ast.identifier_name(SPAN, "import");
        let meta_ident = ctx.ast.identifier_name(SPAN, "meta");
        let url_ident_name = ctx.ast.identifier_name(SPAN, "url");

        let import_meta = ctx.ast.meta_property(SPAN, import_ident, meta_ident);
        let import_meta_url = ctx.ast.static_member_expression(
            SPAN,
            Expression::MetaProperty(ctx.ast.alloc(import_meta)),
            url_ident_name,
            false,
        );
        let meta_url_arg =
            oxc::ast::ast::Argument::StaticMemberExpression(ctx.ast.alloc(import_meta_url));

        // Create arguments vector
        let mut arguments = ctx.ast.vec_with_capacity(2);
        arguments.push(path_arg);
        arguments.push(meta_url_arg);

        // Create the new expression
        let new_expr = ctx.ast.new_expression(
            SPAN,
            Expression::Identifier(ctx.ast.alloc(url_ident)),
            None as Option<
                oxc::allocator::Box<'a, oxc::ast::ast::TSTypeParameterInstantiation<'a>>,
            >,
            arguments,
        );

        // Wrap in StaticMemberExpression to access .href property
        let href_ident = ctx.ast.identifier_name(SPAN, "href");
        let static_member = ctx.ast.static_member_expression(
            SPAN,
            Expression::NewExpression(ctx.ast.alloc(new_expr)),
            href_ident,
            false,
        );

        Expression::StaticMemberExpression(ctx.ast.alloc(static_member))
    }
}
