use crate::model::glossary::*;
use std::collections::BTreeSet;
use unicode_normalization::UnicodeNormalization;

pub(super) fn diagnostic(
    code: &str,
    id: Option<&str>,
    detail: impl Into<String>,
) -> GlossaryDiagnostic {
    GlossaryDiagnostic {
        code: code.into(),
        term_id: id.map(str::to_owned),
        detail: detail.into(),
    }
}
pub(super) fn bounded(scope: &TermScope) -> bool {
    !scope.keys.is_empty()
        || !scope.paths.is_empty()
        || !scope.screens.is_empty()
        || !scope.roles.is_empty()
}
pub fn validate_glossary(doc: &GlossaryDocument) -> Vec<GlossaryDiagnostic> {
    let mut out = Vec::new();
    if doc.schema_version != 2 {
        out.push(diagnostic(
            "unsupported_schema_version",
            None,
            "schema_version must be 2",
        ));
    }
    let mut ids = BTreeSet::new();
    for term in &doc.terms {
        if !ids.insert(&term.id) {
            out.push(diagnostic(
                "duplicate_term_id",
                Some(&term.id),
                "Term IDs must be unique",
            ));
        }
        validate_term(term, &mut out);
    }
    out
}
fn validate_term(term: &GlossaryTerm, out: &mut Vec<GlossaryDiagnostic>) {
    let id = Some(term.id.as_str());
    for (field, value) in [("id", &term.id), ("source", &term.source)] {
        if value.trim().is_empty() {
            out.push(diagnostic(
                &format!("empty_{field}"),
                id,
                format!("{field} must not be blank"),
            ));
        }
    }
    for (field, locale) in [
        ("source_locale", &term.source_locale),
        ("target_locale", &term.target_locale),
    ] {
        if language_tags::LanguageTag::parse(locale).is_err() {
            out.push(diagnostic(
                "invalid_locale",
                id,
                format!("{field} must be a valid language tag"),
            ));
        }
    }
    if term.preferred.is_empty() && term.forbidden.is_empty() && !term.do_not_translate {
        out.push(diagnostic(
            "empty_rule",
            id,
            "Specify preferred, forbidden, or do_not_translate",
        ));
    }
    for forms in [
        &term.source_variants,
        &term.preferred,
        &term.accepted_variants,
        &term.forbidden,
    ] {
        validate_forms(forms, id, out);
    }
    let accepted: BTreeSet<String> = term
        .preferred
        .iter()
        .chain(&term.accepted_variants)
        .map(|s| s.nfc().collect())
        .collect();
    if term
        .forbidden
        .iter()
        .any(|s| accepted.contains(&s.nfc().collect::<String>()))
    {
        out.push(diagnostic(
            "contradictory_term",
            id,
            "A form cannot be both accepted and forbidden",
        ));
    }
    if term.do_not_translate
        && term.preferred.iter().any(|s| {
            s.nfc().collect::<String>() != term.source.nfc().collect::<String>()
                && !term.source_variants.contains(s)
        })
    {
        out.push(diagnostic(
            "contradictory_term",
            id,
            "Do-not-translate preferred forms must be source spellings",
        ));
    }
    validate_scope(&term.scope, id, out);
    for exception in &term.exceptions {
        if !bounded(&exception.scope) {
            out.push(diagnostic(
                "unbounded_exception",
                id,
                "Exceptions require an explicit key, path, screen, or role selector",
            ));
        }
        if exception.reason.trim().is_empty() {
            out.push(diagnostic(
                "missing_exception_reason",
                id,
                "Exception reason must not be blank",
            ));
        }
        if exception.accepted_variants.is_empty() && exception.ignore_checks.is_empty() {
            out.push(diagnostic(
                "empty_exception",
                id,
                "Exception must declare a variant or waived check",
            ));
        }
        validate_forms(&exception.accepted_variants, id, out);
        validate_scope(&exception.scope, id, out);
    }
}
fn validate_forms(forms: &[String], id: Option<&str>, out: &mut Vec<GlossaryDiagnostic>) {
    let mut seen = BTreeSet::new();
    for form in forms {
        if form.trim().is_empty() {
            out.push(diagnostic(
                "empty_term_form",
                id,
                "Term forms must not be blank",
            ));
        }
        if !seen.insert(form.nfc().collect::<String>()) {
            out.push(diagnostic(
                "duplicate_term_form",
                id,
                "Canonically equivalent forms are duplicates",
            ));
        }
    }
}
fn validate_scope(scope: &TermScope, id: Option<&str>, out: &mut Vec<GlossaryDiagnostic>) {
    // Catalog keys and authored selectors use exact identity, not term normalization.
    let mut keys = BTreeSet::new();
    for key in &scope.keys {
        if !keys.insert(key) {
            out.push(diagnostic(
                "duplicate_scope_key",
                id,
                "Scope keys must be unique",
            ));
        }
    }
    for selectors in [&scope.screens, &scope.roles] {
        let mut seen = BTreeSet::new();
        for selector in selectors {
            if selector.trim().is_empty() {
                out.push(diagnostic(
                    "empty_term_form",
                    id,
                    "Context selectors must not be blank",
                ));
            }
            if !seen.insert(selector) {
                out.push(diagnostic(
                    "duplicate_term_form",
                    id,
                    "Context selectors must be unique",
                ));
            }
        }
    }
    let mut paths = std::collections::HashSet::new();
    for path in &scope.paths {
        if !paths.insert(path) {
            out.push(diagnostic(
                "duplicate_scope_path",
                id,
                "Scope paths must be unique",
            ));
        }
        if let Err(reason) = crate::model::xcstrings::paths::validate_supported_path(path) {
            out.push(diagnostic("invalid_scope_path", id, reason));
        }
    }
}
