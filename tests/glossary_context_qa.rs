use serde_json::json;
use xcstrings_mcp::model::{context::ResolvedContext, glossary::*};
use xcstrings_mcp::service::glossary::{TerminologyInput, check_terminology, relevant_terms};

fn term(id: &str, source: &str, preferred: &str) -> GlossaryTerm {
    GlossaryTerm {
        id: id.into(),
        source_locale: "en".into(),
        target_locale: "fr".into(),
        source: source.into(),
        preferred: vec![preferred.into()],
        ..Default::default()
    }
}
fn report(terms: Vec<GlossaryTerm>, source: &str, target: &str) -> TerminologyReport {
    check_terminology(
        &GlossaryDocument {
            terms,
            ..Default::default()
        },
        &TerminologyInput {
            key: "profile.action",
            source_locale: "en",
            target_locale: "fr",
            path: &[],
            source_text: source,
            target_text: Some(target),
            context: &ResolvedContext::default(),
        },
    )
}
#[test]
fn preferred_missing_reports_exact_address_and_expected_forms() {
    let actual = report(
        vec![term("account", "Account", "Compte")],
        "Account settings",
        "Réglages du profil",
    );
    assert_eq!(actual.issues.len(), 1);
    assert_eq!(actual.issues[0].code, TerminologyCode::PreferredMissing);
    assert_eq!(actual.issues[0].term_id, "account");
    assert_eq!(actual.issues[0].key, "profile.action");
    assert_eq!(actual.issues[0].locale, "fr");
    assert_eq!(actual.issues[0].path, vec![]);
    assert_eq!(actual.issues[0].expected, vec!["Compte"]);
    assert_eq!(actual.evaluated_term_ids, vec!["account"]);
}
#[test]
fn forbidden_is_reported_even_when_preferred_also_present() {
    let mut rule = term("account", "Account", "Compte");
    rule.forbidden = vec!["Profil".into()];
    let actual = report(vec![rule], "Account", "Compte et Profil");
    assert_eq!(actual.issues.len(), 1);
    assert_eq!(actual.issues[0].code, TerminologyCode::ForbiddenUsed);
    assert_eq!(actual.issues[0].observed, vec!["Profil"]);
}
#[test]
fn unicode_words_do_not_match_substrings_or_implicit_inflections() {
    assert_eq!(
        report(
            vec![term("account", "account", "compte")],
            "accounting accounts",
            "profil"
        )
        .evaluated_term_ids,
        Vec::<String>::new()
    );
}
#[test]
fn explicit_source_and_target_inflections_are_accepted() {
    let mut rule = term("account", "account", "compte");
    rule.source_variants = vec!["accounts".into()];
    rule.accepted_variants = vec!["comptes".into()];
    let actual = report(vec![rule], "accounts", "comptes");
    assert_eq!(actual.issues, vec![]);
    assert_eq!(actual.evaluated_term_ids, vec!["account"]);
}
#[test]
fn canonically_equivalent_unicode_matches_without_changing_input() {
    let source = "Cafe\u{301}";
    let target = "Cafe\u{301}";
    let actual = report(vec![term("cafe", "Café", "Café")], source, target);
    assert_eq!(actual.issues, vec![]);
    assert_eq!(actual.evaluated_term_ids, vec!["cafe"]);
    assert_eq!(source, "Cafe\u{301}");
}
#[test]
fn casing_is_explicit_and_not_inferred_from_locale() {
    assert_eq!(
        report(
            vec![term("account", "Account", "Compte")],
            "account",
            "profil"
        )
        .evaluated_term_ids,
        Vec::<String>::new()
    );
}
#[test]
fn format_and_substitution_identifiers_are_not_terms() {
    assert_eq!(
        report(
            vec![
                term("ld", "lld", "entier"),
                term("account", "Account", "Compte")
            ],
            "%lld %#@Account@",
            "%lld %#@Account@"
        )
        .evaluated_term_ids,
        Vec::<String>::new()
    );
}
#[test]
fn longest_contained_source_phrase_suppresses_shorter_term() {
    let actual = report(
        vec![
            term("account", "account", "compte"),
            term("bank", "bank account", "compte bancaire"),
        ],
        "bank account",
        "compte bancaire",
    );
    assert_eq!(actual.evaluated_term_ids, vec!["bank"]);
    assert_eq!(actual.issues, vec![]);
}
#[test]
fn shorter_term_still_applies_to_separate_noncontained_occurrence() {
    let actual = report(
        vec![
            term("account", "account", "compte"),
            term("bank", "bank account", "compte bancaire"),
        ],
        "bank account and account",
        "compte bancaire",
    );
    assert_eq!(actual.evaluated_term_ids, vec!["account", "bank"]);
}
#[test]
fn equal_applicable_senses_report_conflict_without_arbitrary_choice() {
    let actual = report(
        vec![
            term("financial", "bank", "banque"),
            term("river", "bank", "rive"),
        ],
        "bank",
        "banque",
    );
    assert_eq!(actual.issues[0].code, TerminologyCode::RuleConflict);
    assert_eq!(actual.evaluated_term_ids, Vec::<String>::new());
    assert_eq!(actual.unevaluated_term_ids, vec!["financial", "river"]);
}
#[test]
fn context_missing_is_unevaluated_and_never_inferred_from_key() {
    let mut rule = term("account", "Account", "Compte");
    rule.scope.screens = vec!["Profile".into()];
    let actual = report(vec![rule], "Account", "Profil");
    assert_eq!(actual.issues[0].code, TerminologyCode::Unevaluated);
    assert_eq!(actual.evaluated_term_ids, Vec::<String>::new());
}
#[test]
fn exact_context_exception_accepts_declared_variant_only() {
    let mut rule = term("account", "Account", "Compte");
    rule.exceptions = vec![TermException {
        scope: TermScope {
            keys: vec!["profile.action".into()],
            ..Default::default()
        },
        reason: "Button noun inflection".into(),
        accepted_variants: vec!["Comptes".into()],
        ..Default::default()
    }];
    assert_eq!(report(vec![rule], "Account", "Comptes").issues, vec![]);
}
#[test]
fn do_not_translate_preserves_matched_source_spelling() {
    let mut rule = term("brand", "Acme", "Acme");
    rule.do_not_translate = true;
    let actual = report(vec![rule], "Open Acme", "Ouvrir Acmé");
    assert_eq!(
        actual.issues[0].code,
        TerminologyCode::UntranslatableChanged
    );
}
#[test]
fn dictionary_script_requires_explicit_literal_policy() {
    let rule = term("settings", "设置", "Réglages");
    assert_eq!(
        report(vec![rule.clone()], "设置", "Réglages").issues[0].code,
        TerminologyCode::Unevaluated
    );
    assert_eq!(
        report(
            vec![GlossaryTerm {
                match_mode: TermMatchMode::Literal,
                ..rule
            }],
            "设置",
            "Réglages"
        )
        .issues,
        vec![]
    );
}
#[test]
fn future_scope_semantics_are_not_silently_unscoped() {
    let mut rule = term("account", "Account", "Compte");
    rule.scope
        .extra
        .insert("future".into(), json!({"condition":true}));
    assert_eq!(
        report(vec![rule], "Account", "Compte").issues[0].code,
        TerminologyCode::Unevaluated
    );
}
#[test]
fn relevant_terms_use_same_explicit_context_scope_as_qa() {
    let mut rule = term("account", "Account", "Compte");
    rule.scope.roles = vec!["title".into()];
    let mut context = ResolvedContext::default();
    context.fields.role = Some("button".into());
    let selected = relevant_terms(
        &GlossaryDocument {
            terms: vec![rule],
            ..Default::default()
        },
        &TerminologyInput {
            key: "profile.action",
            source_locale: "en",
            target_locale: "fr",
            path: &[],
            source_text: "Account",
            target_text: None,
            context: &context,
        },
    );
    assert_eq!(selected.terms, vec![]);
}

#[test]
fn all_three_conflicting_senses_are_unevaluated() {
    let actual = report(
        vec![
            term("a", "bank", "banque"),
            term("b", "bank", "rive"),
            term("c", "bank", "banc"),
        ],
        "bank",
        "banque",
    );
    assert_eq!(actual.evaluated_term_ids, Vec::<String>::new());
    assert_eq!(actual.unevaluated_term_ids, vec!["a", "b", "c"]);
}

#[test]
fn escaped_argument_literal_is_prose_but_real_substitution_token_is_not() {
    let actual = report(
        vec![term("literal", "arg", "argument")],
        "%%arg and %arg",
        "%%arg et %arg",
    );
    assert_eq!(actual.evaluated_term_ids, vec!["literal"]);
    assert_eq!(actual.issues[0].code, TerminologyCode::PreferredMissing);
}

#[test]
fn empty_translation_has_advisory_missing_term_but_absent_target_has_no_check() {
    let rule = term("account", "Account", "Compte");
    assert_eq!(
        report(vec![rule.clone()], "Account", "").issues[0].code,
        TerminologyCode::PreferredMissing
    );
    let actual = check_terminology(
        &GlossaryDocument {
            terms: vec![rule],
            ..Default::default()
        },
        &TerminologyInput {
            key: "k",
            source_locale: "en",
            target_locale: "fr",
            path: &[],
            source_text: "Account",
            target_text: None,
            context: &ResolvedContext::default(),
        },
    );
    assert_eq!(actual, TerminologyReport::default());
}

#[test]
fn locale_pair_has_no_implicit_region_fallback_or_reverse_lookup() {
    let doc = GlossaryDocument {
        terms: vec![term("account", "Account", "Compte")],
        ..Default::default()
    };
    let actual = check_terminology(
        &doc,
        &TerminologyInput {
            key: "k",
            source_locale: "en",
            target_locale: "fr-CA",
            path: &[],
            source_text: "Account",
            target_text: Some("Profil"),
            context: &ResolvedContext::default(),
        },
    );
    assert_eq!(actual, TerminologyReport::default());
}

#[test]
fn explicit_exception_waives_only_named_check() {
    let mut rule = term("account", "Account", "Compte");
    rule.forbidden = vec!["Profil".into()];
    rule.exceptions = vec![TermException {
        scope: TermScope {
            keys: vec!["profile.action".into()],
            ..Default::default()
        },
        reason: "No preferred noun in this abbreviated button".into(),
        ignore_checks: vec![TermCheck::Preferred],
        ..Default::default()
    }];
    let actual = report(vec![rule], "Account", "Profil");
    assert_eq!(actual.issues.len(), 1);
    assert_eq!(actual.issues[0].code, TerminologyCode::ForbiddenUsed);
}

#[test]
fn overlapping_literal_occurrence_not_contained_in_longer_match_remains_relevant() {
    let short = GlossaryTerm {
        match_mode: TermMatchMode::Literal,
        ..term("short", "aba", "court")
    };
    let long = GlossaryTerm {
        match_mode: TermMatchMode::Literal,
        ..term("long", "abab", "long")
    };
    let actual = report(vec![short, long], "ababa", "long et court");
    assert_eq!(actual.evaluated_term_ids, vec!["long", "short"]);
    assert_eq!(actual.issues, vec![]);
}

#[test]
fn preferred_and_forbidden_rules_for_same_form_are_conflicting() {
    let forbidden = GlossaryTerm {
        preferred: vec![],
        forbidden: vec!["Compte".into()],
        ..term("b", "Account", "unused")
    };
    let actual = report(
        vec![term("a", "Account", "Compte"), forbidden],
        "Account",
        "Compte",
    );
    assert_eq!(actual.evaluated_term_ids, Vec::<String>::new());
    assert_eq!(actual.unevaluated_term_ids, vec!["a", "b"]);
    assert_eq!(
        actual.issues.iter().map(|i| i.code).collect::<Vec<_>>(),
        vec![TerminologyCode::RuleConflict, TerminologyCode::RuleConflict]
    );
}

#[test]
fn compatible_do_not_translate_and_preferred_rules_do_not_conflict() {
    let brand = GlossaryTerm {
        do_not_translate: true,
        ..term("brand", "Acme", "Acme")
    };
    let actual = report(
        vec![brand, term("preferred", "Acme", "Acme")],
        "Acme",
        "Acme",
    );
    assert_eq!(actual.evaluated_term_ids, vec!["brand", "preferred"]);
    assert_eq!(actual.issues, vec![]);
}

#[test]
fn waived_preferred_check_does_not_create_a_false_rule_conflict() {
    let waived = GlossaryTerm {
        exceptions: vec![TermException {
            scope: TermScope {
                keys: vec!["profile.action".into()],
                ..Default::default()
            },
            reason: "Context uses another sense".into(),
            ignore_checks: vec![TermCheck::Preferred],
            ..Default::default()
        }],
        ..term("a", "bank", "banque")
    };
    let actual = report(vec![waived, term("b", "bank", "rive")], "bank", "rive");
    assert_eq!(actual.evaluated_term_ids, vec!["a", "b"]);
    assert_eq!(actual.issues, vec![]);
}

#[test]
fn do_not_translate_variant_conflict_uses_matched_spelling_not_base_preferred() {
    let brand = GlossaryTerm {
        source_variants: vec!["ACME".into()],
        do_not_translate: true,
        ..term("brand", "Acme", "Acme")
    };
    let actual = report(
        vec![brand, term("preferred", "ACME", "Acme")],
        "ACME",
        "Acme",
    );
    assert_eq!(actual.evaluated_term_ids, Vec::<String>::new());
    assert_eq!(actual.unevaluated_term_ids, vec!["brand", "preferred"]);
    assert_eq!(
        actual.issues.iter().map(|i| i.code).collect::<Vec<_>>(),
        vec![TerminologyCode::RuleConflict, TerminologyCode::RuleConflict]
    );
}
