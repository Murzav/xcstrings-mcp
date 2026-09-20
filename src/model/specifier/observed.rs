use super::{DynamicComponent, analyze_format, append_analysis_problems, logical_arguments};
use crate::model::specifier::FormatComparisonIssue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatArgumentRole {
    Value,
    Width,
    Precision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedFormatArgument {
    pub position: u32,
    pub role: FormatArgumentRole,
    pub raw: String,
    pub format: String,
}

/// Describe actual argument positions using the same normalization as validation.
/// Positional gaps are legal here because a leaf can reference its parent's args.
pub fn observed_format_arguments(
    text: &str,
) -> Result<Vec<ObservedFormatArgument>, Vec<FormatComparisonIssue>> {
    let analysis = analyze_format(text);
    let mut errors = Vec::new();
    append_analysis_problems("source", &analysis, &mut errors);
    let logical = logical_arguments("source", &analysis, &mut errors, true);
    let Some(logical) = logical.filter(|_| errors.is_empty()) else {
        return Err(errors);
    };
    let mut observed = Vec::with_capacity(logical.arguments.len());
    for (argument, occurrence) in analysis.arguments.iter().zip(logical.occurrences) {
        for (component, role) in [
            (occurrence.signature.width, FormatArgumentRole::Width),
            (
                occurrence.signature.precision,
                FormatArgumentRole::Precision,
            ),
        ] {
            if let Some(DynamicComponent::Dynamic(position)) = component {
                observed.push(ObservedFormatArgument {
                    position,
                    role,
                    raw: argument.raw.clone(),
                    format: "%d".into(),
                });
            }
        }
        observed.push(ObservedFormatArgument {
            position: occurrence.position,
            role: FormatArgumentRole::Value,
            raw: argument.raw.clone(),
            format: format!(
                "%{}{}",
                argument.length_modifier.as_deref().unwrap_or(""),
                argument.conversion
            ),
        });
    }
    Ok(observed)
}
