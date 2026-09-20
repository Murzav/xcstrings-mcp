use crate::model::{context::ResolvedContext, glossary::*, xcstrings::paths::LeafStep};
use std::ops::Range;
use unicode_normalization::UnicodeNormalization;
use unicode_script::{Script, UnicodeScript};
use unicode_segmentation::UnicodeSegmentation;

pub struct TerminologyInput<'a> {
    pub key: &'a str,
    pub source_locale: &'a str,
    pub target_locale: &'a str,
    pub path: &'a [LeafStep],
    pub source_text: &'a str,
    pub target_text: Option<&'a str>,
    pub context: &'a ResolvedContext,
}

#[derive(PartialEq, Eq)]
enum ScopeMatch {
    Yes,
    No,
    Unknown,
}
fn scope_match(scope: &TermScope, input: &TerminologyInput<'_>) -> ScopeMatch {
    if (!scope.keys.is_empty() && !scope.keys.iter().any(|s| s == input.key))
        || (!scope.paths.is_empty() && !scope.paths.iter().any(|p| p == input.path))
    {
        return ScopeMatch::No;
    }
    let mut unknown = !scope.extra.is_empty();
    for (allowed, value) in [
        (&scope.screens, &input.context.fields.screen),
        (&scope.roles, &input.context.fields.role),
    ] {
        if allowed.is_empty() {
            continue;
        }
        match value {
            Some(value) if !allowed.contains(value) => return ScopeMatch::No,
            None => unknown = true,
            _ => {}
        }
    }
    if unknown {
        ScopeMatch::Unknown
    } else {
        ScopeMatch::Yes
    }
}

/// Mask parser-recognized placeholders before NFC comparison. Prose is never edited.
pub(super) fn prose(text: &str) -> String {
    let analysis = crate::model::specifier::analyze_format(text);
    let mut ranges: Vec<_> = analysis.arguments.iter().map(|a| a.start..a.end).collect();
    if let Ok(references) = crate::model::xcstrings::references::substitution_references(text) {
        ranges.extend(references.into_iter().map(|r| r.start..r.end));
    }
    ranges.extend(crate::model::specifier::substitution_placeholder_ranges(
        text,
    ));
    ranges.sort_by_key(|r| (r.start, std::cmp::Reverse(r.end)));
    let mut result = String::with_capacity(text.len());
    let mut cursor = 0;
    for range in ranges {
        if range.start < cursor {
            continue;
        }
        result.push_str(&text[cursor..range.start]);
        result.push('\0');
        cursor = range.end;
    }
    result.push_str(&text[cursor..]);
    result.nfc().collect()
}
fn dictionary_script(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(
            c.script(),
            Script::Han
                | Script::Hiragana
                | Script::Katakana
                | Script::Thai
                | Script::Lao
                | Script::Khmer
                | Script::Myanmar
        )
    })
}
pub(super) fn needs_tailoring(text: &str, mode: TermMatchMode) -> bool {
    mode == TermMatchMode::Word && dictionary_script(text)
}
pub(super) fn occurrences(text: &str, form: &str, mode: TermMatchMode) -> Vec<Range<usize>> {
    let form: String = form.nfc().collect();
    if form.is_empty() {
        return Vec::new();
    }
    let boundaries: std::collections::BTreeSet<_> = text
        .split_word_bound_indices()
        .flat_map(|(i, s)| [i, i + s.len()])
        .collect();
    text.char_indices()
        .filter_map(|(start, _)| {
            if !text[start..].starts_with(&form) {
                return None;
            }
            let end = start + form.len();
            (mode == TermMatchMode::Literal
                || (boundaries.contains(&start) && boundaries.contains(&end)))
            .then_some(start..end)
        })
        .collect()
}
struct Candidate {
    term: RelevantTerm,
    spans: Vec<Range<usize>>,
}

pub fn relevant_terms(doc: &GlossaryDocument, input: &TerminologyInput<'_>) -> TermSelection {
    let source = prose(input.source_text);
    let mut candidates = Vec::new();
    for term in &doc.terms {
        if term.source_locale != input.source_locale || term.target_locale != input.target_locale {
            continue;
        }
        let scope = scope_match(&term.scope, input);
        if scope == ScopeMatch::No {
            continue;
        }
        let mut matched_source = Vec::new();
        let mut spans = Vec::new();
        let mut tailored = false;
        for form in std::iter::once(&term.source).chain(&term.source_variants) {
            let needs = needs_tailoring(form, term.match_mode);
            let found = occurrences(
                &source,
                form,
                if needs {
                    TermMatchMode::Literal
                } else {
                    term.match_mode
                },
            );
            if !found.is_empty() {
                matched_source.push(form.clone());
                spans.extend(found);
                tailored |= needs;
            }
        }
        if spans.is_empty() {
            continue;
        }
        let mut relevant = RelevantTerm {
            term: term.clone(),
            applicability: TermApplicability::Applicable,
            accepted_variants: term
                .preferred
                .iter()
                .filter(|_| !term.do_not_translate)
                .chain(&term.accepted_variants)
                .cloned()
                .collect(),
            ignored_checks: Vec::new(),
            matched_source,
            explanation: None,
        };
        if scope == ScopeMatch::Unknown || tailored {
            make_unknown(
                &mut relevant,
                "Context selector or word segmentation cannot be evaluated deterministically",
            );
        }
        for exception in &term.exceptions {
            match scope_match(&exception.scope, input) {
                ScopeMatch::No => {}
                ScopeMatch::Unknown => make_unknown(
                    &mut relevant,
                    "Exception context is unavailable or unsupported",
                ),
                ScopeMatch::Yes if !exception.extra.is_empty() => {
                    make_unknown(&mut relevant, "Unknown exception semantics")
                }
                ScopeMatch::Yes => {
                    relevant
                        .accepted_variants
                        .extend(exception.accepted_variants.iter().cloned());
                    relevant.ignored_checks.extend(&exception.ignore_checks);
                }
            }
        }
        relevant.accepted_variants.sort();
        relevant.accepted_variants.dedup();
        candidates.push(Candidate {
            term: relevant,
            spans,
        });
    }
    // A shorter rule remains relevant when it also has an independent occurrence.
    let surviving: Vec<_> = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            let visible = candidate.spans.iter().any(|span| {
                !candidates.iter().enumerate().any(|(other_index, other)| {
                    index != other_index
                        && other.term.applicability == TermApplicability::Applicable
                        && other.spans.iter().any(|long| {
                            long.start <= span.start
                                && long.end >= span.end
                                && long.len() > span.len()
                        })
                })
            });
            visible.then_some(index)
        })
        .collect();
    let mut terms: Vec<_> = surviving
        .into_iter()
        .map(|i| candidates[i].term.clone())
        .collect();
    terms.sort_by(|a, b| a.term.id.cmp(&b.term.id));
    let mut diagnostics = Vec::new();
    let mut conflicts = std::collections::BTreeSet::new();
    for i in 0..terms.len() {
        for j in i + 1..terms.len() {
            if conflicting(&terms[i], &terms[j]) {
                diagnostics.push(super::validation::diagnostic(
                    "glossary_rule_conflict",
                    Some(&terms[i].term.id),
                    format!("Conflicts with {}", terms[j].term.id),
                ));
                conflicts.insert(i);
                conflicts.insert(j);
            }
        }
    }
    for index in conflicts {
        make_unknown(
            &mut terms[index],
            "Conflicting equally specific terminology rules",
        );
    }
    TermSelection { terms, diagnostics }
}
fn make_unknown(term: &mut RelevantTerm, reason: &str) {
    term.applicability = TermApplicability::Unevaluated;
    term.explanation = Some(reason.into());
}
fn conflicting(a: &RelevantTerm, b: &RelevantTerm) -> bool {
    if a.applicability != TermApplicability::Applicable
        || b.applicability != TermApplicability::Applicable
    {
        return false;
    }
    if !a
        .matched_source
        .iter()
        .any(|x| b.matched_source.iter().any(|y| x.nfc().eq(y.nfc())))
    {
        return false;
    }
    let allowed_a = required_forms(a);
    let allowed_b = required_forms(b);
    if let (Some(a), Some(b)) = (&allowed_a, &allowed_b)
        && a.is_disjoint(b)
    {
        return true;
    }
    let forbidden: std::collections::BTreeSet<String> = [a, b]
        .into_iter()
        .filter(|term| !term.ignored_checks.contains(&TermCheck::Forbidden))
        .flat_map(|term| &term.term.forbidden)
        .map(|form| form.nfc().collect())
        .collect();
    [&allowed_a, &allowed_b]
        .into_iter()
        .flatten()
        .any(|forms| forms.is_subset(&forbidden))
}
fn required_forms(term: &RelevantTerm) -> Option<std::collections::BTreeSet<String>> {
    if term.term.do_not_translate {
        if term.ignored_checks.contains(&TermCheck::DoNotTranslate) {
            return None;
        }
        Some(
            term.matched_source
                .iter()
                .chain(&term.accepted_variants)
                .map(|s| s.nfc().collect())
                .collect(),
        )
    } else if !term.term.preferred.is_empty()
        && !term.ignored_checks.contains(&TermCheck::Preferred)
    {
        Some(
            term.accepted_variants
                .iter()
                .map(|s| s.nfc().collect())
                .collect(),
        )
    } else {
        None
    }
}
