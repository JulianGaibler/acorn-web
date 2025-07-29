use std::collections::HashMap;

use oxc::ast::ast::ImportDeclaration;
use oxc_traverse::{ReusableTraverseCtx, Traverse, TraverseCtx};

pub struct UrlTransformer<'a> {
    url_replacements: &'a HashMap<String, String>,
}

impl<'a> UrlTransformer<'a> {
    pub fn new(url_replacements: &'a HashMap<String, String>) -> Self {
        Self { url_replacements }
    }

    pub fn build(
        &mut self,
        program: &mut oxc::ast::ast::Program<'a>,
        ctx: &mut ReusableTraverseCtx<'a, ()>,
    ) {
        oxc_traverse::traverse_mut_with_ctx(self, program, ctx);
    }
}

impl<'a> Traverse<'a, ()> for UrlTransformer<'a> {
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
