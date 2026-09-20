use super::matching::{TerminologyInput, needs_tailoring, occurrences, prose};
use crate::model::glossary::*;

pub fn check_terminology(
    doc: &GlossaryDocument,
    input: &TerminologyInput<'_>,
) -> TerminologyReport {
    let selection = super::relevant_terms(doc, input);
    let mut result = TerminologyReport::default();
    let Some(target) = input.target_text else {
        return result;
    };
    let target = prose(target);
    for relevant in selection.terms {
        let term = &relevant.term;
        let conflict = relevant.explanation.as_deref()
            == Some("Conflicting equally specific terminology rules");
        if relevant.applicability == TermApplicability::Unevaluated {
            result.unevaluated_term_ids.push(term.id.clone());
            result.issues.push(issue(
                input,
                term,
                if conflict {
                    TerminologyCode::RuleConflict
                } else {
                    TerminologyCode::Unevaluated
                },
                Vec::new(),
                Vec::new(),
                relevant
                    .explanation
                    .as_deref()
                    .unwrap_or("Rule cannot be evaluated"),
            ));
            continue;
        }
        let unknown_target = term
            .forbidden
            .iter()
            .chain(&relevant.accepted_variants)
            .any(|s| needs_tailoring(s, term.match_mode));
        if unknown_target {
            result.unevaluated_term_ids.push(term.id.clone());
            result.issues.push(issue(
                input,
                term,
                TerminologyCode::Unevaluated,
                Vec::new(),
                Vec::new(),
                "Target forms require an explicit literal matching policy",
            ));
            continue;
        }
        result.evaluated_term_ids.push(term.id.clone());
        let contains = |form: &str| !occurrences(&target, form, term.match_mode).is_empty();
        if term.do_not_translate && !relevant.ignored_checks.contains(&TermCheck::DoNotTranslate) {
            let missing: Vec<_> = relevant
                .matched_source
                .iter()
                .filter(|form| !contains(form))
                .cloned()
                .collect();
            let accepted_exception = relevant.accepted_variants.iter().any(|form| contains(form));
            if !missing.is_empty() && !accepted_exception {
                result.issues.push(issue(
                    input,
                    term,
                    TerminologyCode::UntranslatableChanged,
                    missing,
                    Vec::new(),
                    "Keep the matched source spelling or an explicitly accepted variant",
                ));
            }
        } else if !term.do_not_translate
            && !term.preferred.is_empty()
            && !relevant.ignored_checks.contains(&TermCheck::Preferred)
            && !relevant.accepted_variants.iter().any(|form| contains(form))
        {
            result.issues.push(issue(
                input,
                term,
                TerminologyCode::PreferredMissing,
                relevant.accepted_variants.clone(),
                Vec::new(),
                "No preferred or explicitly accepted form is present",
            ));
        }
        if !relevant.ignored_checks.contains(&TermCheck::Forbidden) {
            let forbidden: Vec<_> = term
                .forbidden
                .iter()
                .filter(|form| contains(form))
                .cloned()
                .collect();
            if !forbidden.is_empty() {
                result.issues.push(issue(
                    input,
                    term,
                    TerminologyCode::ForbiddenUsed,
                    term.preferred.clone(),
                    forbidden,
                    "A forbidden term occurs in the translation",
                ));
            }
        }
    }
    result
}
fn issue(
    input: &TerminologyInput<'_>,
    term: &GlossaryTerm,
    code: TerminologyCode,
    expected: Vec<String>,
    observed: Vec<String>,
    detail: &str,
) -> TerminologyIssue {
    TerminologyIssue {
        code,
        term_id: term.id.clone(),
        key: input.key.into(),
        locale: input.target_locale.into(),
        path: input.path.to_vec(),
        expected,
        observed,
        detail: detail.into(),
    }
}
