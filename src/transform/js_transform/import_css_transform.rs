use oxc::ast::ast::ImportDeclaration;
use oxc::span::SPAN;
use oxc_traverse::{ReusableTraverseCtx, Traverse, TraverseCtx};

// ...existing code...

pub struct ImportCssTransformer {
    css_imported: bool,
}

impl ImportCssTransformer {
    pub fn new() -> Self {
        Self {
            css_imported: false,
        }
    }

    pub fn build<'a>(
        &mut self,
        program: &mut oxc::ast::ast::Program<'a>,
        ctx: &mut ReusableTraverseCtx<'a, ()>,
    ) {
        self.css_imported = false;
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
        // Check if "css" is already in the specifiers or has been added in this file
        let css_already_imported = node.specifiers.as_ref().map_or(false, |specs| {
            specs.iter().any(|spec| {
                if let oxc::ast::ast::ImportDeclarationSpecifier::ImportSpecifier(specific) = spec {
                    specific.imported.name() == "css"
                } else {
                    false
                }
            })
        });

        if css_already_imported || self.css_imported {
            if css_already_imported {
                self.css_imported = true;
            }
            return;
        }

        // Add "css" to the specifiers
        let css_export_name =
            oxc::ast::ast::ModuleExportName::IdentifierName(ctx.ast.identifier_name(SPAN, "css"));
        let css_binding_ident = ctx.ast.binding_identifier(SPAN, "css");
        let import_specifier = ctx.ast.import_specifier(
            SPAN,
            css_export_name,
            css_binding_ident,
            oxc::ast::ast::ImportOrExportKind::Value,
        );
        node.specifiers
            .get_or_insert_with(|| ctx.ast.vec_with_capacity(0))
            .push(oxc::ast::ast::ImportDeclarationSpecifier::ImportSpecifier(
                ctx.ast.alloc(import_specifier),
            ));
        self.css_imported = true;
    }
}
