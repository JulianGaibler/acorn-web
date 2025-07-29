use lightningcss::stylesheet::StyleSheet;
use lightningcss::values::url::Url;
use lightningcss::visitor::{Visit, VisitTypes, Visitor};
use std::collections::HashMap;

use crate::errors::TransformError;

pub struct UrlReplacer<'a> {
    url_replacements: &'a HashMap<String, String>,
}

impl<'a> UrlReplacer<'a> {
    pub fn new(url_replacements: &'a HashMap<String, String>) -> Self {
        Self { url_replacements }
    }

    pub fn build(&self, stylesheet: &mut StyleSheet) -> Result<(), TransformError> {
        let mut visitor = UrlReplacerVisitor {
            url_replacements: self.url_replacements,
        };
        stylesheet
            .visit(&mut visitor)
            .map_err(|e| TransformError::CssTransform {
                message: format!("{:?}", e),
            })
    }
}

struct UrlReplacerVisitor<'a> {
    url_replacements: &'a HashMap<String, String>,
}

impl<'a, 'i> Visitor<'i> for UrlReplacerVisitor<'a> {
    type Error = TransformError;

    fn visit_url(&mut self, url: &mut Url<'i>) -> std::result::Result<(), Self::Error> {
        let url_str = url.url.to_string();

        // Split at the first '?' or '#' to get the base part for replacement
        let (base, suffix) = match url_str.find(|c| c == '?' || c == '#') {
            Some(idx) => (&url_str[..idx], &url_str[idx..]),
            None => (url_str.as_str(), ""),
        };

        if let Some(replacement) = self.url_replacements.get(base) {
            // Reconstruct the url with the replacement and the original suffix
            let new_url = format!("{}{}", replacement, suffix);
            url.url = new_url.into();
        } else if !base.starts_with("data:")
            && !base.starts_with("http://")
            && !base.starts_with("https://")
            && !base.starts_with("//")
        {
            // print url_replacements
            eprintln!("Available replacements: {:?}", self.url_replacements);
            return Err(TransformError::UrlNotFound { url: url_str });
        }
        Ok(())
    }

    fn visit_types(&self) -> VisitTypes {
        lightningcss::visit_types!(URLS | RULES)
    }
}
