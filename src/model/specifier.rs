use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use unicode_script::{Script, UnicodeScript};

#[path = "specifier/comparison.rs"]
mod comparison;
pub use comparison::compare_formats;
pub(crate) use comparison::compare_substitution_formats;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FormatSpecifier {
    pub raw: String,
    pub position: Option<u32>,
    pub conversion: char,
    pub length_modifier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatSpan {
    pub raw: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatArgument {
    pub raw: String,
    pub start: usize,
    pub end: usize,
    pub position: Option<u32>,
    pub flags: String,
    pub width: Option<String>,
    pub precision: Option<String>,
    pub length_modifier: Option<String>,
    pub conversion: char,
    width_position: Option<u32>,
    precision_position: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatProblem {
    pub code: &'static str,
    pub raw: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FormatAnalysis {
    pub arguments: Vec<FormatArgument>,
    pub ambiguous: Vec<FormatSpan>,
    pub literals: Vec<FormatSpan>,
    pub problems: Vec<FormatProblem>,
}

impl FormatAnalysis {
    pub fn spans(&self) -> Vec<FormatSpan> {
        let mut spans = Vec::with_capacity(
            self.arguments.len() + self.ambiguous.len() + self.literals.len() + self.problems.len(),
        );
        spans.extend(self.arguments.iter().map(|argument| FormatSpan {
            raw: argument.raw.clone(),
            start: argument.start,
            end: argument.end,
        }));
        spans.extend(self.ambiguous.iter().cloned());
        spans.extend(self.literals.iter().cloned());
        spans.extend(self.problems.iter().map(|problem| FormatSpan {
            raw: problem.raw.clone(),
            start: problem.start,
            end: problem.end,
        }));
        spans.sort_by_key(|span| span.start);
        spans
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatComparisonIssue {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FormatComparison {
    pub errors: Vec<FormatComparisonIssue>,
    pub warnings: Vec<FormatComparisonIssue>,
}

struct ParsedCandidate {
    argument: FormatArgument,
    valid_pair: bool,
    invalid_position: bool,
}

pub fn analyze_format(text: &str) -> FormatAnalysis {
    let mut analysis = FormatAnalysis::default();
    let mut cursor = 0;

    while cursor < text.len() {
        let Some(relative) = text[cursor..].find('%') else {
            break;
        };
        let percent = cursor + relative;
        let run = text[percent..]
            .bytes()
            .take_while(|byte| *byte == b'%')
            .count();
        let paired = run / 2;
        for pair in 0..paired {
            let start = percent + pair * 2;
            analysis.literals.push(span(text, start, start + 2));
        }

        if run % 2 == 0 {
            cursor = percent + run;
            continue;
        }

        let start = percent + paired * 2;
        match parse_candidate(text, start) {
            Some(candidate) => {
                let valid_pair = candidate.valid_pair;
                let invalid_position = candidate.invalid_position;
                let argument = candidate.argument;
                let end = argument.end;
                let continuation = text[argument.end..].chars().next();
                let followed_by_word =
                    continuation.is_some_and(|ch| ch.is_alphanumeric() || ch == '_');
                let has_definite_shape = valid_pair
                    && (argument.position.is_some()
                        || (argument.length_modifier.is_some() && !argument.flags.contains(' ')));
                let is_unspaced_script_adjacent =
                    continuation.is_some_and(is_supported_unspaced_script);
                if invalid_position {
                    analysis.problems.push(FormatProblem {
                        code: "invalid_positional_argument",
                        raw: argument.raw,
                        start,
                        end: argument.end,
                    });
                } else if followed_by_word
                    && argument.conversion.is_ascii_alphabetic()
                    && !has_definite_shape
                    && !is_unspaced_script_adjacent
                {
                    analysis.ambiguous.push(span(text, start, argument.end));
                } else if valid_pair {
                    analysis.arguments.push(argument);
                } else {
                    analysis.problems.push(FormatProblem {
                        code: "invalid_modifier_conversion",
                        raw: argument.raw,
                        start,
                        end: argument.end,
                    });
                }
                cursor = end;
            }
            None => {
                analysis.literals.push(span(text, start, start + 1));
                cursor = start + 1;
            }
        }
    }

    analysis
}

fn parse_candidate(text: &str, start: usize) -> Option<ParsedCandidate> {
    let mut cursor = start + 1;
    let after_percent = cursor;
    let digit_end = consume_ascii_digits(text, cursor);
    let (position, invalid_value_position) =
        if digit_end > cursor && char_at(text, digit_end) == Some('$') {
            let parsed = parse_position(&text[cursor..digit_end]);
            cursor = digit_end + 1;
            parsed
        } else {
            cursor = after_percent;
            (None, false)
        };

    let flags_start = cursor;
    while char_at(text, cursor).is_some_and(is_flag) {
        cursor += 1;
    }
    let flags = text[flags_start..cursor].to_string();

    let (width, width_position, invalid_width_position, next) = parse_width(text, cursor);
    cursor = next;
    let (precision, precision_position, invalid_precision_position, next) =
        parse_precision(text, cursor);
    cursor = next;
    let (length_modifier, next) = parse_length(text, cursor);
    cursor = next;

    let conversion = char_at(text, cursor).filter(|value| is_conversion(*value))?;
    cursor += conversion.len_utf8();
    let raw = text[start..cursor].to_string();
    let valid_pair = valid_modifier_conversion(length_modifier.as_deref(), conversion);
    Some(ParsedCandidate {
        argument: FormatArgument {
            raw,
            start,
            end: cursor,
            position,
            flags,
            width,
            precision,
            length_modifier,
            conversion,
            width_position,
            precision_position,
        },
        valid_pair,
        invalid_position: invalid_value_position
            || invalid_width_position
            || invalid_precision_position,
    })
}

fn parse_position(digits: &str) -> (Option<u32>, bool) {
    match digits.parse() {
        Ok(value @ 1..) => (Some(value), false),
        Ok(0) | Err(_) => (None, true),
    }
}

fn parse_width(text: &str, cursor: usize) -> (Option<String>, Option<u32>, bool, usize) {
    if char_at(text, cursor) == Some('*') {
        let digits_start = cursor + 1;
        let digits_end = consume_ascii_digits(text, digits_start);
        if digits_end > digits_start && char_at(text, digits_end) == Some('$') {
            let (position, invalid) = parse_position(&text[digits_start..digits_end]);
            return (Some("*".to_string()), position, invalid, digits_end + 1);
        }
        return (Some("*".to_string()), None, false, cursor + 1);
    }
    let end = consume_ascii_digits(text, cursor);
    if end > cursor {
        (Some(text[cursor..end].to_string()), None, false, end)
    } else {
        (None, None, false, cursor)
    }
}

fn parse_precision(text: &str, cursor: usize) -> (Option<String>, Option<u32>, bool, usize) {
    if char_at(text, cursor) != Some('.') {
        return (None, None, false, cursor);
    }
    let value_start = cursor;
    let mut next = cursor + 1;
    if char_at(text, next) == Some('*') {
        let digits_start = next + 1;
        let digits_end = consume_ascii_digits(text, digits_start);
        if digits_end > digits_start && char_at(text, digits_end) == Some('$') {
            let (position, invalid) = parse_position(&text[digits_start..digits_end]);
            return (Some("*".to_string()), position, invalid, digits_end + 1);
        }
        return (Some("*".to_string()), None, false, next + 1);
    }
    next = consume_ascii_digits(text, next);
    (Some(text[value_start..next].to_string()), None, false, next)
}

fn parse_length(text: &str, cursor: usize) -> (Option<String>, usize) {
    for modifier in ["hh", "ll", "h", "l", "q", "L", "z", "t", "j"] {
        if text[cursor..].starts_with(modifier) {
            return (Some(modifier.to_string()), cursor + modifier.len());
        }
    }
    (None, cursor)
}

fn valid_modifier_conversion(modifier: Option<&str>, conversion: char) -> bool {
    match modifier {
        None => true,
        Some("L") => matches!(conversion, 'a' | 'A' | 'e' | 'E' | 'f' | 'F' | 'g' | 'G'),
        Some("h" | "hh" | "l" | "ll" | "q" | "z" | "t" | "j") => {
            matches!(conversion, 'd' | 'o' | 'u' | 'x' | 'X')
        }
        Some(_) => false,
    }
}

fn consume_ascii_digits(text: &str, mut cursor: usize) -> usize {
    while text.as_bytes().get(cursor).is_some_and(u8::is_ascii_digit) {
        cursor += 1;
    }
    cursor
}

fn char_at(text: &str, cursor: usize) -> Option<char> {
    text.get(cursor..)?.chars().next()
}

fn is_flag(value: char) -> bool {
    matches!(value, '-' | '+' | ' ' | '0' | '#' | '\'')
}

fn is_conversion(value: char) -> bool {
    matches!(
        value,
        'd' | 'D'
            | 'i'
            | 'o'
            | 'O'
            | 'u'
            | 'U'
            | 'x'
            | 'X'
            | 'e'
            | 'E'
            | 'f'
            | 'F'
            | 'g'
            | 'G'
            | 'a'
            | 'A'
            | 'c'
            | 'C'
            | 's'
            | 'S'
            | 'p'
            | 'n'
            | '@'
    )
}

fn is_supported_unspaced_script(value: char) -> bool {
    matches!(
        value.script(),
        Script::Han | Script::Hiragana | Script::Katakana | Script::Hangul
    )
}

fn span(text: &str, start: usize, end: usize) -> FormatSpan {
    FormatSpan {
        raw: text[start..end].to_string(),
        start,
        end,
    }
}

pub(crate) fn extract_specifiers(text: &str) -> Vec<FormatSpecifier> {
    analyze_format(text)
        .arguments
        .into_iter()
        .map(|argument| FormatSpecifier {
            raw: argument.raw,
            position: argument.position,
            conversion: argument.conversion,
            length_modifier: argument.length_modifier,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_only_definite_arguments() {
        let specs = extract_specifiers("100% Local: %@ and %2$lld");
        assert_eq!(
            specs
                .iter()
                .map(|value| value.raw.as_str())
                .collect::<Vec<_>>(),
            ["%@", "%2$lld"]
        );
    }

    #[test]
    fn percent_escape_is_not_an_argument() {
        assert!(extract_specifiers("100%% done").is_empty());
    }
}
