use std::collections::BTreeMap;

use indexmap::IndexMap;
use proptest::prelude::*;
use xcstrings_mcp::model::specifier::{analyze_format, compare_formats};
use xcstrings_mcp::model::translation::CompletedTranslation;
use xcstrings_mcp::model::xcstrings::{
    Localization, PluralVariation, StringEntry, StringUnit, TranslationState, Variations,
    XcStringsFile,
};
use xcstrings_mcp::service::{extractor, file_validator, validator, xliff};

#[path = "support/modifier_oracle.rs"]
mod modifier_oracle;

fn simple_entry(source: &str, target: Option<(&str, &str)>) -> StringEntry {
    let mut localizations = IndexMap::new();
    localizations.insert(
        "en".to_string(),
        Localization {
            string_unit: Some(StringUnit {
                state: TranslationState::Translated,
                value: source.to_string(),
            }),
            variations: None,
            substitutions: None,
        },
    );
    if let Some((locale, value)) = target {
        localizations.insert(
            locale.to_string(),
            Localization {
                string_unit: Some(StringUnit {
                    state: TranslationState::Translated,
                    value: value.to_string(),
                }),
                variations: None,
                substitutions: None,
            },
        );
    }
    StringEntry {
        extraction_state: None,
        should_translate: true,
        comment: None,
        localizations: Some(localizations),
    }
}

fn file_with(key: &str, entry: StringEntry) -> XcStringsFile {
    XcStringsFile {
        source_language: "en".to_string(),
        strings: IndexMap::from([(key.to_string(), entry)]),
        version: "1.0".to_string(),
    }
}

fn translation(key: &str, value: &str) -> CompletedTranslation {
    CompletedTranslation {
        key: key.to_string(),
        locale: "de".to_string(),
        value: value.to_string(),
        plural_forms: None,
        substitution_name: None,
    }
}

#[test]
fn extractor_and_search_advertise_only_definite_arguments() {
    let source = "100% Local Storage: %@";
    let file = file_with("storage_usage", simple_entry(source, None));

    let (untranslated, untranslated_total) =
        extractor::get_untranslated(&file, "de", 10, 0).expect("extract untranslated");
    let (search_results, search_total) =
        extractor::search_keys(&file, "storage", "de", 10, 0).expect("search keys");

    assert_eq!(untranslated_total, 1);
    assert_eq!(untranslated[0].source_text, source);
    assert_eq!(untranslated[0].format_specifiers, ["%@"]);
    assert_eq!(search_total, 1);
    assert_eq!(search_results[0].source_text, source);
    assert_eq!(search_results[0].format_specifiers, ["%@"]);
}

#[test]
fn classifies_definite_ambiguous_and_literal_percent_sequences() {
    struct Case {
        input: &'static str,
        arguments: &'static [&'static str],
        ambiguous: &'static [&'static str],
        literals: &'static [&'static str],
        problems: &'static [(&'static str, &'static str)],
    }

    let cases = [
        Case {
            input: "%@ %d %lld %1$@",
            arguments: &["%@", "%d", "%lld", "%1$@"],
            ambiguous: &[],
            literals: &[],
            problems: &[],
        },
        Case {
            input: "100% Local Storage",
            arguments: &[],
            ambiguous: &["% Lo"],
            literals: &[],
            problems: &[],
        },
        Case {
            input: "100% lokaler Speicher",
            arguments: &[],
            ambiguous: &["% lo"],
            literals: &[],
            problems: &[],
        },
        Case {
            input: "You've logged 85% of your goal",
            arguments: &[],
            ambiguous: &["% o"],
            literals: &[],
            problems: &[],
        },
        Case {
            input: "7.0-8.0% - Acceptable",
            arguments: &[],
            ambiguous: &["% - A"],
            literals: &[],
            problems: &[],
        },
        Case {
            input: "%days %safe %20days % direct",
            arguments: &[],
            ambiguous: &["%d", "%s", "%20d", "% d"],
            literals: &[],
            problems: &[],
        },
        Case {
            input: "%% % %?",
            arguments: &[],
            ambiguous: &[],
            literals: &["%%", "%", "%"],
            problems: &[],
        },
        Case {
            input: "%%%d %%%%%@",
            arguments: &["%d", "%@"],
            ambiguous: &[],
            literals: &["%%", "%%", "%%"],
            problems: &[],
        },
        Case {
            input: "(%@), %08.2f!",
            arguments: &["%@", "%08.2f"],
            ambiguous: &[],
            literals: &[],
            problems: &[],
        },
        Case {
            input: "%d日 %lld日 %2$lld日",
            arguments: &["%d", "%lld", "%2$lld"],
            ambiguous: &[],
            literals: &[],
            problems: &[],
        },
        Case {
            input: "%dꥠ %d𰀀 %dé %dβ",
            arguments: &["%d", "%d"],
            ambiguous: &["%d", "%d"],
            literals: &[],
            problems: &[],
        },
        Case {
            input: "%Ld",
            arguments: &[],
            ambiguous: &[],
            literals: &[],
            problems: &[("invalid_modifier_conversion", "%Ld")],
        },
    ];

    for case in cases {
        let analysis = analyze_format(case.input);
        assert_eq!(
            analysis
                .arguments
                .iter()
                .map(|value| value.raw.as_str())
                .collect::<Vec<_>>(),
            case.arguments,
            "definite arguments for {:?}",
            case.input
        );
        assert_eq!(
            analysis
                .ambiguous
                .iter()
                .map(|value| value.raw.as_str())
                .collect::<Vec<_>>(),
            case.ambiguous,
            "ambiguous sequences for {:?}",
            case.input
        );
        assert_eq!(
            analysis
                .literals
                .iter()
                .map(|value| value.raw.as_str())
                .collect::<Vec<_>>(),
            case.literals,
            "literal sequences for {:?}",
            case.input
        );
        assert_eq!(
            analysis
                .problems
                .iter()
                .map(|value| (value.code, value.raw.as_str()))
                .collect::<Vec<_>>(),
            case.problems,
            "invalid sequences for {:?}",
            case.input
        );
    }
}

#[test]
fn models_each_foundation_argument_component() {
    let analysis = analyze_format("%3$-+#08.4lld");
    assert!(analysis.problems.is_empty());
    assert_eq!(analysis.arguments.len(), 1);
    let argument = &analysis.arguments[0];
    assert_eq!(argument.position, Some(3));
    assert_eq!(argument.flags, "-+#0");
    assert_eq!(argument.width.as_deref(), Some("8"));
    assert_eq!(argument.precision.as_deref(), Some(".4"));
    assert_eq!(argument.length_modifier.as_deref(), Some("ll"));
    assert_eq!(argument.conversion, 'd');
}

#[test]
fn foundation_j_modifier_is_integer_only_and_remains_definite() {
    let accepted = analyze_format("%jd %ju %jx %2$jd𰀀");
    assert_eq!(
        accepted
            .arguments
            .iter()
            .map(|argument| (
                argument.raw.as_str(),
                argument.position,
                argument.length_modifier.as_deref(),
                argument.conversion,
            ))
            .collect::<Vec<_>>(),
        [
            ("%jd", None, Some("j"), 'd'),
            ("%ju", None, Some("j"), 'u'),
            ("%jx", None, Some("j"), 'x'),
            ("%2$jd", Some(2), Some("j"), 'd'),
        ]
    );
    assert!(accepted.ambiguous.is_empty());
    assert!(accepted.problems.is_empty());

    let invalid = analyze_format("%jf");
    assert!(invalid.arguments.is_empty());
    assert_eq!(
        invalid
            .problems
            .iter()
            .map(|problem| (problem.code, problem.raw.as_str()))
            .collect::<Vec<_>>(),
        [("invalid_modifier_conversion", "%jf")]
    );

    assert_eq!(
        compare_formats("%jd items", "items")
            .errors
            .iter()
            .map(|issue| issue.code)
            .collect::<Vec<_>>(),
        ["format_specifier_count_mismatch"]
    );
    assert_eq!(
        compare_formats("%jd items", "%jf items")
            .errors
            .iter()
            .map(|issue| issue.code)
            .collect::<Vec<_>>(),
        ["invalid_format_specifier"]
    );
}

#[test]
fn foundation_modifier_oracle_accepts_all_72_valid_pairs() {
    for token in modifier_oracle::VALID_CASES {
        let analysis = analyze_format(token);
        assert_eq!(
            analysis
                .arguments
                .iter()
                .map(|argument| argument.raw.as_str())
                .collect::<Vec<_>>(),
            [token],
            "{token:?}"
        );
        assert!(analysis.ambiguous.is_empty(), "{token:?}");
        assert!(analysis.literals.is_empty(), "{token:?}");
        assert!(analysis.problems.is_empty(), "{token:?}");

        let comparison = compare_formats(token, token);
        assert!(comparison.errors.is_empty(), "{token:?}");
        assert!(comparison.warnings.is_empty(), "{token:?}");
    }
}

#[test]
fn foundation_modifier_oracle_rejects_all_168_invalid_pairs() {
    for token in modifier_oracle::INVALID_CASES {
        let analysis = analyze_format(token);
        assert!(analysis.arguments.is_empty(), "{token:?}");
        assert!(analysis.ambiguous.is_empty(), "{token:?}");
        assert!(analysis.literals.is_empty(), "{token:?}");
        assert_eq!(
            analysis
                .problems
                .iter()
                .map(|problem| (problem.code, problem.raw.as_str()))
                .collect::<Vec<_>>(),
            [("invalid_modifier_conversion", token)],
            "{token:?}"
        );

        let comparison = compare_formats(token, token);
        assert_eq!(
            comparison
                .errors
                .iter()
                .map(|issue| issue.code)
                .collect::<Vec<_>>(),
            ["invalid_format_specifier", "invalid_format_specifier"],
            "{token:?}"
        );
        assert!(comparison.warnings.is_empty(), "{token:?}");
    }
}

#[test]
fn shared_validation_matches_all_240_modifier_oracle_cases() {
    for token in modifier_oracle::VALID_CASES {
        let source = format!("{token} items");
        let target = format!("{token} Artikel");
        let file = file_with("items", simple_entry(&source, Some(("de", &target))));

        let submitted =
            validator::validate_translations_detailed(&file, &[translation("items", &target)]);
        assert!(submitted.rejected.is_empty(), "{token:?}");
        assert!(submitted.warnings.is_empty(), "{token:?}");

        let reports = file_validator::validate_file(&file, Some("de"));
        assert!(reports[0].errors.is_empty(), "{token:?}");
        assert!(reports[0].warnings.is_empty(), "{token:?}");
    }

    for token in modifier_oracle::INVALID_CASES {
        let source = format!("{token} items");
        let target = format!("{token} Artikel");
        let file = file_with("items", simple_entry(&source, Some(("de", &target))));

        let submitted =
            validator::validate_translations_detailed(&file, &[translation("items", &target)]);
        assert_eq!(submitted.rejected.len(), 2, "{token:?}");
        assert!(submitted.rejected.iter().all(|rejection| {
            rejection
                .reason
                .contains(&format!("invalid format sequence {token}"))
        }));
        assert!(submitted.warnings.is_empty(), "{token:?}");

        let reports = file_validator::validate_file(&file, Some("de"));
        assert_eq!(
            format_issue_codes(&reports[0].errors),
            ["invalid_format_specifier", "invalid_format_specifier"],
            "{token:?}"
        );
        assert!(reports[0].warnings.is_empty(), "{token:?}");
    }
}

#[test]
fn positional_argument_bounds_are_explicit_and_overflow_blocks() {
    let zero = analyze_format("%0$d");
    assert!(zero.arguments.is_empty());
    assert_eq!(
        zero.problems
            .iter()
            .map(|problem| (problem.code, problem.raw.as_str()))
            .collect::<Vec<_>>(),
        [("invalid_positional_argument", "%0$d")]
    );

    let maximum = analyze_format("%4294967295$d");
    assert_eq!(maximum.arguments.len(), 1);
    assert_eq!(maximum.arguments[0].position, Some(u32::MAX));
    assert!(maximum.problems.is_empty());

    for token in [
        "%4294967296$d",
        "%2$*0$d",
        "%2$*4294967296$d",
        "%2$.*0$d",
        "%2$.*4294967296$d",
    ] {
        let analysis = analyze_format(token);
        assert!(analysis.arguments.is_empty(), "{token:?}");
        assert_eq!(
            analysis
                .problems
                .iter()
                .map(|problem| (problem.code, problem.raw.as_str()))
                .collect::<Vec<_>>(),
            [("invalid_positional_argument", token)],
            "{token:?}"
        );
    }

    for token in ["%2$*4294967295$d", "%2$.*4294967295$d"] {
        let analysis = analyze_format(token);
        assert_eq!(analysis.arguments.len(), 1, "{token:?}");
        assert!(analysis.problems.is_empty(), "{token:?}");
    }

    assert_eq!(
        compare_formats("%d", "%4294967296$d")
            .errors
            .iter()
            .map(|issue| issue.code)
            .collect::<Vec<_>>(),
        ["invalid_positional_argument"]
    );
}

#[test]
fn span_view_includes_arguments_and_invalid_sequences_at_unicode_boundaries() {
    let text = "é%d — %Ld";
    let analysis = analyze_format(text);
    let spans = analysis.spans();
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].raw, "%d");
    assert_eq!(spans[1].raw, "%Ld");
    assert_eq!(analysis.arguments.len(), 1);
    assert!(analysis.ambiguous.is_empty());
    assert!(analysis.literals.is_empty());
    assert_eq!(analysis.problems.len(), 1);
    assert_eq!(analysis.problems[0].code, "invalid_modifier_conversion");
    for span in spans {
        assert!(text.is_char_boundary(span.start));
        assert!(text.is_char_boundary(span.end));
    }
}

#[test]
fn comparator_allows_only_valid_logical_argument_reorders() {
    let valid = compare_formats("%@ has %08.2f", "%2$08.2f : %1$@");
    assert!(valid.errors.is_empty(), "{:?}", valid.errors);

    let duplicate = compare_formats("%@ %d", "%1$@ %1$d");
    assert_eq!(
        duplicate
            .errors
            .iter()
            .map(|issue| issue.code)
            .collect::<Vec<_>>(),
        ["duplicate_positional_argument"]
    );

    let missing = compare_formats("%@ %d", "%1$@ %3$d");
    assert_eq!(
        missing
            .errors
            .iter()
            .map(|issue| issue.code)
            .collect::<Vec<_>>(),
        ["missing_positional_argument"]
    );

    let mixed = compare_formats("%@ %d", "%1$@ %d");
    assert_eq!(
        mixed
            .errors
            .iter()
            .map(|issue| issue.code)
            .collect::<Vec<_>>(),
        ["mixed_positional_arguments"]
    );
}

#[test]
fn comparator_preserves_flags_width_precision_conversion_and_modifier() {
    let cases = [
        ("%+08.2f", "%08.2f"),
        ("%+08.2f", "%+09.2f"),
        ("%+08.2f", "%+08.3f"),
        ("%+08.2f", "%+08.2e"),
        ("%lld", "%ld"),
        ("%lld", "%Ld"),
    ];
    for (source, target) in cases {
        let result = compare_formats(source, target);
        assert_eq!(
            result
                .errors
                .iter()
                .map(|issue| issue.code)
                .collect::<Vec<_>>(),
            if target == "%Ld" {
                vec!["invalid_format_specifier"]
            } else {
                vec!["format_specifier_type_mismatch"]
            },
            "{source:?} vs {target:?}"
        );
    }
}

#[test]
fn comparator_preserves_dynamic_width_and_precision_positions() {
    let reordered = compare_formats("%*.*f", "%3$*1$.*2$f");
    assert!(reordered.errors.is_empty(), "{:?}", reordered.errors);

    let swapped_components = compare_formats("%*.*f", "%3$*2$.*1$f");
    assert_eq!(
        swapped_components
            .errors
            .iter()
            .map(|issue| issue.code)
            .collect::<Vec<_>>(),
        ["format_specifier_type_mismatch"]
    );
}

#[test]
fn cjk_adjacent_arguments_stay_definite_and_block_real_mismatches() {
    let valid_cases = [
        ("%d days", "%d日"),
        ("%d days", "%dꥠ"),
        ("%d days", "%d𰀀"),
        ("%lld days", "%lld日"),
        ("%@ • %lld days", "%1$@ • %2$lld日"),
    ];
    for (source, target) in valid_cases {
        let comparison = compare_formats(source, target);
        assert!(comparison.errors.is_empty(), "{source:?} vs {target:?}");
        assert!(comparison.warnings.is_empty(), "{source:?} vs {target:?}");
    }

    let missing = compare_formats("%lld days", "残り日");
    assert_eq!(
        missing
            .errors
            .iter()
            .map(|issue| issue.code)
            .collect::<Vec<_>>(),
        ["format_specifier_count_mismatch"]
    );
    let wrong = compare_formats("%lld days", "%d日");
    assert_eq!(
        wrong
            .errors
            .iter()
            .map(|issue| issue.code)
            .collect::<Vec<_>>(),
        ["format_specifier_type_mismatch"]
    );

    let valid_file = file_with("days", simple_entry("%lld days", Some(("de", "%lld日"))));
    let valid_report = file_validator::validate_file(&valid_file, Some("de"));
    assert!(valid_report[0].errors.is_empty());
    assert!(
        valid_report[0]
            .warnings
            .iter()
            .all(|issue| issue.issue_type != "ambiguous_format_sequence_mismatch")
    );

    let missing_file = file_with("days", simple_entry("%lld days", Some(("de", "残り日"))));
    let missing_report = file_validator::validate_file(&missing_file, Some("de"));
    assert_eq!(
        format_issue_codes(&missing_report[0].errors),
        ["format_specifier_count_mismatch"]
    );
    let wrong_file = file_with("days", simple_entry("%lld days", Some(("de", "%d日"))));
    let wrong_report = file_validator::validate_file(&wrong_file, Some("de"));
    assert_eq!(
        format_issue_codes(&wrong_report[0].errors),
        ["format_specifier_type_mismatch"]
    );
}

#[test]
fn ambiguous_mismatches_warn_but_do_not_block() {
    let result = compare_formats("100% Local Storage", "100% lokaler Speicher");
    assert!(result.errors.is_empty());
    assert_eq!(result.warnings.len(), 1);
    assert_eq!(
        result.warnings[0].code,
        "ambiguous_format_sequence_mismatch"
    );
}

#[test]
fn submit_and_file_validation_share_prose_and_mismatch_outcomes() {
    let cases = [
        ("storage", "100% Local Storage", "100% lokaler Speicher"),
        (
            "progress",
            "You've logged 85% of your goal",
            "Du hast 85% deines Ziels erreicht",
        ),
        ("range", "7.0-8.0% - Acceptable", "7,0-8,0% - vertretbar"),
    ];

    for (key, source, target) in cases {
        let file = file_with(key, simple_entry(source, Some(("de", target))));
        let detailed =
            validator::validate_translations_detailed(&file, &[translation(key, target)]);
        assert!(
            detailed.rejected.is_empty(),
            "{key}: {:?}",
            detailed.rejected
        );
        assert_eq!(
            detailed
                .warnings
                .iter()
                .filter(|issue| issue.issue_type == "ambiguous_format_sequence_mismatch")
                .map(|issue| issue.issue_type.as_str())
                .collect::<Vec<_>>(),
            ["ambiguous_format_sequence_mismatch"],
            "{key}: {:?}",
            detailed.warnings
        );

        let reports = file_validator::validate_file(&file, Some("de"));
        assert!(
            reports[0].errors.is_empty(),
            "{key}: {:?}",
            reports[0].errors
        );
        assert_eq!(
            reports[0]
                .warnings
                .iter()
                .filter(|issue| issue.issue_type == "ambiguous_format_sequence_mismatch")
                .map(|issue| issue.issue_type.as_str())
                .collect::<Vec<_>>(),
            ["ambiguous_format_sequence_mismatch"],
            "{key}: {:?}",
            reports[0].warnings
        );
    }

    let file = file_with(
        "count",
        simple_entry("%lld files", Some(("de", "%Ld Dateien"))),
    );
    let detailed =
        validator::validate_translations_detailed(&file, &[translation("count", "%Ld Dateien")]);
    assert_eq!(detailed.rejected.len(), 1);
    assert!(
        detailed.rejected[0]
            .reason
            .contains("invalid format sequence %Ld")
    );
    let reports = file_validator::validate_file(&file, Some("de"));
    assert_eq!(
        format_issue_codes(&reports[0].errors),
        ["invalid_format_specifier"]
    );
}

#[test]
fn shared_source_resolver_handles_key_fallback_and_matching_plural_forms() {
    let fallback_file = file_with("Value: %d", simple_entry_without_source());
    let fallback = validator::validate_translations_detailed(
        &fallback_file,
        &[translation("Value: %d", "Wert: %d")],
    );
    assert!(fallback.rejected.is_empty());

    let plural_file = file_with("items", plural_entry());
    let mut forms = BTreeMap::new();
    forms.insert("one".to_string(), "%d Element".to_string());
    forms.insert("other".to_string(), "%lld Elemente".to_string());
    let plural = CompletedTranslation {
        key: "items".to_string(),
        locale: "de".to_string(),
        value: String::new(),
        plural_forms: Some(forms),
        substitution_name: None,
    };
    let detailed = validator::validate_translations_detailed(&plural_file, &[plural]);
    assert!(detailed.rejected.is_empty(), "{:?}", detailed.rejected);
}

#[test]
fn submit_and_file_validation_have_plural_classification_parity() {
    let mut entry = plural_entry();
    let target_plural = BTreeMap::from([
        (
            "one".to_string(),
            PluralVariation {
                string_unit: StringUnit {
                    state: TranslationState::Translated,
                    value: "%d Element".to_string(),
                },
            },
        ),
        (
            "other".to_string(),
            PluralVariation {
                string_unit: StringUnit {
                    state: TranslationState::Translated,
                    value: "%lld Elemente".to_string(),
                },
            },
        ),
    ]);
    entry.localizations.as_mut().unwrap().insert(
        "de".to_string(),
        Localization {
            string_unit: None,
            variations: Some(Variations {
                plural: Some(target_plural),
                device: None,
            }),
            substitutions: None,
        },
    );
    let file = file_with("items", entry);
    let mut forms = BTreeMap::new();
    forms.insert("one".to_string(), "%d Element".to_string());
    forms.insert("other".to_string(), "%lld Elemente".to_string());
    let submitted = CompletedTranslation {
        key: "items".to_string(),
        locale: "de".to_string(),
        value: String::new(),
        plural_forms: Some(forms),
        substitution_name: None,
    };

    let submit = validator::validate_translations_detailed(&file, &[submitted]);
    let file_report = file_validator::validate_file(&file, Some("de"));
    assert!(submit.rejected.is_empty(), "{:?}", submit.rejected);
    assert!(
        file_report[0].errors.is_empty(),
        "{:?}",
        file_report[0].errors
    );

    let mut broken = file.clone();
    broken.strings["items"].localizations.as_mut().unwrap()["de"]
        .variations
        .as_mut()
        .unwrap()
        .plural
        .as_mut()
        .unwrap()
        .get_mut("other")
        .unwrap()
        .string_unit
        .value = "Elemente".to_string();
    let report = file_validator::validate_file(&broken, Some("de"));
    assert!(
        report[0]
            .errors
            .iter()
            .any(|issue| issue.issue_type == "format_specifier_count_mismatch")
    );
}

#[test]
fn shared_source_resolver_validates_substitution_placeholders() {
    let file = file_with("birds", substitution_entry());
    let mut forms = BTreeMap::new();
    forms.insert("one".to_string(), "Vogel".to_string());
    forms.insert("other".to_string(), "%arg Vögel".to_string());
    let submitted = CompletedTranslation {
        key: "birds".to_string(),
        locale: "de".to_string(),
        value: String::new(),
        plural_forms: Some(forms),
        substitution_name: Some("BIRDS".to_string()),
    };
    let detailed = validator::validate_translations_detailed(&file, &[submitted]);
    assert!(
        detailed
            .rejected
            .iter()
            .any(|issue| issue.reason.contains("substitution placeholder")),
        "{:?}",
        detailed.rejected
    );
}

#[test]
fn substitution_validation_requires_exact_percent_arg_tokens() {
    let valid = substitution_translation("%argꥠ", "%arg𰀀");
    let valid_file = file_with("birds", substitution_entry_with_target("%argꥠ", "%arg𰀀"));
    let valid_submit = validator::validate_translations_detailed(
        &file_with("birds", substitution_entry()),
        &[valid],
    );
    assert!(valid_submit.rejected.is_empty());
    assert!(
        file_validator::validate_file(&valid_file, Some("de"))[0]
            .errors
            .is_empty()
    );

    let invalid_targets = ["%argument", "%arg_suffix", "%%arg", "%argéclair", "%argβ"];
    for target in invalid_targets {
        let submitted = substitution_translation(target, target);
        let submit_report = validator::validate_translations_detailed(
            &file_with("birds", substitution_entry()),
            &[submitted],
        );
        assert_eq!(submit_report.rejected.len(), 2, "submit target {target:?}");
        assert!(
            submit_report
                .rejected
                .iter()
                .all(|issue| { issue.reason.contains("substitution placeholder mismatch") })
        );

        let file = file_with("birds", substitution_entry_with_target(target, target));
        let file_report = file_validator::validate_file(&file, Some("de"));
        assert_eq!(
            format_issue_codes(&file_report[0].errors),
            [
                "substitution_placeholder_mismatch",
                "substitution_placeholder_mismatch"
            ],
            "file target {target:?}"
        );
    }
}

#[test]
fn xliff_decodes_and_normalizes_attribute_values() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff version="1.2"><file target-language="d&#x65;"><body>
<trans-unit id="a&amp;b&lt;c&gt;d&apos;e&quot;f&#x21;&#33;">
<source>Source</source><target>Target</target>
</trans-unit></body></file></xliff>"#;
    let (locale, translations) = xliff::import_xliff(xml).unwrap();
    assert_eq!(locale, "de");
    assert_eq!(translations.len(), 1);
    assert_eq!(translations[0].key, "a&b<c>d'e\"f!!");
}

#[test]
fn xliff_attribute_export_import_roundtrip_preserves_special_keys() {
    let key = "a&b<c>d'e\"f&#33;";
    let file = file_with(key, simple_entry("Source", Some(("de", "Ziel"))));
    let (xml, count) = xliff::export_xliff(&file, "de", "test.xcstrings", false).unwrap();
    assert_eq!(count, 1);
    let (locale, translations) = xliff::import_xliff(&xml).unwrap();
    assert_eq!(locale, "de");
    assert_eq!(translations[0].key, key);
}

proptest! {
    #[test]
    fn definite_arguments_survive_generated_unicode_context(
        prefix in "[a-zé日 ]{0,32}",
        token in prop::sample::select(vec!["%d", "%lld", "%2$lld"]),
        suffix in prop::sample::select(vec!["日", "分", "秒", "개"]),
    ) {
        let text = format!("{prefix}{token}{suffix}");
        let analysis = analyze_format(&text);
        prop_assert_eq!(
            analysis.arguments.iter().map(|argument| argument.raw.as_str()).collect::<Vec<_>>(),
            vec![token]
        );
        prop_assert!(analysis.ambiguous.is_empty());
        prop_assert!(analysis.problems.is_empty());
    }

    #[test]
    fn all_reported_spans_are_utf8_boundaries(text in ".{0,128}") {
        let analysis = analyze_format(&text);
        for span in analysis.spans() {
            prop_assert!(text.is_char_boundary(span.start));
            prop_assert!(text.is_char_boundary(span.end));
            prop_assert_eq!(&text[span.start..span.end], span.raw.as_str());
        }
    }

    #[test]
    fn valid_positional_reorder_is_invariant(
        left in prop::sample::select(vec!["@", "d", "lld", ".2f"]),
        right in prop::sample::select(vec!["@", "d", "lld", ".2f"]),
    ) {
        let source = format!("%{left} then %{right}");
        let target = format!("%2${right} then %1${left}");
        let comparison = compare_formats(&source, &target);
        prop_assert!(comparison.errors.is_empty(), "{:?}", comparison.errors);
    }
}

fn format_issue_codes(issues: &[xcstrings_mcp::model::translation::ValidationIssue]) -> Vec<&str> {
    issues
        .iter()
        .filter(|issue| {
            issue.issue_type.starts_with("format_")
                || issue.issue_type == "invalid_format_specifier"
                || issue.issue_type == "invalid_positional_argument"
                || issue.issue_type == "substitution_placeholder_mismatch"
        })
        .map(|issue| issue.issue_type.as_str())
        .collect()
}

fn simple_entry_without_source() -> StringEntry {
    StringEntry {
        extraction_state: None,
        should_translate: true,
        comment: None,
        localizations: None,
    }
}

fn plural_entry() -> StringEntry {
    let plural = BTreeMap::from([
        (
            "one".to_string(),
            PluralVariation {
                string_unit: StringUnit {
                    state: TranslationState::Translated,
                    value: "%d item".to_string(),
                },
            },
        ),
        (
            "other".to_string(),
            PluralVariation {
                string_unit: StringUnit {
                    state: TranslationState::Translated,
                    value: "%lld items".to_string(),
                },
            },
        ),
    ]);
    let localization = Localization {
        string_unit: None,
        variations: Some(Variations {
            plural: Some(plural),
            device: None,
        }),
        substitutions: None,
    };
    StringEntry {
        extraction_state: None,
        should_translate: true,
        comment: None,
        localizations: Some(IndexMap::from([("en".to_string(), localization)])),
    }
}

fn substitution_entry() -> StringEntry {
    let substitution = substitution_value("%arg bird", "%arg birds");
    let localization = Localization {
        string_unit: Some(StringUnit {
            state: TranslationState::Translated,
            value: "I saw %#@BIRDS@".to_string(),
        }),
        variations: None,
        substitutions: Some(BTreeMap::from([("BIRDS".to_string(), substitution)])),
    };
    StringEntry {
        extraction_state: None,
        should_translate: true,
        comment: None,
        localizations: Some(IndexMap::from([("en".to_string(), localization)])),
    }
}

fn substitution_entry_with_target(one: &str, other: &str) -> StringEntry {
    let mut entry = substitution_entry();
    entry.localizations.as_mut().unwrap().insert(
        "de".to_string(),
        Localization {
            string_unit: None,
            variations: None,
            substitutions: Some(BTreeMap::from([(
                "BIRDS".to_string(),
                substitution_value(one, other),
            )])),
        },
    );
    entry
}

fn substitution_value(one: &str, other: &str) -> serde_json::Value {
    serde_json::json!({
        "argNum": 1,
        "formatSpecifier": "lld",
        "variations": {
            "plural": {
                "one": { "stringUnit": { "state": "translated", "value": one } },
                "other": { "stringUnit": { "state": "translated", "value": other } }
            }
        }
    })
}

fn substitution_translation(one: &str, other: &str) -> CompletedTranslation {
    CompletedTranslation {
        key: "birds".to_string(),
        locale: "de".to_string(),
        value: String::new(),
        plural_forms: Some(BTreeMap::from([
            ("one".to_string(), one.to_string()),
            ("other".to_string(), other.to_string()),
        ])),
        substitution_name: Some("BIRDS".to_string()),
    }
}
