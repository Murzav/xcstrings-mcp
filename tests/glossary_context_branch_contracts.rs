//! Public corruption and loss-prevention contracts found during branch review.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use xcstrings_mcp::model::{context::*, glossary::*};
use xcstrings_mcp::service::{context, glossary};
use xcstrings_mcp::{XcStringsError, XcStringsFile};

fn catalog() -> XcStringsFile {
    serde_json::from_value(json!({"sourceLanguage":"en","version":"1.0","strings":{
        "account":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"Account %lld"}}}},
        "keep":{"comment":"Unrelated metadata"}
    }})).unwrap()
}
fn authored(value: Value) -> AuthoredContext {
    serde_json::from_value(value).unwrap()
}
fn rule() -> GlossaryTerm {
    serde_json::from_value(
        json!({"id":"legacy:2:en:2:fr:7:Account","source_locale":"en",
        "target_locale":"fr","source":"Account","preferred":["Compte"],"vendor":{"keep":[8,3]}}),
    )
    .unwrap()
}
fn context_diagnostic(code: &str, key: &str, path: Option<Value>, detail: &str) -> Value {
    json!({"code":code,"key":key,"path":path,"detail":detail})
}

macro_rules! malformed_pair {
    ($name:ident,$raw:literal,$message:literal) => {
        #[test]
        fn $name() {
            let error = glossary::parse_glossary_document(Some($raw)).unwrap_err();
            assert!(matches!(error, XcStringsError::GlossaryError(ref text) if text == $message));
        }
    };
}
malformed_pair!(
    legacy_missing_pair_separator_is_not_silently_dropped,
    r#"{"en-fr":{"Account":"Compte"}}"#,
    "legacy locale pair must contain →"
);
malformed_pair!(
    legacy_ambiguous_pair_separator_is_not_guessed,
    r#"{"en→fr→de":{"Account":"Compte"}}"#,
    "invalid legacy locale pair"
);

macro_rules! legacy_collision {
    ($name:ident,$field:literal,$value:expr) => {
        #[test]
        fn $name() {
            let mut value = serde_json::to_value(rule()).unwrap();
            value[$field] = $value;
            let existing: GlossaryTerm = serde_json::from_value(value).unwrap();
            let document = GlossaryDocument {
                terms: vec![existing],
                ..Default::default()
            };
            let rejected = glossary::legacy_upsert(
                &document,
                "en",
                "fr",
                &BTreeMap::from([("Account".into(), "Profil".into())]),
            )
            .unwrap_err();
            assert_eq!(
                rejected,
                vec![GlossaryDiagnostic {
                    code: "legacy_identity_collision".into(),
                    term_id: Some("legacy:2:en:2:fr:7:Account".into()),
                    detail: "Existing rule does not represent this legacy entry".into()
                }]
            );
        }
    };
}
legacy_collision!(
    legacy_edit_cannot_overwrite_another_source_with_reserved_id,
    "source",
    json!("Profile")
);
legacy_collision!(
    legacy_edit_cannot_overwrite_another_source_locale,
    "source_locale",
    json!("de")
);
legacy_collision!(
    legacy_edit_cannot_overwrite_another_target_locale,
    "target_locale",
    json!("ca")
);
legacy_collision!(
    legacy_edit_cannot_erase_a_contextual_scope,
    "scope",
    json!({"screens":["Billing"]})
);

#[test]
fn legacy_projection_never_picks_one_of_two_different_preferred_rules() {
    let document: GlossaryDocument = serde_json::from_value(json!({"schema_version":2,"terms":[
        {"id":"personal","source_locale":"en","target_locale":"fr","source":"Account","preferred":["Compte"]},
        {"id":"public","source_locale":"en","target_locale":"fr","source":"Account","preferred":["Profil"]}
    ]})).unwrap();
    assert_eq!(
        glossary::project_legacy_entries(&document, "en", "fr", None),
        GlossaryProjection {
            entries: BTreeMap::new(),
            omitted_term_ids: vec!["personal".into(), "public".into()]
        }
    );
}

macro_rules! rich_projection {
    ($name:ident,$field:literal,$value:expr) => {
        #[test]
        fn $name() {
            let mut value = serde_json::to_value(rule()).unwrap();
            value[$field] = $value;
            let document: GlossaryDocument = serde_json::from_value(json!({"schema_version":2,"terms":[value]})).unwrap();
            assert_eq!(glossary::validate_glossary(&document),vec![]);
            assert_eq!(glossary::project_legacy_entries(&document,"en","fr",None),GlossaryProjection{
                entries:BTreeMap::new(),omitted_term_ids:vec!["legacy:2:en:2:fr:7:Account".into()]});
        }
    };
}
rich_projection!(
    legacy_projection_does_not_discard_unknown_scope,
    "scope",
    json!({"future_axis":["billing"]})
);
rich_projection!(
    legacy_projection_does_not_choose_a_preferred_synonym,
    "preferred",
    json!(["Compte", "Profil"])
);
rich_projection!(
    legacy_projection_does_not_discard_source_variants,
    "source_variants",
    json!(["Accounts"])
);
rich_projection!(
    legacy_projection_does_not_discard_accepted_inflections,
    "accepted_variants",
    json!(["Comptes"])
);
rich_projection!(
    legacy_projection_does_not_discard_forbidden_forms,
    "forbidden",
    json!(["Profil"])
);
rich_projection!(
    legacy_projection_does_not_discard_contextual_exception,
    "exceptions",
    json!([{"scope":{"paths":[[]]},"reason":"Reviewed root label","ignore_checks":["preferred"]}])
);

#[test]
fn legacy_projection_does_not_reduce_do_not_translate_to_a_preferred_form() {
    let document: GlossaryDocument = serde_json::from_value(json!({"schema_version":2,"terms":[
        {"id":"brand","source_locale":"en","target_locale":"fr","source":"Acme","preferred":["Acme"],"do_not_translate":true}
    ]})).unwrap();
    assert_eq!(glossary::validate_glossary(&document), vec![]);
    assert_eq!(
        glossary::project_legacy_entries(&document, "en", "fr", None),
        GlossaryProjection {
            entries: BTreeMap::new(),
            omitted_term_ids: vec!["brand".into()]
        }
    );
}

macro_rules! duplicate_selector {
    ($name:ident,$scope:expr,$code:literal,$detail:literal) => {
        #[test]
        fn $name() {
            let mut term = rule();
            term.scope = serde_json::from_value($scope).unwrap();
            assert_eq!(
                glossary::validate_glossary(&GlossaryDocument {
                    terms: vec![term],
                    ..Default::default()
                }),
                vec![GlossaryDiagnostic {
                    code: $code.into(),
                    term_id: Some("legacy:2:en:2:fr:7:Account".into()),
                    detail: $detail.into()
                }]
            );
        }
    };
}
duplicate_selector!(
    duplicate_empty_key_selector_is_rejected,
    json!({"keys":["",""]}),
    "duplicate_scope_key",
    "Scope keys must be unique"
);
duplicate_selector!(
    duplicate_screen_selector_is_rejected,
    json!({"screens":["Billing","Billing"]}),
    "duplicate_term_form",
    "Context selectors must be unique"
);
duplicate_selector!(
    duplicate_role_selector_is_rejected,
    json!({"roles":["label","label"]}),
    "duplicate_term_form",
    "Context selectors must be unique"
);

#[test]
fn context_batch_cannot_both_replace_and_remove_the_same_record() {
    let contexts = BTreeMap::from([(
        "account".into(),
        authored(json!({"context":{"purpose":"Keep meaning"},"vendor":[7,2]})),
    )]);
    let edits = [
        ContextEdit::Set {
            key: "account".into(),
            context: Box::new(authored(json!({"context":{"purpose":"New meaning"}}))),
        },
        ContextEdit::Remove {
            key: "account".into(),
        },
    ];
    let errors = context::apply_context_edits(&catalog(), &contexts, &edits).unwrap_err();
    assert_eq!(
        serde_json::to_value(errors).unwrap(),
        json!([context_diagnostic(
            "duplicate_context_edit",
            "account",
            None,
            "Each key may be addressed once"
        )])
    );
}

#[test]
fn invalid_binding_rejects_the_whole_context_batch_including_valid_removal() {
    let contexts = BTreeMap::from([(
        "keep".into(),
        authored(json!({"context":{"purpose":"Retain this record"},"vendor":{"value":8}})),
    )]);
    let errors = context::apply_context_edits(
        &catalog(),
        &contexts,
        &[
            ContextEdit::Remove { key: "keep".into() },
            ContextEdit::Set {
                key: "account".into(),
                context: Box::new(authored(json!({"context":{"neighbors":["missing"]}}))),
            },
        ],
    )
    .unwrap_err();
    assert_eq!(
        serde_json::to_value(errors).unwrap(),
        json!([context_diagnostic(
            "unknown_context_neighbor",
            "account",
            None,
            "Neighbor missing does not exist"
        )])
    );
    assert_eq!(
        serde_json::to_value(contexts).unwrap(),
        json!({"keep":{"context":{"purpose":"Retain this record"},"leaves":[],"vendor":{"value":8}}})
    );
}

#[test]
fn resolving_a_corrupt_context_fails_instead_of_returning_partial_facts() {
    let contexts = BTreeMap::from([(
        "account".into(),
        authored(json!({"context":{"purpose":"Valid purpose","role":" "}})),
    )]);
    let errors = context::resolve_context(&contexts, "account", &[]).unwrap_err();
    assert_eq!(
        serde_json::to_value(errors).unwrap(),
        json!([context_diagnostic(
            "empty_context_field",
            "account",
            None,
            "role must not be blank"
        )])
    );
}

#[test]
fn binding_validation_reports_stale_context_keys_explicitly() {
    let contexts = BTreeMap::from([(
        "removed".into(),
        authored(json!({"context":{"purpose":"Old context"}})),
    )]);
    assert_eq!(
        serde_json::to_value(context::validate_context_bindings(&catalog(), &contexts)).unwrap(),
        json!([context_diagnostic(
            "unknown_context_key",
            "removed",
            None,
            "Authored context refers to a missing catalog key"
        )])
    );
}

#[test]
fn nonexisting_target_only_path_is_not_invented_from_key_fallback() {
    let file: XcStringsFile = serde_json::from_value(
        json!({"sourceLanguage":"en","version":"1.0","strings":{"%lld files":{}}}),
    )
    .unwrap();
    let contexts = BTreeMap::from([(
        "%lld files".into(),
        authored(json!({"leaves":[{"path":[{"plural":"other"}],"context":{"purpose":"Count"}}]})),
    )]);
    assert_eq!(
        serde_json::to_value(context::validate_context_bindings(&file, &contexts)).unwrap(),
        json!([context_diagnostic(
            "unknown_context_path",
            "%lld files",
            Some(json!([{"plural":"other"}])),
            "No source leaf can resolve this context path"
        )])
    );
}

#[test]
fn unsupported_leaf_axis_is_rejected_before_context_resolution() {
    let contexts = BTreeMap::from([(
        "account".into(),
        authored(
            json!({"leaves":[{"path":[{"plural":"future"}],"context":{"purpose":"No guessed category"}}]}),
        ),
    )]);
    let errors = context::resolve_context(&contexts, "account", &[]).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(
        (errors[0].code.as_str(), errors[0].key.as_str()),
        ("invalid_context_path", "account")
    );
    assert_eq!(
        serde_json::to_value(&errors[0].path).unwrap(),
        json!([{"plural":"future"}])
    );
}

macro_rules! unsafe_screenshot {
    ($name:ident,$uri:literal) => {
        #[test]
        fn $name() {
            let record=authored(json!({"context":{"screenshots":[{"uri":$uri}]}}));
            assert_eq!(serde_json::to_value(context::validate_authored_context("account",&record)).unwrap(),json!([
                context_diagnostic("invalid_screenshot_reference","account",None,"Screenshot must be an inert HTTPS URL or a safe relative path")
            ]));
        }
    };
}
unsafe_screenshot!(
    screenshot_control_character_is_not_an_inert_reference,
    "screens/billing\n.png"
);
unsafe_screenshot!(
    screenshot_backslash_path_cannot_hide_parent_traversal,
    "screens\\..\\billing.png"
);
unsafe_screenshot!(
    screenshot_encoded_parent_traversal_is_not_accepted,
    "screens/%2e%2e/billing.png"
);
