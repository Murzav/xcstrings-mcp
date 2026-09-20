use serde_json::json;
use std::collections::BTreeMap;
use xcstrings_mcp::model::glossary::*;
use xcstrings_mcp::service::glossary::{
    apply_glossary_edit, legacy_upsert, parse_glossary_document, project_legacy_entries,
    serialize_glossary_document,
};

#[test]
fn legacy_migration_preserves_both_locale_directions_and_values() {
    let parsed = parse_glossary_document(Some(
        r#"{"en→fr":{"Account":"Compte"},"fr→en":{"Compte":"Account"}}"#,
    ))
    .unwrap();
    assert!(parsed.needs_migration);
    assert_eq!(parsed.document.schema_version, 2);
    assert_eq!(parsed.document.terms.len(), 2);
    assert_eq!(
        project_legacy_entries(&parsed.document, "en", "fr", None).entries,
        BTreeMap::from([("Account".into(), "Compte".into())])
    );
    let text = serialize_glossary_document(&parsed.document).unwrap();
    let again = parse_glossary_document(Some(&text)).unwrap();
    assert!(!again.needs_migration);
    assert_eq!(again.document, parsed.document);
}

#[test]
fn structured_policy_roundtrip_conserves_unknown_nested_data() {
    let raw = json!({"schema_version":2,"terms":[{"id":"account","source_locale":"en","target_locale":"fr","source":"Account","preferred":["Compte"],"scope":{"screen_future":{"arr":[1,{"x":true}]}},"vendor":{"approved":false}}],"vendor":{"revision":7}}).to_string();
    let parsed = parse_glossary_document(Some(&raw)).unwrap();
    let again = parse_glossary_document(Some(
        &serialize_glossary_document(&parsed.document).unwrap(),
    ))
    .unwrap();
    assert_eq!(again.document, parsed.document);
    assert_eq!(
        again.document.terms[0].scope.extra["screen_future"],
        json!({"arr":[1,{"x":true}]})
    );
}

#[test]
fn parser_rejects_duplicate_members_inside_unknown_array_objects() {
    let raw = r#"{"schema_version":2,"terms":[],"vendor":[{"a":1,"a":2}]}"#;
    assert!(
        parse_glossary_document(Some(raw))
            .unwrap_err()
            .to_string()
            .contains("duplicate")
    );
}

#[test]
fn parser_rejects_unknown_schema_version() {
    let error = parse_glossary_document(Some(r#"{"schema_version":3,"terms":[]}"#)).unwrap_err();
    assert!(error.to_string().contains("schema_version"));
}

#[test]
fn parser_rejects_duplicate_ids() {
    let raw = json!({"schema_version":2,"terms":[term(),term()]}).to_string();
    assert!(
        parse_glossary_document(Some(&raw))
            .unwrap_err()
            .to_string()
            .contains("duplicate_term_id")
    );
}

#[test]
fn edit_rejects_whole_batch_when_one_rule_invalid_without_mutation() {
    let original = GlossaryDocument {
        terms: vec![term()],
        ..Default::default()
    };
    let bad = GlossaryTerm {
        id: "bad".into(),
        source: String::new(),
        ..term()
    };
    let error = apply_glossary_edit(
        &original,
        &GlossaryEdit {
            upsert: vec![bad],
            remove_ids: vec!["account".into()],
        },
    )
    .unwrap_err();
    assert_eq!(error[0].code, "empty_source");
    assert_eq!(original.terms, vec![term()]);
}

#[test]
fn edit_rejects_same_identity_removed_and_upserted() {
    let error = apply_glossary_edit(
        &GlossaryDocument::default(),
        &GlossaryEdit {
            upsert: vec![term()],
            remove_ids: vec!["account".into()],
        },
    )
    .unwrap_err();
    assert_eq!(error[0].code, "duplicate_edit_id");
}

#[test]
fn unbounded_exception_is_rejected() {
    let mut rule = term();
    rule.exceptions.push(TermException {
        reason: "ignore anywhere".into(),
        ignore_checks: vec![TermCheck::Preferred],
        ..Default::default()
    });
    let error = apply_glossary_edit(
        &GlossaryDocument::default(),
        &GlossaryEdit {
            upsert: vec![rule],
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(error[0].code, "unbounded_exception");
}

#[test]
fn legacy_upsert_keeps_rich_contextual_rules_and_unknown_document_fields() {
    let mut rich = term();
    rich.scope.roles = vec!["verb".into()];
    let original = GlossaryDocument {
        terms: vec![rich.clone()],
        extra: BTreeMap::from([("vendor".into(), json!([7, 3]))]),
        ..Default::default()
    };
    let updated = legacy_upsert(
        &original,
        "en",
        "fr",
        &BTreeMap::from([("Account".into(), "Profil".into())]),
    )
    .unwrap();
    assert_eq!(updated.terms[0], rich);
    assert_eq!(updated.extra, original.extra);
    assert_eq!(updated.terms.len(), 2);
    let projection = project_legacy_entries(&updated, "en", "fr", None);
    assert_eq!(
        projection.entries,
        BTreeMap::from([("Account".into(), "Profil".into())])
    );
    assert_eq!(projection.omitted_term_ids, vec!["account"]);
}

#[test]
fn forbidden_and_accepted_identical_form_is_invalid() {
    let mut rule = term();
    rule.forbidden = vec!["Compte".into()];
    let error = apply_glossary_edit(
        &GlossaryDocument::default(),
        &GlossaryEdit {
            upsert: vec![rule],
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(error[0].code, "contradictory_term");
}

fn term() -> GlossaryTerm {
    GlossaryTerm {
        id: "account".into(),
        source_locale: "en".into(),
        target_locale: "fr".into(),
        source: "Account".into(),
        preferred: vec!["Compte".into()],
        ..Default::default()
    }
}

#[test]
fn exact_key_selectors_allow_empty_and_canonically_distinct_catalog_keys() {
    let mut rule = term();
    rule.scope.keys = vec!["".into(), "Café".into(), "Cafe\u{301}".into()];
    let candidate = apply_glossary_edit(
        &GlossaryDocument::default(),
        &GlossaryEdit {
            upsert: vec![rule.clone()],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        candidate.terms[0].scope.keys,
        vec!["", "Café", "Cafe\u{301}"]
    );
}
