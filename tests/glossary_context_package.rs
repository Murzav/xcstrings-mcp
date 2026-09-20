use serde_json::json;
use std::collections::BTreeMap;
use xcstrings_mcp::model::{
    context::*,
    xcstrings::{XcStringsFile, paths::LeafStep},
};
use xcstrings_mcp::service::{
    context::{
        apply_context_edits, authored_snapshot, build_context_package, resolve_context,
        validate_authored_context, validate_context_bindings,
    },
    parser,
};

fn catalog() -> XcStringsFile {
    parser::parse(&json!({"sourceLanguage":"en","version":"1.0","strings":{
    "profile.action":{"comment":"Developer instruction","localizations":{"en":{"stringUnit":{"state":"translated","value":"Show %2$@ for %1$lld"}},"fr":{"stringUnit":{"state":"needs_review","value":"Voir %2$@ pour %1$lld"}}}},
    "profile.label":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"Label"}}}},
    "related":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"Related"}}}},
    "unrelated":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"Unrelated"}}}}
}}).to_string()).unwrap()
}
fn contexts() -> CatalogContexts {
    BTreeMap::from([(
        "profile.action".into(),
        AuthoredContext {
            context: ContextFields {
                screen: Some("Profile".into()),
                role: Some("button".into()),
                variables: Some(vec![VariableMeaning {
                    reference: VariableReference::Argument(1),
                    meaning: "Number of selected accounts".into(),
                    extra: BTreeMap::new(),
                }]),
                neighbors: Some(vec!["related".into()]),
                ..Default::default()
            },
            ..Default::default()
        },
    )])
}
#[test]
fn count_zero_returns_requested_key_comment_draft_and_explicit_meaning() {
    let package =
        build_context_package(&catalog(), &contexts(), None, "profile.action", "fr", 0).unwrap();
    assert_eq!(package.current.key, "profile.action");
    assert_eq!(
        package.current.comment.as_deref(),
        Some("Developer instruction")
    );
    assert_eq!(
        package.current.leaves[0].state,
        Some(xcstrings_mcp::model::xcstrings::TranslationState::NeedsReview)
    );
    assert_eq!(package.authored.fields.screen.as_deref(), Some("Profile"));
    assert_eq!(package.leaf_contexts[0].variables[0].position, Some(1));
    assert_eq!(
        package.leaf_contexts[0].variables[0].meaning.as_deref(),
        Some("Number of selected accounts")
    );
    assert_eq!(package.leaf_contexts[0].variables[1].meaning, None);
    assert!(package.neighbors.is_empty());
}
#[test]
fn neighbors_prioritize_explicit_and_do_not_fill_unrelated_prefixes() {
    let package =
        build_context_package(&catalog(), &contexts(), None, "profile.action", "fr", 50).unwrap();
    assert_eq!(
        package
            .neighbors
            .iter()
            .map(|n| (&*n.unit.key, n.relation))
            .collect::<Vec<_>>(),
        vec![
            ("related", NeighborRelation::Explicit),
            ("profile.label", NeighborRelation::PrefixHeuristic)
        ]
    );
    assert!(!package.neighbors_truncated);
}
#[test]
fn leaf_override_preserves_provenance_and_explicit_empty_list_clears() {
    let mut contexts = contexts();
    contexts.get_mut("profile.action").unwrap().leaves = vec![LeafContextOverride {
        path: vec![LeafStep::Plural("other".into())],
        context: ContextFields {
            role: Some("label".into()),
            variables: Some(vec![]),
            ..Default::default()
        },
        ..Default::default()
    }];
    let resolved = resolve_context(
        &contexts,
        "profile.action",
        &[LeafStep::Plural("other".into())],
    )
    .unwrap();
    assert_eq!(resolved.fields.screen.as_deref(), Some("Profile"));
    assert_eq!(resolved.fields.role.as_deref(), Some("label"));
    assert_eq!(resolved.fields.variables, Some(vec![]));
    assert_eq!(
        resolved.provenance,
        BTreeMap::from([
            ("neighbors".into(), ContextOrigin::Key),
            ("role".into(), ContextOrigin::Leaf),
            ("screen".into(), ContextOrigin::Key),
            ("variables".into(), ContextOrigin::Leaf)
        ])
    );
}
#[test]
fn duplicate_leaf_override_is_rejected_instead_of_first_match_winning() {
    let leaf = LeafContextOverride {
        path: vec![],
        ..Default::default()
    };
    let authored = AuthoredContext {
        leaves: vec![leaf.clone(), leaf],
        ..Default::default()
    };
    assert_eq!(
        validate_authored_context("key", &authored)[0].code,
        "duplicate_context_path"
    );
}
#[test]
fn source_snapshot_conserves_unknown_data_and_excludes_other_keys() {
    let mut contexts = contexts();
    contexts
        .get_mut("profile.action")
        .unwrap()
        .extra
        .insert("future".into(), json!({"ordered":[3,1]}));
    let snapshot = authored_snapshot(&contexts, "profile.action");
    contexts.insert(
        "other".into(),
        AuthoredContext {
            context: ContextFields {
                purpose: Some("Unrelated".into()),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    assert_eq!(authored_snapshot(&contexts, "profile.action"), snapshot);
    assert_eq!(snapshot["future"], json!({"ordered":[3,1]}));
    assert_eq!(authored_snapshot(&contexts, "missing"), json!(null));
}
#[test]
fn source_snapshot_ignores_leaf_override_order_but_not_authored_neighbor_order() {
    let a = LeafContextOverride {
        path: vec![LeafStep::Plural("one".into())],
        context: ContextFields {
            role: Some("one".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    let b = LeafContextOverride {
        path: vec![LeafStep::Plural("other".into())],
        context: ContextFields {
            role: Some("other".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    let left = BTreeMap::from([(
        "k".into(),
        AuthoredContext {
            leaves: vec![a.clone(), b.clone()],
            ..Default::default()
        },
    )]);
    let right = BTreeMap::from([(
        "k".into(),
        AuthoredContext {
            leaves: vec![b, a],
            ..Default::default()
        },
    )]);
    assert_eq!(
        authored_snapshot(&left, "k"),
        authored_snapshot(&right, "k")
    );
}
#[test]
fn stale_variable_binding_reports_diagnostic_without_deleting_authored_data() {
    let mut contexts = contexts();
    contexts
        .get_mut("profile.action")
        .unwrap()
        .context
        .variables
        .as_mut()
        .unwrap()[0]
        .reference = VariableReference::Argument(9);
    let original = contexts.clone();
    let diagnostics = validate_context_bindings(&catalog(), &contexts);
    assert_eq!(diagnostics[0].code, "unknown_context_variable");
    assert_eq!(contexts, original);
}
#[test]
fn context_edit_failure_is_atomic_and_remove_can_clean_deleted_key() {
    let contexts = contexts();
    let error = apply_context_edits(
        &catalog(),
        &contexts,
        &[
            ContextEdit::Remove {
                key: "profile.action".into(),
            },
            ContextEdit::Set {
                key: "missing".into(),
                context: Box::default(),
            },
        ],
    )
    .unwrap_err();
    assert_eq!(error[0].code, "unknown_context_key");
    assert!(contexts.contains_key("profile.action"));
    let stale = BTreeMap::from([("missing".into(), AuthoredContext::default())]);
    assert_eq!(
        apply_context_edits(
            &catalog(),
            &stale,
            &[ContextEdit::Remove {
                key: "missing".into()
            }]
        )
        .unwrap(),
        BTreeMap::new()
    );
}
#[test]
fn screenshot_parent_traversal_and_active_uri_are_rejected() {
    let bad = AuthoredContext {
        context: ContextFields {
            screenshots: Some(vec![
                ScreenshotReference {
                    uri: "../secret.png".into(),
                    ..Default::default()
                },
                ScreenshotReference {
                    uri: "javascript:alert(1)".into(),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(
        validate_authored_context("key", &bad)
            .iter()
            .map(|d| d.code.as_str())
            .collect::<Vec<_>>(),
        vec![
            "invalid_screenshot_reference",
            "invalid_screenshot_reference"
        ]
    );
}
#[test]
fn screenshots_are_inert_relative_or_https_references() {
    let authored = AuthoredContext {
        context: ContextFields {
            screenshots: Some(vec![
                ScreenshotReference {
                    uri: "screens/profile.png".into(),
                    ..Default::default()
                },
                ScreenshotReference {
                    uri: "https://example.com/profile.png".into(),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(validate_authored_context("key", &authored), vec![]);
}
#[test]
fn missing_context_never_infers_screen_or_meaning_from_key() {
    let package = build_context_package(
        &catalog(),
        &CatalogContexts::new(),
        None,
        "profile.action",
        "fr",
        0,
    )
    .unwrap();
    assert_eq!(package.authored, ResolvedContext::default());
    assert_eq!(package.leaf_contexts[0].variables[0].meaning, None);
}
#[test]
fn missing_requested_key_is_explicit_error() {
    assert!(
        matches!(build_context_package(&catalog(),&contexts(),None,"missing","fr",0),Err(xcstrings_mcp::XcStringsError::KeyNotFound(key)) if key=="missing")
    );
}

#[test]
fn dynamic_width_precision_and_value_have_actual_argument_positions() {
    let file=parser::parse(&json!({"sourceLanguage":"en","version":"1.0","strings":{"k":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"%*.*f"}}}}}}).to_string()).unwrap();
    let package =
        build_context_package(&file, &CatalogContexts::new(), None, "k", "fr", 0).unwrap();
    assert_eq!(
        package.leaf_contexts[0]
            .variables
            .iter()
            .map(|v| (v.position, v.role, v.format.as_deref()))
            .collect::<Vec<_>>(),
        vec![
            (Some(1), ArgumentRole::Width, Some("%d")),
            (Some(2), ArgumentRole::Precision, Some("%d")),
            (Some(3), ArgumentRole::Value, Some("%f"))
        ]
    );
}

#[test]
fn named_substitution_meaning_uses_metadata_position_not_iteration_order() {
    let file=parser::parse(&json!({"sourceLanguage":"en","version":"1.0","strings":{"k":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"%#@COUNT@ items"},"substitutions":{"COUNT":{"argNum":2,"formatSpecifier":"lld","variations":{"plural":{"one":{"stringUnit":{"state":"translated","value":"%arg item"}},"other":{"stringUnit":{"state":"translated","value":"%arg items"}}}}}}}}}}}).to_string()).unwrap();
    let authored = AuthoredContext {
        context: ContextFields {
            variables: Some(vec![VariableMeaning {
                reference: VariableReference::Substitution("COUNT".into()),
                meaning: "Number of remaining files".into(),
                extra: BTreeMap::new(),
            }]),
            ..Default::default()
        },
        ..Default::default()
    };
    let package = build_context_package(
        &file,
        &BTreeMap::from([("k".into(), authored)]),
        None,
        "k",
        "fr",
        0,
    )
    .unwrap();
    assert_eq!(
        package.leaf_contexts[0].variables[0].reference,
        VariableReference::Substitution("COUNT".into())
    );
    assert_eq!(package.leaf_contexts[0].variables[0].position, Some(2));
    assert_eq!(
        package.leaf_contexts[0].variables[0].meaning.as_deref(),
        Some("Number of remaining files")
    );
    assert_eq!(package.diagnostics, vec![]);
}

#[test]
fn returned_screenshots_are_explicitly_unverified_resources() {
    let mut contexts = contexts();
    contexts
        .get_mut("profile.action")
        .unwrap()
        .context
        .screenshots = Some(vec![ScreenshotReference {
        uri: "screens/profile.png".into(),
        ..Default::default()
    }]);
    let package =
        build_context_package(&catalog(), &contexts, None, "profile.action", "fr", 0).unwrap();
    assert_eq!(
        package.authored.fields.screenshots.as_ref().unwrap()[0].uri,
        "screens/profile.png"
    );
    assert_eq!(
        package
            .diagnostics
            .iter()
            .filter(|d| d.code == "screenshot_availability_unverified")
            .count(),
        1
    );
}

#[test]
fn existing_target_only_plural_accepts_context_bound_to_source_key_fallback() {
    let file=parser::parse(&json!({"sourceLanguage":"en","version":"1.0","strings":{"%lld files":{"localizations":{"fr":{"variations":{"plural":{"one":{"stringUnit":{"state":"needs_review","value":"%lld fichier"}},"other":{"stringUnit":{"state":"needs_review","value":"%lld fichiers"}}}}}}}}}).to_string()).unwrap();
    let authored = AuthoredContext {
        leaves: vec![LeafContextOverride {
            path: vec![LeafStep::Plural("other".into())],
            context: ContextFields {
                purpose: Some("Count label".into()),
                variables: Some(vec![VariableMeaning {
                    reference: VariableReference::Argument(1),
                    meaning: "File count".into(),
                    extra: BTreeMap::new(),
                }]),
                ..Default::default()
            },
            ..Default::default()
        }],
        ..Default::default()
    };
    let updated = apply_context_edits(
        &file,
        &CatalogContexts::new(),
        &[ContextEdit::Set {
            key: "%lld files".into(),
            context: Box::new(authored.clone()),
        }],
    )
    .unwrap();
    assert_eq!(updated["%lld files"], authored);
    assert_eq!(validate_context_bindings(&file, &updated), vec![]);
}

#[test]
fn leaf_specific_explicit_neighbor_is_included_before_prefix_fallback() {
    let file=parser::parse(&json!({"sourceLanguage":"en","version":"1.0","strings":{"count":{"localizations":{"en":{"variations":{"plural":{"one":{"stringUnit":{"state":"translated","value":"%lld file"}},"other":{"stringUnit":{"state":"translated","value":"%lld files"}}}}}}},"related":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"Details"}}}}}}).to_string()).unwrap();
    let authored = AuthoredContext {
        leaves: vec![LeafContextOverride {
            path: vec![LeafStep::Plural("other".into())],
            context: ContextFields {
                neighbors: Some(vec!["related".into()]),
                ..Default::default()
            },
            ..Default::default()
        }],
        ..Default::default()
    };
    let package = build_context_package(
        &file,
        &BTreeMap::from([("count".into(), authored)]),
        None,
        "count",
        "fr",
        50,
    )
    .unwrap();
    assert_eq!(
        package
            .neighbors
            .iter()
            .map(|n| (&*n.unit.key, n.relation))
            .collect::<Vec<_>>(),
        vec![("related", NeighborRelation::Explicit)]
    );
}

#[test]
fn explicit_neighbor_may_address_existing_empty_catalog_key() {
    let file=parser::parse(&json!({"sourceLanguage":"en","version":"1.0","strings":{"title":{},"":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"Empty key content"}}}}}}).to_string()).unwrap();
    let authored = AuthoredContext {
        context: ContextFields {
            neighbors: Some(vec![String::new()]),
            ..Default::default()
        },
        ..Default::default()
    };
    let contexts = apply_context_edits(
        &file,
        &CatalogContexts::new(),
        &[ContextEdit::Set {
            key: "title".into(),
            context: Box::new(authored),
        }],
    )
    .unwrap();
    let package = build_context_package(&file, &contexts, None, "title", "fr", 1).unwrap();
    assert_eq!(package.neighbors.len(), 1);
    assert_eq!(package.neighbors[0].unit.key, "");
    assert_eq!(package.neighbors[0].relation, NeighborRelation::Explicit);
}
