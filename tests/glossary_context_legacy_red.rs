use xcstrings_mcp::{XcStringsError, service::glossary::parse_glossary};

#[test]
fn legacy_glossary_rejects_duplicate_term_instead_of_losing_first_value() {
    let result = parse_glossary(Some(r#"{"en→fr":{"account":"compte","account":"profil"}}"#));
    assert!(
        matches!(result, Err(XcStringsError::GlossaryError(ref message)) if message.contains("duplicate"))
    );
}

#[test]
fn legacy_glossary_rejects_duplicate_locale_pair_instead_of_losing_terms() {
    let result = parse_glossary(Some(
        r#"{"en→fr":{"account":"compte"},"en→fr":{"profile":"profil"}}"#,
    ));
    assert!(
        matches!(result, Err(XcStringsError::GlossaryError(ref message)) if message.contains("duplicate"))
    );
}
