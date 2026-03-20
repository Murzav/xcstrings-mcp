use std::sync::LazyLock;

use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

static SPECIFIER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"%((\d+)\$)?[-+ 0#']*(\d+|\*)?(\.\d+|\.\*)?([hlqLzt]{0,2})[diouxXeEfgGaAcspn@]")
        .expect("specifier regex is valid")
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FormatSpecifier {
    pub raw: String,
    pub position: Option<u32>,
    pub conversion: char,
    pub length_modifier: Option<String>,
}

impl FormatSpecifier {
    pub(crate) fn is_compatible_with(&self, other: &Self) -> bool {
        self.conversion == other.conversion && self.length_modifier == other.length_modifier
    }
}

pub(crate) fn extract_specifiers(text: &str) -> Vec<FormatSpecifier> {
    // Replace %% with placeholder to avoid matching literal percent signs
    let cleaned = text.replace("%%", "\x00\x00");

    SPECIFIER_RE
        .find_iter(&cleaned)
        .map(|m| {
            let raw = &text[m.start()..m.end()];
            let caps = SPECIFIER_RE.captures(raw).expect("regex already matched");

            let position = caps.get(2).and_then(|p| p.as_str().parse::<u32>().ok());

            let conversion = raw
                .chars()
                .last()
                .expect("regex guarantees at least one char");

            let length_modifier = caps
                .get(5)
                .map(|lm| lm.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());

            FormatSpecifier {
                raw: raw.to_string(),
                position,
                conversion,
                length_modifier,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_specifier() {
        let specs = extract_specifiers("%@");
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].conversion, '@');
        assert_eq!(specs[0].position, None);
    }

    #[test]
    fn test_positional_specifier() {
        let specs = extract_specifiers("%1$@");
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].position, Some(1));
        assert_eq!(specs[0].conversion, '@');
    }

    #[test]
    fn test_multiple_specifiers() {
        let specs = extract_specifiers("%1$@ has %2$lld items");
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].position, Some(1));
        assert_eq!(specs[0].conversion, '@');
        assert_eq!(specs[1].position, Some(2));
        assert_eq!(specs[1].conversion, 'd');
        assert_eq!(specs[1].length_modifier, Some("ll".to_string()));
    }

    #[test]
    fn test_percent_escape() {
        let specs = extract_specifiers("100%% done");
        assert!(specs.is_empty());
    }

    #[test]
    fn test_float_specifier() {
        let specs = extract_specifiers("%.2f");
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].conversion, 'f');
        assert_eq!(specs[0].position, None);
    }

    #[test]
    fn test_complex() {
        let specs = extract_specifiers("%1$@ has %2$lld (%.2f%%)");
        assert_eq!(specs.len(), 3);
        assert_eq!(specs[0].raw, "%1$@");
        assert_eq!(specs[1].raw, "%2$lld");
        assert_eq!(specs[2].raw, "%.2f");
    }
}
