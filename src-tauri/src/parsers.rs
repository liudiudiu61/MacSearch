use std::collections::HashSet;

pub trait ContentParser {
    fn parse(&self, bytes: &[u8]) -> Result<String, ParseFailure>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseFailure {
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Utf8TextParser {
    parse_error_code: String,
}

impl Utf8TextParser {
    pub fn new(parse_error_code: String) -> Self {
        Self { parse_error_code }
    }
}

impl ContentParser for Utf8TextParser {
    fn parse(&self, bytes: &[u8]) -> Result<String, ParseFailure> {
        std::str::from_utf8(bytes)
            .map(|content| content.to_string())
            .map_err(|_| ParseFailure {
                code: self.parse_error_code.clone(),
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParserRegistry {
    text_extensions: HashSet<String>,
    text_parser: Utf8TextParser,
}

impl ParserRegistry {
    pub fn from_text_extensions(text_extensions: Vec<String>, parse_error_code: String) -> Self {
        Self {
            text_extensions: text_extensions
                .into_iter()
                .map(|extension| normalize_extension(&extension))
                .collect(),
            text_parser: Utf8TextParser::new(parse_error_code),
        }
    }

    pub fn parse_extension(&self, extension: &str, bytes: &[u8]) -> Result<String, ParseFailure> {
        if self
            .text_extensions
            .contains(&normalize_extension(extension))
        {
            return self.text_parser.parse(bytes);
        }

        Err(ParseFailure {
            code: self.text_parser.parse_error_code.clone(),
        })
    }

    pub fn supports_extension(&self, extension: &str) -> bool {
        self.text_extensions
            .contains(&normalize_extension(extension))
    }
}

fn normalize_extension(extension: &str) -> String {
    let trimmed = extension.trim().to_ascii_lowercase();
    if trimmed.is_empty() || trimmed.starts_with('.') {
        trimmed
    } else {
        format!(".{trimmed}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsers_utf8_text_content_for_configured_text_extension() {
        let registry = ParserRegistry::from_text_extensions(
            vec![".md".to_string(), ".txt".to_string()],
            "Parse_Error".to_string(),
        );

        let content = registry
            .parse_extension(".md", b"AnyTXT parser boundary")
            .expect("text parser succeeds");

        assert_eq!(content, "AnyTXT parser boundary");
    }

    #[test]
    fn parsers_malformed_utf8_with_configured_error_code() {
        let parser = Utf8TextParser::new("Parse_Error".to_string());

        let failure = parser.parse(&[0xff, 0xfe, 0xfd]).expect_err("parse fails");

        assert_eq!(
            failure,
            ParseFailure {
                code: "Parse_Error".to_string()
            }
        );
    }

    #[test]
    fn parsers_report_unsupported_extension_without_fixed_business_code() {
        let registry = ParserRegistry::from_text_extensions(
            vec![".md".to_string()],
            "Parse_Error".to_string(),
        );

        assert!(!registry.supports_extension(".pdf"));
        assert!(registry.supports_extension(".md"));
    }
}
