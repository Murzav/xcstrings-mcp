use proptest::prelude::*;
use serde_json::{Value, json};
use xcstrings_mcp::service::semantic_merge::{
    ConflictChoice, ConflictResolution, MergeOptions, MergeReport, fingerprint, prepare_merge,
};

fn catalog(version: &str, strings: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "sourceLanguage": "en",
        "strings": strings,
        "version": version,
    }))
    .unwrap()
}

fn merged_value(base: &[u8], current: &[u8], incoming: &[u8]) -> Value {
    let prepared = prepare_merge(base, current, incoming, &MergeOptions::default()).unwrap();
    serde_json::from_str(&prepared.content).unwrap()
}

#[test]
fn three_way_key_matrix_preserves_changes_and_deletions() {
    struct Case {
        name: &'static str,
        base: Value,
        current: Value,
        incoming: Value,
        expected: Value,
    }

    let cases = [
        Case {
            name: "unchanged",
            base: json!({"key": {"comment": "base"}}),
            current: json!({"key": {"comment": "base"}}),
            incoming: json!({"key": {"comment": "base"}}),
            expected: json!({"key": {"comment": "base"}}),
        },
        Case {
            name: "current change",
            base: json!({"key": {"comment": "base"}}),
            current: json!({"key": {"comment": "current"}}),
            incoming: json!({"key": {"comment": "base"}}),
            expected: json!({"key": {"comment": "current"}}),
        },
        Case {
            name: "incoming change",
            base: json!({"key": {"comment": "base"}}),
            current: json!({"key": {"comment": "base"}}),
            incoming: json!({"key": {"comment": "incoming"}}),
            expected: json!({"key": {"comment": "incoming"}}),
        },
        Case {
            name: "identical double add",
            base: json!({}),
            current: json!({"key": {"comment": "same"}}),
            incoming: json!({"key": {"comment": "same"}}),
            expected: json!({"key": {"comment": "same"}}),
        },
        Case {
            name: "double delete",
            base: json!({"key": {"comment": "base"}}),
            current: json!({}),
            incoming: json!({}),
            expected: json!({}),
        },
        Case {
            name: "current delete incoming unchanged",
            base: json!({"key": {"comment": "base"}}),
            current: json!({}),
            incoming: json!({"key": {"comment": "base"}}),
            expected: json!({}),
        },
        Case {
            name: "incoming delete current unchanged",
            base: json!({"key": {"comment": "base"}}),
            current: json!({"key": {"comment": "base"}}),
            incoming: json!({}),
            expected: json!({}),
        },
    ];

    for case in cases {
        let base = catalog("1.0", case.base);
        let current = catalog("1.0", case.current);
        let incoming = catalog("1.0", case.incoming);
        let merged = merged_value(&base, &current, &incoming);
        assert_eq!(merged["strings"], case.expected, "{}", case.name);
    }
}

#[test]
fn merges_known_maps_but_keeps_string_units_and_unknown_subtrees_atomic() {
    let base = catalog(
        "1.0",
        json!({
            "key": {
                "future": {"left": 1, "right": 1},
                "localizations": {
                    "en": {
                        "stringUnit": {"state": "translated", "value": "Base"},
                        "variations": {
                            "plural": {
                                "one": {"stringUnit": {"state": "translated", "value": "one base"}},
                                "other": {"stringUnit": {"state": "translated", "value": "other base"}}
                            },
                            "device": {
                                "iphone": {"stringUnit": {"state": "translated", "value": "phone base"}}
                            }
                        },
                        "substitutions": {
                            "COUNT": {
                                "argNum": 1,
                                "formatSpecifier": "lld",
                                "variations": {"plural": {
                                    "one": {"stringUnit": {"state": "translated", "value": "%arg base"}},
                                    "other": {"stringUnit": {"state": "translated", "value": "%arg bases"}}
                                }}
                            }
                        }
                    }
                }
            }
        }),
    );
    let current = catalog(
        "1.0",
        json!({
            "key": {
                "future": {"left": 2, "right": 1},
                "localizations": {
                    "en": {
                        "stringUnit": {"state": "translated", "value": "Current"},
                        "variations": {
                            "plural": {
                                "one": {"stringUnit": {"state": "translated", "value": "one current"}},
                                "other": {"stringUnit": {"state": "translated", "value": "other base"}}
                            },
                            "device": {
                                "iphone": {"stringUnit": {"state": "translated", "value": "phone current"}}
                            }
                        },
                        "substitutions": {
                            "COUNT": {
                                "argNum": 1,
                                "formatSpecifier": "lld",
                                "variations": {"plural": {
                                    "one": {"stringUnit": {"state": "translated", "value": "%arg current"}},
                                    "other": {"stringUnit": {"state": "translated", "value": "%arg bases"}}
                                }}
                            }
                        }
                    }
                }
            }
        }),
    );
    let incoming = catalog(
        "1.0",
        json!({
            "key": {
                "future": {"left": 1, "right": 2},
                "localizations": {
                    "en": {
                        "stringUnit": {"state": "new", "value": "Base"},
                        "variations": {
                            "plural": {
                                "one": {"stringUnit": {"state": "translated", "value": "one base"}},
                                "other": {"stringUnit": {"state": "translated", "value": "other incoming"}}
                            },
                            "device": {
                                "iphone": {"stringUnit": {"state": "translated", "value": "phone base"}},
                                "ipad": {"stringUnit": {"state": "translated", "value": "tablet incoming"}}
                            }
                        },
                        "substitutions": {
                            "COUNT": {
                                "argNum": 1,
                                "formatSpecifier": "lld",
                                "variations": {"plural": {
                                    "one": {"stringUnit": {"state": "translated", "value": "%arg base"}},
                                    "other": {"stringUnit": {"state": "translated", "value": "%arg incoming"}}
                                }}
                            }
                        }
                    }
                }
            }
        }),
    );

    let prepared = prepare_merge(&base, &current, &incoming, &MergeOptions::default()).unwrap();
    assert_eq!(prepared.report.conflict_total, 2);
    let pointers: Vec<_> = prepared
        .report
        .conflicts
        .iter()
        .map(|conflict| conflict.pointer.as_str())
        .collect();
    assert!(pointers.contains(&"/strings/key/future"));
    assert!(pointers.contains(&"/strings/key/localizations/en/stringUnit"));

    let mut resolutions = prepared
        .report
        .conflicts
        .iter()
        .map(|conflict| ConflictResolution {
            conflict_id: conflict.id.clone(),
            choice: ConflictChoice::Current,
        })
        .collect::<Vec<_>>();
    resolutions.sort_by(|left, right| left.conflict_id.cmp(&right.conflict_id));
    let resolved = prepare_merge(
        &base,
        &current,
        &incoming,
        &MergeOptions {
            resolutions,
            ..MergeOptions::default()
        },
    )
    .unwrap();
    let value: Value = serde_json::from_str(&resolved.content).unwrap();
    let en = &value["strings"]["key"]["localizations"]["en"];
    assert_eq!(en["stringUnit"]["value"], "Current");
    assert_eq!(en["stringUnit"]["state"], "translated");
    assert_eq!(
        en["variations"]["plural"]["one"]["stringUnit"]["value"],
        "one current"
    );
    assert_eq!(
        en["variations"]["plural"]["other"]["stringUnit"]["value"],
        "other incoming"
    );
    assert_eq!(
        en["variations"]["device"]["iphone"]["stringUnit"]["value"],
        "phone current"
    );
    assert_eq!(
        en["variations"]["device"]["ipad"]["stringUnit"]["value"],
        "tablet incoming"
    );
    assert_eq!(
        en["substitutions"]["COUNT"]["variations"]["plural"]["one"]["stringUnit"]["value"],
        "%arg current"
    );
    assert_eq!(
        en["substitutions"]["COUNT"]["variations"]["plural"]["other"]["stringUnit"]["value"],
        "%arg incoming"
    );
    assert_eq!(
        value["strings"]["key"]["future"],
        json!({"left": 2, "right": 1})
    );
}

#[test]
fn preserves_current_order_and_appends_incoming_only_keys_in_order() {
    let base = catalog("1.0", json!({"b": {}, "a": {}}));
    let current = catalog("1.0", json!({"b": {}, "a": {}, "current": {}}));
    let incoming = catalog("1.0", json!({"b": {}, "a": {}, "z": {}, "c": {}}));

    let merged = merged_value(&base, &current, &incoming);
    let keys = merged["strings"]
        .as_object()
        .unwrap()
        .keys()
        .collect::<Vec<_>>();
    assert_eq!(keys, ["b", "a", "current", "z", "c"]);
}

#[test]
fn rejects_source_language_mismatch_and_conflicts_on_divergent_version() {
    let base = catalog("1.0", json!({}));
    let current = catalog("1.1", json!({}));
    let incoming = catalog("2.0", json!({}));
    let mismatch = br#"{"sourceLanguage":"de","strings":{},"version":"1.0"}"#;

    let error = prepare_merge(&base, &current, mismatch, &MergeOptions::default()).unwrap_err();
    assert!(error.to_string().contains("sourceLanguage"));

    let prepared = prepare_merge(&base, &current, &incoming, &MergeOptions::default()).unwrap();
    assert_eq!(prepared.report.conflict_total, 1);
    assert_eq!(prepared.report.conflicts[0].pointer, "/version");
    assert_eq!(prepared.report.conflicts[0].kind, "scalar_divergence");
}

#[test]
fn conflict_ids_pagination_previews_and_fingerprints_are_stable_and_bounded() {
    let long_current = "c".repeat(400);
    let long_incoming = "i".repeat(400);
    let base = catalog(
        "1.0",
        json!({"a": {"comment": "base"}, "b": {"comment": "base"}}),
    );
    let current = catalog(
        "1.0",
        json!({"a": {"comment": long_current}, "b": {"comment": "current"}}),
    );
    let incoming = catalog(
        "1.0",
        json!({"a": {"comment": long_incoming}, "b": {"comment": "incoming"}}),
    );
    let options = MergeOptions {
        conflict_offset: 1,
        conflict_limit: 1,
        ..MergeOptions::default()
    };

    let first = prepare_merge(&base, &current, &incoming, &options).unwrap();
    let second = prepare_merge(&base, &current, &incoming, &options).unwrap();
    assert_eq!(first.report.conflict_total, 2);
    assert_eq!(first.report.unresolved_conflict_total, 2);
    assert_eq!(first.report.conflict_offset, 1);
    assert_eq!(first.report.conflicts.len(), 1);
    assert!(!first.report.has_more);
    assert_eq!(first.report.conflicts[0].id, second.report.conflicts[0].id);
    assert!(first.report.conflicts[0].current.preview.len() <= 203);
    assert!(first.report.conflicts[0].incoming.preview.len() <= 203);
    assert_eq!(first.report.fingerprints.base.sha256, fingerprint(&base));
    assert_eq!(first.report.fingerprints.current.key_count, 2);
    assert_eq!(first.report.fingerprints.result.key_count, 2);
}

#[test]
fn divergent_add_delete_modify_and_all_resolution_choices_are_exact() {
    let base = catalog(
        "1.0",
        json!({"delete_modify": {"comment": "base"}, "modify_delete": {"comment": "base"}}),
    );
    let current = catalog(
        "1.0",
        json!({
            "added": {"comment": "current add"},
            "modify_delete": {"comment": "current change"}
        }),
    );
    let incoming = catalog(
        "1.0",
        json!({
            "added": {"comment": "incoming add"},
            "delete_modify": {"comment": "incoming change"}
        }),
    );
    let conflicts = prepare_merge(&base, &current, &incoming, &MergeOptions::default()).unwrap();
    assert_eq!(conflicts.report.conflict_total, 3);
    let by_pointer = conflicts
        .report
        .conflicts
        .iter()
        .map(|conflict| (conflict.pointer.as_str(), conflict))
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(by_pointer["/strings/added"].kind, "divergent_add");
    assert_eq!(by_pointer["/strings/delete_modify"].kind, "delete_modify");
    assert_eq!(by_pointer["/strings/modify_delete"].kind, "delete_modify");

    let resolutions = [
        ("/strings/added", ConflictChoice::Incoming),
        ("/strings/delete_modify", ConflictChoice::Base),
        ("/strings/modify_delete", ConflictChoice::Current),
    ]
    .into_iter()
    .map(|(pointer, choice)| ConflictResolution {
        conflict_id: by_pointer[pointer].id.clone(),
        choice,
    })
    .collect();
    let resolved = prepare_merge(
        &base,
        &current,
        &incoming,
        &MergeOptions {
            resolutions,
            ..MergeOptions::default()
        },
    )
    .unwrap();
    assert_eq!(resolved.report.resolutions_applied, 3);
    assert_eq!(resolved.report.unresolved_conflict_total, 0);
    let value: Value = serde_json::from_str(&resolved.content).unwrap();
    assert_eq!(value["strings"]["added"]["comment"], "incoming add");
    assert_eq!(value["strings"]["delete_modify"]["comment"], "base");
    assert_eq!(
        value["strings"]["modify_delete"]["comment"],
        "current change"
    );
}

#[test]
fn divergent_changes_report_exact_schema_level_pointers() {
    struct Case {
        name: &'static str,
        base: Value,
        current: Value,
        incoming: Value,
        pointer: &'static str,
    }
    let cases = [
        Case {
            name: "metadata",
            base: json!({"key": {"comment": "base"}}),
            current: json!({"key": {"comment": "current"}}),
            incoming: json!({"key": {"comment": "incoming"}}),
            pointer: "/strings/key/comment",
        },
        Case {
            name: "locale",
            base: json!({"key": {"localizations": {}}}),
            current: json!({"key": {"localizations": {"de": {"stringUnit": {"state": "translated", "value": "current"}}}}}),
            incoming: json!({"key": {"localizations": {"de": {"stringUnit": {"state": "translated", "value": "incoming"}}}}}),
            pointer: "/strings/key/localizations/de",
        },
        Case {
            name: "plural",
            base: json!({"key": {"localizations": {"de": {"variations": {"plural": {"one": {"stringUnit": {"state": "translated", "value": "base"}}}}}}}}),
            current: json!({"key": {"localizations": {"de": {"variations": {"plural": {"one": {"stringUnit": {"state": "translated", "value": "current"}}}}}}}}),
            incoming: json!({"key": {"localizations": {"de": {"variations": {"plural": {"one": {"stringUnit": {"state": "translated", "value": "incoming"}}}}}}}}),
            pointer: "/strings/key/localizations/de/variations/plural/one/stringUnit",
        },
        Case {
            name: "device",
            base: json!({"key": {"localizations": {"de": {"variations": {"device": {"iphone": {"stringUnit": {"state": "translated", "value": "base"}}}}}}}}),
            current: json!({"key": {"localizations": {"de": {"variations": {"device": {"iphone": {"stringUnit": {"state": "translated", "value": "current"}}}}}}}}),
            incoming: json!({"key": {"localizations": {"de": {"variations": {"device": {"iphone": {"stringUnit": {"state": "translated", "value": "incoming"}}}}}}}}),
            pointer: "/strings/key/localizations/de/variations/device/iphone/stringUnit",
        },
        Case {
            name: "substitution",
            base: json!({"key": {"localizations": {"de": {"substitutions": {"COUNT": {"argNum": 1, "formatSpecifier": "lld"}}}}}}),
            current: json!({"key": {"localizations": {"de": {"substitutions": {"COUNT": {"argNum": 2, "formatSpecifier": "lld"}}}}}}),
            incoming: json!({"key": {"localizations": {"de": {"substitutions": {"COUNT": {"argNum": 3, "formatSpecifier": "lld"}}}}}}),
            pointer: "/strings/key/localizations/de/substitutions/COUNT/argNum",
        },
        Case {
            name: "substitution plural",
            base: json!({"key": {"localizations": {"de": {"substitutions": {"COUNT": {"argNum": 1, "formatSpecifier": "lld", "variations": {"plural": {"one": {"stringUnit": {"state": "translated", "value": "%arg base"}}}}}}}}}}),
            current: json!({"key": {"localizations": {"de": {"substitutions": {"COUNT": {"argNum": 1, "formatSpecifier": "lld", "variations": {"plural": {"one": {"stringUnit": {"state": "translated", "value": "%arg current"}}}}}}}}}}),
            incoming: json!({"key": {"localizations": {"de": {"substitutions": {"COUNT": {"argNum": 1, "formatSpecifier": "lld", "variations": {"plural": {"one": {"stringUnit": {"state": "translated", "value": "%arg incoming"}}}}}}}}}}),
            pointer: "/strings/key/localizations/de/substitutions/COUNT/variations/plural/one/stringUnit",
        },
    ];

    for case in cases {
        let base = catalog("1.0", case.base);
        let current = catalog("1.0", case.current);
        let incoming = catalog("1.0", case.incoming);
        let prepared = prepare_merge(&base, &current, &incoming, &MergeOptions::default()).unwrap();
        assert_eq!(prepared.report.conflict_total, 1, "{}", case.name);
        assert_eq!(
            prepared.report.conflicts[0].pointer, case.pointer,
            "{}",
            case.name
        );
    }
}

#[test]
fn unknown_resolution_duplicate_resolution_and_invalid_pagination_fail_closed() {
    let base = catalog("1.0", json!({"key": {"comment": "base"}}));
    let current = catalog("1.0", json!({"key": {"comment": "current"}}));
    let incoming = catalog("1.0", json!({"key": {"comment": "incoming"}}));
    let unknown = ConflictResolution {
        conflict_id: "merge-v1:not-a-real-conflict".into(),
        choice: ConflictChoice::Current,
    };
    let error = prepare_merge(
        &base,
        &current,
        &incoming,
        &MergeOptions {
            resolutions: vec![unknown.clone()],
            ..MergeOptions::default()
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("unknown conflict"));
    let duplicate = prepare_merge(
        &base,
        &current,
        &incoming,
        &MergeOptions {
            resolutions: vec![unknown.clone(), unknown],
            ..MergeOptions::default()
        },
    )
    .unwrap_err();
    assert!(duplicate.to_string().contains("duplicate resolution"));
    for valid_limit in [1, 500] {
        let prepared = prepare_merge(
            &base,
            &current,
            &incoming,
            &MergeOptions {
                conflict_limit: valid_limit,
                ..MergeOptions::default()
            },
        )
        .unwrap();
        assert_eq!(prepared.report.conflict_limit, valid_limit);
        assert_eq!(prepared.report.conflict_total, 1);
    }
    for invalid_limit in [0, 501] {
        let error = prepare_merge(
            &base,
            &current,
            &incoming,
            &MergeOptions {
                conflict_limit: invalid_limit,
                ..MergeOptions::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("between 1 and 500"));
    }
}

#[test]
fn conflict_metadata_uses_rfc6901_pointer_key_locale_and_field() {
    let key = "a/b~c";
    let entry = |value: &str| {
        let mut strings = serde_json::Map::new();
        strings.insert(
            key.into(),
            json!({"localizations": {"de-AT": {
                "stringUnit": {"state": "translated", "value": value}
            }}}),
        );
        Value::Object(strings)
    };
    let base = catalog("1.0", entry("base"));
    let current = catalog("1.0", entry("current"));
    let incoming = catalog("1.0", entry("incoming"));
    let prepared = prepare_merge(&base, &current, &incoming, &MergeOptions::default()).unwrap();
    let conflict = &prepared.report.conflicts[0];
    assert_eq!(
        conflict.pointer,
        "/strings/a~1b~0c/localizations/de-AT/stringUnit"
    );
    assert_eq!(conflict.key.as_deref(), Some(key));
    assert_eq!(conflict.locale.as_deref(), Some("de-AT"));
    assert_eq!(conflict.field.as_deref(), Some("stringUnit"));
    assert_eq!(conflict.kind, "atomic_divergence");
}

#[test]
fn conflict_metadata_uses_schema_positions_when_keys_match_marker_names() {
    let key = "localizations";
    let locale = "strings/localizations~edge";
    let entry = |value: &str| {
        let mut localizations = serde_json::Map::new();
        localizations.insert(
            locale.into(),
            json!({"stringUnit": {"state": "translated", "value": value}}),
        );
        let mut strings = serde_json::Map::new();
        strings.insert(key.into(), json!({"localizations": localizations}));
        Value::Object(strings)
    };
    let base = catalog("1.0", entry("base"));
    let current = catalog("1.0", entry("current"));
    let incoming = catalog("1.0", entry("incoming"));
    let prepared = prepare_merge(&base, &current, &incoming, &MergeOptions::default()).unwrap();

    let conflict = &prepared.report.conflicts[0];
    assert_eq!(
        conflict.pointer,
        "/strings/localizations/localizations/strings~1localizations~0edge/stringUnit"
    );
    assert_eq!(conflict.key.as_deref(), Some(key));
    assert_eq!(conflict.locale.as_deref(), Some(locale));
    assert_eq!(conflict.field.as_deref(), Some("stringUnit"));
}

#[derive(Clone, Copy)]
enum KnownMapLevel {
    StringsEntry,
    LocalizationsMap,
    LocalizationEntry,
    VariationsMap,
    PluralMap,
    PluralEntry,
    DeviceMap,
    DeviceEntry,
    SubstitutionsMap,
    SubstitutionEntry,
    SubstitutionVariationsMap,
    SubstitutionPluralMap,
    SubstitutionPluralEntry,
}

fn level_node(level: KnownMapLevel, label: &str) -> Value {
    let unit = || json!({"stringUnit": {"state": "translated", "value": label}});
    let substitution = || {
        json!({
            "argNum": 1,
            "formatSpecifier": "lld",
            "future": label
        })
    };
    match level {
        KnownMapLevel::StringsEntry => json!({"comment": label}),
        KnownMapLevel::LocalizationsMap => json!({"de": unit()}),
        KnownMapLevel::LocalizationEntry
        | KnownMapLevel::PluralEntry
        | KnownMapLevel::DeviceEntry
        | KnownMapLevel::SubstitutionPluralEntry => unit(),
        KnownMapLevel::VariationsMap => json!({"plural": {"one": unit()}}),
        KnownMapLevel::PluralMap | KnownMapLevel::SubstitutionPluralMap => {
            json!({"one": unit()})
        }
        KnownMapLevel::DeviceMap => json!({"iphone": unit()}),
        KnownMapLevel::SubstitutionsMap => json!({"COUNT": substitution()}),
        KnownMapLevel::SubstitutionEntry => substitution(),
        KnownMapLevel::SubstitutionVariationsMap => json!({"plural": {"one": unit()}}),
    }
}

fn catalog_at_level(level: KnownMapLevel, label: Option<&str>) -> Vec<u8> {
    let node = label.map(|value| level_node(level, value));
    let strings = match level {
        KnownMapLevel::StringsEntry => node
            .map(|value| json!({"key": value}))
            .unwrap_or_else(|| json!({})),
        KnownMapLevel::LocalizationsMap => json!({"key": node
            .map(|value| json!({"localizations": value}))
            .unwrap_or_else(|| json!({}))}),
        KnownMapLevel::LocalizationEntry => {
            json!({"key": {"localizations": node.map(|value| json!({"de": value})).unwrap_or_else(|| json!({}))}})
        }
        KnownMapLevel::VariationsMap => json!({"key": {"localizations": {"de": node
            .map(|value| json!({"variations": value}))
            .unwrap_or_else(|| json!({}))}}}),
        KnownMapLevel::PluralMap => json!({"key": {"localizations": {"de": {
            "variations": node.map(|value| json!({"plural": value})).unwrap_or_else(|| json!({}))
        }}}}),
        KnownMapLevel::PluralEntry => json!({"key": {"localizations": {"de": {
            "variations": {"plural": node.map(|value| json!({"one": value})).unwrap_or_else(|| json!({}))}
        }}}}),
        KnownMapLevel::DeviceMap => json!({"key": {"localizations": {"de": {
            "variations": node.map(|value| json!({"device": value})).unwrap_or_else(|| json!({}))
        }}}}),
        KnownMapLevel::DeviceEntry => json!({"key": {"localizations": {"de": {
            "variations": {"device": node.map(|value| json!({"iphone": value})).unwrap_or_else(|| json!({}))}
        }}}}),
        KnownMapLevel::SubstitutionsMap => json!({"key": {"localizations": {"de": node
            .map(|value| json!({"substitutions": value}))
            .unwrap_or_else(|| json!({}))}}}),
        KnownMapLevel::SubstitutionEntry => json!({"key": {"localizations": {"de": {
            "substitutions": node.map(|value| json!({"COUNT": value})).unwrap_or_else(|| json!({}))
        }}}}),
        KnownMapLevel::SubstitutionVariationsMap => json!({"key": {"localizations": {"de": {
            "substitutions": {"COUNT": node
                .map(|value| json!({"argNum": 1, "formatSpecifier": "lld", "variations": value}))
                .unwrap_or_else(|| json!({"argNum": 1, "formatSpecifier": "lld"}))}
        }}}}),
        KnownMapLevel::SubstitutionPluralMap => json!({"key": {"localizations": {"de": {
            "substitutions": {"COUNT": {
                "argNum": 1,
                "formatSpecifier": "lld",
                "variations": node.map(|value| json!({"plural": value})).unwrap_or_else(|| json!({}))
            }}
        }}}}),
        KnownMapLevel::SubstitutionPluralEntry => json!({"key": {"localizations": {"de": {
            "substitutions": {"COUNT": {
                "argNum": 1,
                "formatSpecifier": "lld",
                "variations": {"plural": node.map(|value| json!({"one": value})).unwrap_or_else(|| json!({}))}
            }}
        }}}}),
    };
    catalog("1.0", strings)
}

fn result_node(result: &Value, level: KnownMapLevel) -> Option<&Value> {
    match level {
        KnownMapLevel::StringsEntry => result.pointer("/strings/key"),
        KnownMapLevel::LocalizationsMap => result.pointer("/strings/key/localizations"),
        KnownMapLevel::LocalizationEntry => result.pointer("/strings/key/localizations/de"),
        KnownMapLevel::VariationsMap => result.pointer("/strings/key/localizations/de/variations"),
        KnownMapLevel::PluralMap => {
            result.pointer("/strings/key/localizations/de/variations/plural")
        }
        KnownMapLevel::PluralEntry => {
            result.pointer("/strings/key/localizations/de/variations/plural/one")
        }
        KnownMapLevel::DeviceMap => {
            result.pointer("/strings/key/localizations/de/variations/device")
        }
        KnownMapLevel::DeviceEntry => {
            result.pointer("/strings/key/localizations/de/variations/device/iphone")
        }
        KnownMapLevel::SubstitutionsMap => {
            result.pointer("/strings/key/localizations/de/substitutions")
        }
        KnownMapLevel::SubstitutionEntry => {
            result.pointer("/strings/key/localizations/de/substitutions/COUNT")
        }
        KnownMapLevel::SubstitutionVariationsMap => {
            result.pointer("/strings/key/localizations/de/substitutions/COUNT/variations")
        }
        KnownMapLevel::SubstitutionPluralMap => {
            result.pointer("/strings/key/localizations/de/substitutions/COUNT/variations/plural")
        }
        KnownMapLevel::SubstitutionPluralEntry => result
            .pointer("/strings/key/localizations/de/substitutions/COUNT/variations/plural/one"),
    }
}

fn level_pointer(level: KnownMapLevel) -> &'static str {
    match level {
        KnownMapLevel::StringsEntry => "/strings/key",
        KnownMapLevel::LocalizationsMap => "/strings/key/localizations",
        KnownMapLevel::LocalizationEntry => "/strings/key/localizations/de",
        KnownMapLevel::VariationsMap => "/strings/key/localizations/de/variations",
        KnownMapLevel::PluralMap => "/strings/key/localizations/de/variations/plural",
        KnownMapLevel::PluralEntry => "/strings/key/localizations/de/variations/plural/one",
        KnownMapLevel::DeviceMap => "/strings/key/localizations/de/variations/device",
        KnownMapLevel::DeviceEntry => "/strings/key/localizations/de/variations/device/iphone",
        KnownMapLevel::SubstitutionsMap => "/strings/key/localizations/de/substitutions",
        KnownMapLevel::SubstitutionEntry => "/strings/key/localizations/de/substitutions/COUNT",
        KnownMapLevel::SubstitutionVariationsMap => {
            "/strings/key/localizations/de/substitutions/COUNT/variations"
        }
        KnownMapLevel::SubstitutionPluralMap => {
            "/strings/key/localizations/de/substitutions/COUNT/variations/plural"
        }
        KnownMapLevel::SubstitutionPluralEntry => {
            "/strings/key/localizations/de/substitutions/COUNT/variations/plural/one"
        }
    }
}

fn divergent_leaf_pointer(level: KnownMapLevel) -> &'static str {
    match level {
        KnownMapLevel::StringsEntry => "/strings/key/comment",
        KnownMapLevel::LocalizationsMap | KnownMapLevel::LocalizationEntry => {
            "/strings/key/localizations/de/stringUnit"
        }
        KnownMapLevel::VariationsMap | KnownMapLevel::PluralMap | KnownMapLevel::PluralEntry => {
            "/strings/key/localizations/de/variations/plural/one/stringUnit"
        }
        KnownMapLevel::DeviceMap | KnownMapLevel::DeviceEntry => {
            "/strings/key/localizations/de/variations/device/iphone/stringUnit"
        }
        KnownMapLevel::SubstitutionsMap | KnownMapLevel::SubstitutionEntry => {
            "/strings/key/localizations/de/substitutions/COUNT/future"
        }
        KnownMapLevel::SubstitutionVariationsMap
        | KnownMapLevel::SubstitutionPluralMap
        | KnownMapLevel::SubstitutionPluralEntry => {
            "/strings/key/localizations/de/substitutions/COUNT/variations/plural/one/stringUnit"
        }
    }
}

fn report_with_sides_swapped(mut report: MergeReport) -> MergeReport {
    std::mem::swap(
        &mut report.fingerprints.current,
        &mut report.fingerprints.incoming,
    );
    std::mem::swap(
        &mut report.auto_applied.current,
        &mut report.auto_applied.incoming,
    );
    if let Some(expected) = &mut report.expected_fingerprints {
        std::mem::swap(&mut expected.current, &mut expected.incoming);
    }
    for conflict in &mut report.conflicts {
        std::mem::swap(&mut conflict.current, &mut conflict.incoming);
    }
    report
}

#[test]
fn complete_add_delete_modify_matrix_holds_at_every_known_map_level() {
    struct Case {
        name: &'static str,
        base: Option<&'static str>,
        current: Option<&'static str>,
        incoming: Option<&'static str>,
        expected: Option<&'static str>,
        conflicts: usize,
        conflict_kind: Option<&'static str>,
        conflict_at_target: bool,
        auto_current: usize,
        auto_incoming: usize,
    }
    let cases = [
        Case {
            name: "absent everywhere",
            base: None,
            current: None,
            incoming: None,
            expected: None,
            conflicts: 0,
            conflict_kind: None,
            conflict_at_target: false,
            auto_current: 0,
            auto_incoming: 0,
        },
        Case {
            name: "current-only add",
            base: None,
            current: Some("current"),
            incoming: None,
            expected: Some("current"),
            conflicts: 0,
            conflict_kind: None,
            conflict_at_target: false,
            auto_current: 1,
            auto_incoming: 0,
        },
        Case {
            name: "incoming-only add",
            base: None,
            current: None,
            incoming: Some("incoming"),
            expected: Some("incoming"),
            conflicts: 0,
            conflict_kind: None,
            conflict_at_target: false,
            auto_current: 0,
            auto_incoming: 1,
        },
        Case {
            name: "unchanged",
            base: Some("base"),
            current: Some("base"),
            incoming: Some("base"),
            expected: Some("base"),
            conflicts: 0,
            conflict_kind: None,
            conflict_at_target: false,
            auto_current: 0,
            auto_incoming: 0,
        },
        Case {
            name: "current modify",
            base: Some("base"),
            current: Some("current"),
            incoming: Some("base"),
            expected: Some("current"),
            conflicts: 0,
            conflict_kind: None,
            conflict_at_target: false,
            auto_current: 1,
            auto_incoming: 0,
        },
        Case {
            name: "incoming modify",
            base: Some("base"),
            current: Some("base"),
            incoming: Some("incoming"),
            expected: Some("incoming"),
            conflicts: 0,
            conflict_kind: None,
            conflict_at_target: false,
            auto_current: 0,
            auto_incoming: 1,
        },
        Case {
            name: "identical double modify",
            base: Some("base"),
            current: Some("same"),
            incoming: Some("same"),
            expected: Some("same"),
            conflicts: 0,
            conflict_kind: None,
            conflict_at_target: false,
            auto_current: 0,
            auto_incoming: 0,
        },
        Case {
            name: "divergent double modify",
            base: Some("base"),
            current: Some("current"),
            incoming: Some("incoming"),
            expected: Some("base"),
            conflicts: 1,
            conflict_kind: Some("atomic_divergence"),
            conflict_at_target: false,
            auto_current: 0,
            auto_incoming: 0,
        },
        Case {
            name: "identical double add",
            base: None,
            current: Some("same"),
            incoming: Some("same"),
            expected: Some("same"),
            conflicts: 0,
            conflict_kind: None,
            conflict_at_target: false,
            auto_current: 0,
            auto_incoming: 0,
        },
        Case {
            name: "divergent double add",
            base: None,
            current: Some("current"),
            incoming: Some("incoming"),
            expected: None,
            conflicts: 1,
            conflict_kind: Some("divergent_add"),
            conflict_at_target: true,
            auto_current: 0,
            auto_incoming: 0,
        },
        Case {
            name: "double delete",
            base: Some("base"),
            current: None,
            incoming: None,
            expected: None,
            conflicts: 0,
            conflict_kind: None,
            conflict_at_target: false,
            auto_current: 0,
            auto_incoming: 0,
        },
        Case {
            name: "current delete unchanged",
            base: Some("base"),
            current: None,
            incoming: Some("base"),
            expected: None,
            conflicts: 0,
            conflict_kind: None,
            conflict_at_target: false,
            auto_current: 1,
            auto_incoming: 0,
        },
        Case {
            name: "incoming delete unchanged",
            base: Some("base"),
            current: Some("base"),
            incoming: None,
            expected: None,
            conflicts: 0,
            conflict_kind: None,
            conflict_at_target: false,
            auto_current: 0,
            auto_incoming: 1,
        },
        Case {
            name: "current delete incoming modify",
            base: Some("base"),
            current: None,
            incoming: Some("incoming"),
            expected: Some("base"),
            conflicts: 1,
            conflict_kind: Some("delete_modify"),
            conflict_at_target: true,
            auto_current: 0,
            auto_incoming: 0,
        },
        Case {
            name: "incoming delete current modify",
            base: Some("base"),
            current: Some("current"),
            incoming: None,
            expected: Some("base"),
            conflicts: 1,
            conflict_kind: Some("delete_modify"),
            conflict_at_target: true,
            auto_current: 0,
            auto_incoming: 0,
        },
    ];
    let levels = [
        KnownMapLevel::StringsEntry,
        KnownMapLevel::LocalizationsMap,
        KnownMapLevel::LocalizationEntry,
        KnownMapLevel::VariationsMap,
        KnownMapLevel::PluralMap,
        KnownMapLevel::PluralEntry,
        KnownMapLevel::DeviceMap,
        KnownMapLevel::DeviceEntry,
        KnownMapLevel::SubstitutionsMap,
        KnownMapLevel::SubstitutionEntry,
        KnownMapLevel::SubstitutionVariationsMap,
        KnownMapLevel::SubstitutionPluralMap,
        KnownMapLevel::SubstitutionPluralEntry,
    ];

    for level in levels {
        for case in &cases {
            let base = catalog_at_level(level, case.base);
            let current = catalog_at_level(level, case.current);
            let incoming = catalog_at_level(level, case.incoming);
            let prepared =
                prepare_merge(&base, &current, &incoming, &MergeOptions::default()).unwrap();
            assert_eq!(
                prepared.report.conflict_total, case.conflicts,
                "{}",
                case.name
            );
            assert_eq!(
                prepared.report.auto_applied.current, case.auto_current,
                "{}",
                case.name
            );
            assert_eq!(
                prepared.report.auto_applied.incoming, case.auto_incoming,
                "{}",
                case.name
            );
            if let Some(conflict) = prepared.report.conflicts.first() {
                assert!(
                    conflict.pointer.starts_with(level_pointer(level)),
                    "{}: {}",
                    case.name,
                    conflict.pointer
                );
                if let Some(kind) = case.conflict_kind {
                    let expected_pointer = if case.conflict_at_target {
                        level_pointer(level)
                    } else {
                        divergent_leaf_pointer(level)
                    };
                    assert_eq!(conflict.pointer, expected_pointer, "{}", case.name);
                    assert_eq!(conflict.kind, kind, "{}", case.name);
                }
            }
            let result: Value = serde_json::from_str(&prepared.content).unwrap();
            assert_eq!(
                result_node(&result, level),
                case.expected.map(|label| level_node(level, label)).as_ref(),
                "{}",
                case.name
            );
        }
    }
}

proptest! {
    #[test]
    fn identity_base_sides_determinism_and_idempotency_hold(
        base_comment in "[a-z]{0,16}",
        current_comment in "[a-z]{0,16}",
        incoming_comment in "[a-z]{0,16}",
    ) {
        let base = catalog("1.0", json!({"key": {"comment": base_comment}}));
        let current = catalog("1.0", json!({"key": {"comment": current_comment}}));
        let incoming = catalog("1.0", json!({"key": {"comment": incoming_comment}}));

        let identity = prepare_merge(&base, &base, &base, &MergeOptions::default()).unwrap();
        let identity_value: Value = serde_json::from_str(&identity.content).unwrap();
        prop_assert_eq!(identity_value["strings"]["key"]["comment"].as_str(), Some(base_comment.as_str()));

        let left = prepare_merge(&base, &base, &incoming, &MergeOptions::default()).unwrap();
        let left_value: Value = serde_json::from_str(&left.content).unwrap();
        prop_assert_eq!(left_value["strings"]["key"]["comment"].as_str(), Some(incoming_comment.as_str()));
        let right = prepare_merge(&base, &current, &base, &MergeOptions::default()).unwrap();
        let right_value: Value = serde_json::from_str(&right.content).unwrap();
        prop_assert_eq!(right_value["strings"]["key"]["comment"].as_str(), Some(current_comment.as_str()));

        let first = prepare_merge(&base, &current, &incoming, &MergeOptions::default()).unwrap();
        let second = prepare_merge(&base, &current, &incoming, &MergeOptions::default()).unwrap();
        prop_assert_eq!(&first.content, &second.content);
        prop_assert_eq!(serde_json::to_value(&first.report).unwrap(), serde_json::to_value(&second.report).unwrap());
        let repeated = prepare_merge(first.content.as_bytes(), first.content.as_bytes(), first.content.as_bytes(), &MergeOptions::default()).unwrap();
        prop_assert_eq!(serde_json::from_str::<Value>(&repeated.content).unwrap(), serde_json::from_str::<Value>(&first.content).unwrap());

        let result: Value = serde_json::from_str(&first.content).unwrap();
        let result_comment = result["strings"]["key"]["comment"].as_str().unwrap();
        prop_assert!(result_comment == base_comment || result_comment == current_comment || result_comment == incoming_comment);
    }

    #[test]
    fn swapping_sides_swaps_reports_without_changing_unresolved_result(
        seed in "[a-z]{1,8}",
    ) {
        let base_comment = format!("base-{seed}");
        for scenario in 0u8..4 {
            let current_change = format!("current-{seed}");
            let incoming_change = format!("incoming-{seed}");
            let (current_comment, incoming_comment) = match scenario {
                0 => (base_comment.clone(), incoming_change),
                1 => (current_change, base_comment.clone()),
                2 => (current_change, incoming_change),
                _ => (current_change.clone(), current_change),
            };
            let base = catalog("1.0", json!({"key": {"comment": base_comment.clone()}}));
            let current = catalog("1.0", json!({"key": {"comment": current_comment}}));
            let incoming = catalog("1.0", json!({"key": {"comment": incoming_comment}}));
            let forward = prepare_merge(&base, &current, &incoming, &MergeOptions::default()).unwrap();
            let reverse = prepare_merge(&base, &incoming, &current, &MergeOptions::default()).unwrap();
            prop_assert_eq!(&forward.content, &reverse.content);
            let swapped_forward = report_with_sides_swapped(forward.report);
            prop_assert_eq!(
                serde_json::to_value(swapped_forward).unwrap(),
                serde_json::to_value(reverse.report).unwrap()
            );
        }
    }

    #[test]
fn key_count_never_drops_without_a_base_proven_deletion(
        base_count in 0usize..20,
        current_additions in 0usize..10,
        incoming_additions in 0usize..10,
        current_delete_stride in 1usize..6,
        incoming_delete_stride in 1usize..6,
    ) {
        let base_strings = (0..base_count)
            .map(|index| (format!("base.{index}"), json!({"comment": "base"})))
            .collect::<serde_json::Map<_, _>>();
        let mut current_strings = base_strings
            .iter()
            .filter(|(key, _)| {
                key.rsplit('.').next().unwrap().parse::<usize>().unwrap()
                    % current_delete_stride
                    != 0
            })
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<serde_json::Map<_, _>>();
        current_strings.extend((0..current_additions).map(|index| {
            (format!("current.{index}"), json!({"comment": "current"}))
        }));
        let mut incoming_strings = base_strings
            .iter()
            .filter(|(key, _)| {
                key.rsplit('.').next().unwrap().parse::<usize>().unwrap()
                    % incoming_delete_stride
                    != 0
            })
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<serde_json::Map<_, _>>();
        incoming_strings.extend((0..incoming_additions).map(|index| {
            (format!("incoming.{index}"), json!({"comment": "incoming"}))
        }));
        let base_survivors = (0..base_count)
            .filter(|index| {
                index % current_delete_stride != 0 && index % incoming_delete_stride != 0
            })
            .count();
        let base = catalog("1.0", Value::Object(base_strings));
        let current = catalog("1.0", Value::Object(current_strings));
        let incoming = catalog("1.0", Value::Object(incoming_strings));
        let merged = prepare_merge(&base, &current, &incoming, &MergeOptions::default()).unwrap();
        let merged_value: Value = serde_json::from_str(&merged.content).unwrap();
        let merged_strings = merged_value["strings"].as_object().unwrap();
        prop_assert_eq!(merged.report.conflict_total, 0);
        prop_assert_eq!(
            merged.report.fingerprints.result.key_count,
            merged_strings.len()
        );
        prop_assert_eq!(
            merged.report.fingerprints.result.key_count,
            base_survivors + current_additions + incoming_additions
        );
        for index in 0..base_count {
            let proven_deleted =
                index % current_delete_stride == 0 || index % incoming_delete_stride == 0;
            prop_assert_eq!(
                merged_strings.contains_key(&format!("base.{index}")),
                !proven_deleted
            );
        }
    }
}

#[test]
fn unused_resolution_error_is_deterministic_in_request_order() {
    let catalog = catalog("1.0", json!({"key": {}}));
    let options = MergeOptions {
        resolutions: vec![
            ConflictResolution {
                conflict_id: "unknown-first".into(),
                choice: ConflictChoice::Current,
            },
            ConflictResolution {
                conflict_id: "unknown-second".into(),
                choice: ConflictChoice::Incoming,
            },
        ],
        ..MergeOptions::default()
    };

    for _ in 0..100 {
        let error = prepare_merge(&catalog, &catalog, &catalog, &options).unwrap_err();
        assert_eq!(
            error.to_string(),
            "invalid format: resolution references unknown conflict unknown-first"
        );
    }
}
