use std::collections::BTreeMap;

use super::{
    FormatAnalysis, FormatArgument, FormatComparison, FormatComparisonIssue, analyze_format,
    is_supported_unspaced_script,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum DynamicComponent {
    Fixed(String),
    Dynamic(u32),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ArgumentSignature {
    flags: String,
    width: Option<DynamicComponent>,
    precision: Option<DynamicComponent>,
    length_modifier: Option<String>,
    conversion: char,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArgumentType {
    Int,
    UnsignedInt,
    Long,
    UnsignedLong,
    LongLong,
    UnsignedLongLong,
    IntMax,
    UnsignedIntMax,
    SignedSize,
    Size,
    PtrDiff,
    Double,
    LongDouble,
    Object,
    CharPointer,
    Utf16Pointer,
    VoidPointer,
    CountPointer,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FormatOccurrence {
    position: u32,
    signature: ArgumentSignature,
}

struct LogicalFormat {
    arguments: BTreeMap<u32, ArgumentType>,
    occurrences: Vec<FormatOccurrence>,
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
    source: Option<LogicalFormat>,
    target: Option<LogicalFormat>,
    errors: &mut Vec<FormatComparisonIssue>,
) {
    let (Some(source), Some(target)) = (source, target) else {
        return;
    };
    if source.arguments.len() != target.arguments.len() {
        errors.push(FormatComparisonIssue {
            code: "format_specifier_count_mismatch",
            message: format!(
                "format specifier count mismatch: source has {} format arguments, translation has {}",
                source.arguments.len(),
                target.arguments.len()
            ),
        });
        return;
    }
    for (position, source_type) in &source.arguments {
        match target.arguments.get(position) {
            Some(target_type) if target_type == source_type => {}
            Some(_) => errors.push(FormatComparisonIssue {
                code: "format_specifier_type_mismatch",
                message: format!(
                    "format specifier type mismatch: argument {position} has an incompatible type"
                ),
            }),
            None => errors.push(FormatComparisonIssue {
                code: "format_specifier_count_mismatch",
                message: format!(
                    "format specifier count mismatch: translation is missing argument {position}"
                ),
            }),
        }
    }
    if !errors.is_empty() {
        return;
    }
    if source.occurrences.len() != target.occurrences.len() {
        errors.push(FormatComparisonIssue {
            code: "format_specifier_count_mismatch",
            message: format!(
                "format specifier count mismatch: source has {} format occurrences, translation has {}",
                source.occurrences.len(),
                target.occurrences.len()
            ),
        });
        return;
    }
    let mut source_occurrences = source.occurrences;
    let mut target_occurrences = target.occurrences;
    source_occurrences.sort_unstable();
    target_occurrences.sort_unstable();
    if source_occurrences != target_occurrences {
        errors.push(FormatComparisonIssue {
            code: "format_specifier_type_mismatch",
            message: "format specifier type mismatch: translation does not preserve each format occurrence's position, flags, width, precision, length modifier, and conversion".to_string(),
        });
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
) -> Option<LogicalFormat> {
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
    let mut arguments = BTreeMap::new();
    let mut occurrences = Vec::with_capacity(analysis.arguments.len());
    let mut compatible = true;
    let mut incompatibility_reported = false;
    for argument in &analysis.arguments {
        let width = resolve_component(
            argument.width.as_deref(),
            argument.width_position,
            has_positional,
            &mut next_position,
        );
        let precision = resolve_component(
            argument.precision.as_deref(),
            argument.precision_position,
            has_positional,
            &mut next_position,
        );
        let value_position = if has_positional {
            argument.position.unwrap_or(0)
        } else {
            let position = next_position;
            next_position += 1;
            position
        };
        if let Some(DynamicComponent::Dynamic(position)) = width {
            compatible &= register_argument(
                side,
                position,
                ArgumentType::Int,
                &mut arguments,
                errors,
                &mut incompatibility_reported,
            );
        }
        if let Some(DynamicComponent::Dynamic(position)) = precision {
            compatible &= register_argument(
                side,
                position,
                ArgumentType::Int,
                &mut arguments,
                errors,
                &mut incompatibility_reported,
            );
        }
        let Some(value_type) = argument_type(argument) else {
            errors.push(FormatComparisonIssue {
                code: "invalid_format_specifier",
                message: format!(
                    "{side} contains an unsupported format argument {}",
                    argument.raw
                ),
            });
            compatible = false;
            continue;
        };
        compatible &= register_argument(
            side,
            value_position,
            value_type,
            &mut arguments,
            errors,
            &mut incompatibility_reported,
        );
        occurrences.push(FormatOccurrence {
            position: value_position,
            signature: ArgumentSignature {
                flags: argument.flags.clone(),
                width,
                precision,
                length_modifier: argument.length_modifier.clone(),
                conversion: argument.conversion,
            },
        });
    }
    validate_positions(side, &arguments, errors)?;
    compatible.then_some(LogicalFormat {
        arguments,
        occurrences,
    })
}

fn validate_positions(
    side: &str,
    arguments: &BTreeMap<u32, ArgumentType>,
    errors: &mut Vec<FormatComparisonIssue>,
) -> Option<()> {
    if arguments.contains_key(&0) {
        errors.push(FormatComparisonIssue {
            code: "missing_positional_argument",
            message: format!("{side} uses positional argument zero or omits a required position"),
        });
        return None;
    }
    if arguments
        .keys()
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

fn register_argument(
    side: &str,
    position: u32,
    argument_type: ArgumentType,
    arguments: &mut BTreeMap<u32, ArgumentType>,
    errors: &mut Vec<FormatComparisonIssue>,
    incompatibility_reported: &mut bool,
) -> bool {
    if let Some(existing) = arguments.get(&position) {
        if *existing == argument_type {
            return true;
        }
        if !*incompatibility_reported {
            errors.push(FormatComparisonIssue {
                code: "incompatible_positional_argument",
                message: format!(
                    "{side} reuses positional argument {position} with incompatible argument types"
                ),
            });
            *incompatibility_reported = true;
        }
        return false;
    }
    arguments.insert(position, argument_type);
    true
}

fn argument_type(argument: &FormatArgument) -> Option<ArgumentType> {
    use ArgumentType::{
        CharPointer, CountPointer, Double, Int, IntMax, Long, LongDouble, LongLong, Object,
        PtrDiff, SignedSize, Size, UnsignedInt, UnsignedIntMax, UnsignedLong, UnsignedLongLong,
        Utf16Pointer, VoidPointer,
    };

    // POSIX defines dynamic width/precision as `int`. Foundation's `h`/`hh`
    // integer values undergo the default integer promotions, so they consume
    // that same ABI type even for an unsigned narrow conversion.
    let argument_type = match (argument.length_modifier.as_deref(), argument.conversion) {
        (None, '@') => Object,
        (None, 'd' | 'D' | 'i' | 'c' | 'C') | (Some("h" | "hh"), 'd' | 'o' | 'u' | 'x' | 'X') => {
            Int
        }
        (None, 'o' | 'O' | 'u' | 'U' | 'x' | 'X') => UnsignedInt,
        (Some("l"), 'd') => Long,
        (Some("l"), 'o' | 'u' | 'x' | 'X') => UnsignedLong,
        (Some("ll" | "q"), 'd') => LongLong,
        (Some("ll" | "q"), 'o' | 'u' | 'x' | 'X') => UnsignedLongLong,
        (Some("j"), 'd') => IntMax,
        (Some("j"), 'o' | 'u' | 'x' | 'X') => UnsignedIntMax,
        (Some("z"), 'd') => SignedSize,
        (Some("z"), 'o' | 'u' | 'x' | 'X') => Size,
        (Some("t"), 'd') => PtrDiff,
        // Apple's positional extractor fetches unsigned z/t values as the same T_SIZET role.
        (Some("t"), 'o' | 'u' | 'x' | 'X') => Size,
        (None, 'a' | 'A' | 'e' | 'E' | 'f' | 'F' | 'g' | 'G') => Double,
        (Some("L"), 'a' | 'A' | 'e' | 'E' | 'f' | 'F' | 'g' | 'G') => LongDouble,
        (None, 's') => CharPointer,
        (None, 'S') => Utf16Pointer,
        (None, 'p') => VoidPointer,
        (None, 'n') => CountPointer,
        _ => return None,
    };
    Some(argument_type)
}

fn resolve_component(
    component: Option<&str>,
    explicit_position: Option<u32>,
    positional: bool,
    next_position: &mut u32,
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
