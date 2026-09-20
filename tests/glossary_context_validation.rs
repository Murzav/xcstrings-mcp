use serde_json::{Value, json};
use xcstrings_mcp::model::{
    context::AuthoredContext,
    glossary::{GlossaryDocument, GlossaryTerm},
};
use xcstrings_mcp::service::{context::validate_authored_context, glossary::validate_glossary};

fn rule() -> Value {
    json!({"id":"save","source_locale":"en","target_locale":"fr","source":"Save","preferred":["Enregistrer"]})
}
macro_rules! invalid_rule {
    ($name:ident,$field:literal,$value:expr,$code:literal) => {
        invalid_rule!($name, $field, $value, $code, "save");
    };
    ($name:ident,$field:literal,$value:expr,$code:literal,$identity:literal) => {
        #[test]
        fn $name() {
            let mut value = rule();
            value[$field] = $value;
            let term: GlossaryTerm = serde_json::from_value(value).unwrap();
            let issues = validate_glossary(&GlossaryDocument {
                terms: vec![term],
                ..Default::default()
            });
            assert_eq!(issues[0].code, $code);
            assert_eq!(issues[0].term_id.as_deref(), Some($identity));
        }
    };
}
invalid_rule!(
    empty_term_identity_is_invalid,
    "id",
    json!(""),
    "empty_id",
    ""
);
invalid_rule!(
    malformed_source_locale_is_invalid,
    "source_locale",
    json!("en-1"),
    "invalid_locale"
);
invalid_rule!(
    rule_without_checks_is_invalid,
    "preferred",
    json!([]),
    "empty_rule"
);
invalid_rule!(
    blank_preferred_form_is_invalid,
    "preferred",
    json!([" "]),
    "empty_term_form"
);
invalid_rule!(
    canonically_duplicate_forms_are_invalid,
    "preferred",
    json!(["Café", "Cafe\u{301}"]),
    "duplicate_term_form"
);
invalid_rule!(
    do_not_translate_cannot_prefer_an_unrelated_spelling,
    "do_not_translate",
    json!(true),
    "contradictory_term"
);
invalid_rule!(
    exception_requires_reason,
    "exceptions",
    json!([{"scope":{"keys":["key"]},"reason":"","ignore_checks":["preferred"]}]),
    "missing_exception_reason"
);
invalid_rule!(
    exception_requires_an_action,
    "exceptions",
    json!([{"scope":{"roles":["button"]},"reason":"Button meaning"}]),
    "empty_exception"
);
invalid_rule!(
    scope_paths_are_unique,
    "scope",
    json!({"paths":[[],[]]}),
    "duplicate_scope_path"
);
invalid_rule!(
    unknown_scope_plural_is_invalid,
    "scope",
    json!({"paths":[[{"plural":"never"}]]}),
    "invalid_scope_path"
);
invalid_rule!(
    scope_screen_is_not_blank,
    "scope",
    json!({"screens":[""]}),
    "empty_term_form"
);

macro_rules! invalid_context {
    ($name:ident,$context:expr,$code:literal)=>{
        #[test] fn $name() {
            let authored:AuthoredContext=serde_json::from_value(json!({"context":$context})).unwrap();
            let issues=validate_authored_context("key",&authored);
            assert_eq!(issues[0].code,$code);assert_eq!(issues[0].key,"key");assert_eq!(issues[0].path,None);
        }
    }
}
invalid_context!(
    context_fact_must_not_be_blank,
    json!({"screen":" "}),
    "empty_context_field"
);
invalid_context!(
    zero_argument_position_is_invalid,
    json!({"variables":[{"reference":{"argument":0},"meaning":"Count"}]}),
    "invalid_context_variable"
);
invalid_context!(
    empty_substitution_name_is_invalid,
    json!({"variables":[{"reference":{"substitution":""},"meaning":"Count"}]}),
    "invalid_context_variable"
);
invalid_context!(
    meaning_must_not_be_blank,
    json!({"variables":[{"reference":{"argument":1},"meaning":""}]}),
    "empty_variable_meaning"
);
invalid_context!(
    duplicate_variable_binding_is_invalid,
    json!({"variables":[{"reference":{"argument":1},"meaning":"Count"},{"reference":{"argument":1},"meaning":"Width"}]}),
    "duplicate_context_variable"
);
invalid_context!(
    self_neighbor_is_invalid,
    json!({"neighbors":["key"]}),
    "invalid_context_neighbor"
);
invalid_context!(
    duplicate_neighbor_is_invalid,
    json!({"neighbors":["other","other"]}),
    "invalid_context_neighbor"
);
invalid_context!(
    screenshot_absolute_file_is_invalid,
    json!({"screenshots":[{"uri":"/tmp/secret.png"}]}),
    "invalid_screenshot_reference"
);
invalid_context!(
    screenshot_empty_https_authority_is_invalid,
    json!({"screenshots":[{"uri":"https:///image.png"}]}),
    "invalid_screenshot_reference"
);
invalid_context!(
    screenshot_credentials_are_invalid,
    json!({"screenshots":[{"uri":"https://user:pass@example.com/image.png"}]}),
    "invalid_screenshot_reference"
);
invalid_context!(
    screenshot_data_uri_is_invalid,
    json!({"screenshots":[{"uri":"data:image/png;base64,AAAA"}]}),
    "invalid_screenshot_reference"
);
