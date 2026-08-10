use std::collections::BTreeMap;

use super::{
    FormatAnalysis, FormatArgument, FormatComparison, FormatComparisonIssue, analyze_format,
    is_supported_unspaced_script,
};

#[derive(Debug, Clone, PartialEq, Eq)]
enum DynamicComponent {
    Fixed(String),
    Dynamic(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ArgumentSignature {
    flags: String,
    width: Option<DynamicComponent>,
    precision: Option<DynamicComponent>,
    length_modifier: Option<String>,
    conversion: char,
}

pub fn compare_formats(source: &str, target: &str) -> FormatComparison {
    compare_formats_with_mode(source, target, false)
}

pub(crate) fn compare_substitution_formats(source: &str, target: &str) -> FormatComparison {
    compare_formats_with_mode(source, target, true)
}

fn compare_formats_with_mode(source: &str, target: &str, substitution: bool) -> FormatComparison {
    let source_analysis = analyze_format(source);
    let target_analysis = analyze_format(target);
    let mut comparison = FormatComparison::default();
    append_analysis_problems("source", &source_analysis, &mut comparison.errors);
    append_analysis_problems("translation", &target_analysis, &mut comparison.errors);

    let source_map = logical_arguments("source", &source_analysis, &mut comparison.errors);
    let target_map = logical_arguments("translation", &target_analysis, &mut comparison.errors);
    if comparison.errors.is_empty() {
        compare_logical_arguments(source_map, target_map, &mut comparison.errors);
    }

    if substitution {
        compare_substitution_placeholders(source, target, &mut comparison.errors);
    }
    compare_ambiguous_sequences(&source_analysis, &target_analysis, &mut comparison.warnings);
    comparison
}

fn compare_logical_arguments(
    source: Option<BTreeMap<u32, ArgumentSignature>>,
    target: Option<BTreeMap<u32, ArgumentSignature>>,
    errors: &mut Vec<FormatComparisonIssue>,
) {
    let (Some(source), Some(target)) = (source, target) else {
        return;
    };
    if source.len() != target.len() {
        errors.push(FormatComparisonIssue {
            code: "format_specifier_count_mismatch",
            message: format!(
                "format specifier count mismatch: source has {} format arguments, translation has {}",
                source.len(),
                target.len()
            ),
        });
        return;
    }
    for (position, source_signature) in &source {
        match target.get(position) {
            Some(target_signature) if target_signature == source_signature => {}
            Some(_) => errors.push(FormatComparisonIssue {
                code: "format_specifier_type_mismatch",
                message: format!(
                    "format specifier type mismatch: argument {position} does not preserve flags, width, precision, length modifier, and conversion"
                ),
            }),
            None => errors.push(FormatComparisonIssue {
                code: "format_specifier_count_mismatch",
                message: format!("format specifier count mismatch: translation is missing argument {position}"),
            }),
        }
    }
}

fn compare_substitution_placeholders(
    source: &str,
    target: &str,
    errors: &mut Vec<FormatComparisonIssue>,
) {
    let source_count = count_substitution_placeholders(source);
    let target_count = count_substitution_placeholders(target);
    if source_count != target_count {
        errors.push(FormatComparisonIssue {
            code: "substitution_placeholder_mismatch",
            message: format!(
                "substitution placeholder mismatch: source has {source_count}, translation has {target_count}"
            ),
        });
    }
}

fn count_substitution_placeholders(text: &str) -> usize {
    let mut cursor = 0;
    let mut count = 0;
    while cursor < text.len() {
        let Some(relative) = text[cursor..].find('%') else {
            break;
        };
        let run_start = cursor + relative;
        let run_length = text[run_start..]
            .bytes()
            .take_while(|byte| *byte == b'%')
            .count();
        if run_length % 2 == 1 {
            let candidate = run_start + run_length - 1;
            let after = candidate + "%arg".len();
            if text[candidate..].starts_with("%arg")
                && text[after..].chars().next().is_none_or(|value| {
                    is_supported_unspaced_script(value)
                        || (!value.is_alphanumeric() && value != '_')
                })
            {
                count += 1;
                cursor = after;
                continue;
            }
        }
        cursor = run_start + run_length;
    }
    count
}

fn compare_ambiguous_sequences(
    source: &FormatAnalysis,
    target: &FormatAnalysis,
    warnings: &mut Vec<FormatComparisonIssue>,
) {
    let source_values: Vec<&str> = source
        .ambiguous
        .iter()
        .map(|value| value.raw.as_str())
        .collect();
    let target_values: Vec<&str> = target
        .ambiguous
        .iter()
        .map(|value| value.raw.as_str())
        .collect();
    if source_values != target_values {
        warnings.push(FormatComparisonIssue {
            code: "ambiguous_format_sequence_mismatch",
            message: format!(
                "ambiguous percent sequences differ: source {source_values:?}, translation {target_values:?}"
            ),
        });
    }
}

fn append_analysis_problems(
    side: &str,
    analysis: &FormatAnalysis,
    errors: &mut Vec<FormatComparisonIssue>,
) {
    for problem in &analysis.problems {
        let (code, description) = match problem.code {
            "invalid_positional_argument" => {
                ("invalid_positional_argument", "invalid positional argument")
            }
            _ => ("invalid_format_specifier", "invalid format sequence"),
        };
        errors.push(FormatComparisonIssue {
            code,
            message: format!("{side} contains {description} {}", problem.raw),
        });
    }
}

fn logical_arguments(
    side: &str,
    analysis: &FormatAnalysis,
    errors: &mut Vec<FormatComparisonIssue>,
) -> Option<BTreeMap<u32, ArgumentSignature>> {
    let has_positional = analysis.arguments.iter().any(has_any_position);
    let has_sequential = analysis.arguments.iter().any(has_any_sequential_argument);
    if has_positional && has_sequential {
        errors.push(FormatComparisonIssue {
            code: "mixed_positional_arguments",
            message: format!("{side} mixes positional and non-positional format arguments"),
        });
        return None;
    }

    let mut next_position = 1;
    let mut used_positions = Vec::new();
    let mut result = BTreeMap::new();
    for argument in &analysis.arguments {
        let width = resolve_component(
            argument.width.as_deref(),
            argument.width_position,
            has_positional,
            &mut next_position,
            &mut used_positions,
        );
        let precision = resolve_component(
            argument.precision.as_deref(),
            argument.precision_position,
            has_positional,
            &mut next_position,
            &mut used_positions,
        );
        let value_position = if has_positional {
            argument.position.unwrap_or(0)
        } else {
            let position = next_position;
            next_position += 1;
            position
        };
        used_positions.push(value_position);
        result.insert(
            value_position,
            ArgumentSignature {
                flags: argument.flags.clone(),
                width,
                precision,
                length_modifier: argument.length_modifier.clone(),
                conversion: argument.conversion,
            },
        );
    }
    validate_positions(side, &mut used_positions, errors)?;
    Some(result)
}

fn validate_positions(
    side: &str,
    positions: &mut [u32],
    errors: &mut Vec<FormatComparisonIssue>,
) -> Option<()> {
    if positions.contains(&0) {
        errors.push(FormatComparisonIssue {
            code: "missing_positional_argument",
            message: format!("{side} uses positional argument zero or omits a required position"),
        });
        return None;
    }
    positions.sort_unstable();
    if positions.windows(2).any(|pair| pair[0] == pair[1]) {
        errors.push(FormatComparisonIssue {
            code: "duplicate_positional_argument",
            message: format!("{side} uses a positional argument more than once"),
        });
        return None;
    }
    if positions
        .iter()
        .enumerate()
        .any(|(index, position)| *position != index as u32 + 1)
    {
        errors.push(FormatComparisonIssue {
            code: "missing_positional_argument",
            message: format!("{side} has a gap in positional arguments"),
        });
        return None;
    }
    Some(())
}

fn resolve_component(
    component: Option<&str>,
    explicit_position: Option<u32>,
    positional: bool,
    next_position: &mut u32,
    used_positions: &mut Vec<u32>,
) -> Option<DynamicComponent> {
    match component {
        Some("*") => {
            let position = if positional {
                explicit_position.unwrap_or(0)
            } else {
                let position = *next_position;
                *next_position += 1;
                position
            };
            used_positions.push(position);
            Some(DynamicComponent::Dynamic(position))
        }
        Some(value) => Some(DynamicComponent::Fixed(value.to_string())),
        None => None,
    }
}

fn has_any_position(argument: &FormatArgument) -> bool {
    argument.position.is_some()
        || argument.width_position.is_some()
        || argument.precision_position.is_some()
}

fn has_any_sequential_argument(argument: &FormatArgument) -> bool {
    argument.position.is_none()
        || (argument.width.as_deref() == Some("*") && argument.width_position.is_none())
        || (argument.precision.as_deref() == Some("*") && argument.precision_position.is_none())
}
