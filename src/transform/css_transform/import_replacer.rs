use lightningcss::stylesheet::StyleSheet;
use lightningcss::visitor::{Visit, VisitTypes, Visitor};
use std::collections::HashMap;

use crate::errors::TransformError;

pub struct ImportReplacer<'a> {
    url_replacements: &'a HashMap<String, String>,
}

impl<'a> ImportReplacer<'a> {
    pub fn new(url_replacements: &'a HashMap<String, String>) -> Self {
        Self { url_replacements }
    }

    pub fn build(&self, stylesheet: &mut StyleSheet) -> Result<(), TransformError> {
        let mut visitor = ImportReplacerVisitor {
            url_replacements: self.url_replacements,
        };
        stylesheet
            .visit(&mut visitor)
            .map_err(|e| TransformError::CssTransform {
                message: format!("{:?}", e),
            })
    }
}

struct ImportReplacerVisitor<'a> {
    url_replacements: &'a HashMap<String, String>,
}

impl<'a, 'i> Visitor<'i> for ImportReplacerVisitor<'a> {
    type Error = TransformError;

    fn visit_rule(
        &mut self,
        rule: &mut lightningcss::rules::CssRule<'i>,
    ) -> std::result::Result<(), Self::Error> {
        if let lightningcss::rules::CssRule::Import(import_rule) = rule {
            let url_str = import_rule.url.to_string();
            if let Some(replacement) = self.url_replacements.get(&url_str) {
                import_rule.url = replacement.clone().into();
            } else if !url_str.starts_with("data:")
                && !url_str.starts_with("http://")
                && !url_str.starts_with("https://")
                && !url_str.starts_with("//")
            {
                return Err(TransformError::UrlNotFound { url: url_str });
            }
        }
        Ok(())
    }

    fn visit_types(&self) -> VisitTypes {
        lightningcss::visit_types!(URLS | RULES)
    }
}
