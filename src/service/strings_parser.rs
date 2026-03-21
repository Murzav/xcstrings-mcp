use std::path::{Path, PathBuf};

use crate::error::XcStringsError;

#[derive(Debug)]
pub struct StringsEntry {
    pub key: String,
    pub value: String,
    pub comment: Option<String>,
}

pub struct DiscoveredStringsFile {
    pub path: PathBuf,
    pub locale: String,
    pub table_name: String,
    pub file_type: StringsFileType,
}

pub enum StringsFileType {
    Strings,
    Stringsdict,
}

fn parse_err(line: usize, message: impl Into<String>) -> XcStringsError {
    XcStringsError::StringsParse {
        line,
        message: message.into(),
    }
}

/// Decode raw bytes detecting BOM: UTF-16LE/BE, UTF-8, with UTF-16LE fallback.
pub fn decode_strings_content(raw: &[u8]) -> Result<String, XcStringsError> {
    if raw.len() >= 2 && raw[0] == 0xFF && raw[1] == 0xFE {
        return decode_utf16(raw, 2, u16::from_le_bytes);
    }
    if raw.len() >= 2 && raw[0] == 0xFE && raw[1] == 0xFF {
        return decode_utf16(raw, 2, u16::from_be_bytes);
    }
    if raw.len() >= 3 && raw[0] == 0xEF && raw[1] == 0xBB && raw[2] == 0xBF {
        return String::from_utf8(raw[3..].to_vec())
            .map_err(|e| parse_err(0, format!("invalid UTF-8 after BOM: {e}")));
    }
    // Heuristic: UTF-16LE without BOM has null bytes at odd positions for ASCII content.
    // String::from_utf8 accepts null bytes, so check for the pattern first.
    if raw.len() >= 2 && raw.len().is_multiple_of(2) && looks_like_utf16le(raw) {
        return decode_utf16(raw, 0, u16::from_le_bytes);
    }
    String::from_utf8(raw.to_vec()).map_err(|e| parse_err(0, format!("invalid encoding: {e}")))
}

/// Check if raw bytes look like UTF-16LE: ASCII chars at even positions, null at odd positions.
fn looks_like_utf16le(raw: &[u8]) -> bool {
    // Sample the first few byte pairs
    let sample = raw.len().min(20);
    if sample < 2 {
        return false;
    }
    let mut null_at_odd = 0;
    let mut pairs = 0;
    for chunk in raw[..sample].chunks_exact(2) {
        pairs += 1;
        if chunk[1] == 0 && chunk[0] != 0 {
            null_at_odd += 1;
        }
    }
    // If most odd bytes are null, it's likely UTF-16LE
    pairs > 0 && null_at_odd * 2 >= pairs
}

fn decode_utf16(
    raw: &[u8],
    skip: usize,
    conv: fn([u8; 2]) -> u16,
) -> Result<String, XcStringsError> {
    let data = &raw[skip..];
    if !data.len().is_multiple_of(2) {
        return Err(parse_err(0, "odd byte count for UTF-16 data"));
    }
    let units: Vec<u16> = data.chunks_exact(2).map(|c| conv([c[0], c[1]])).collect();
    String::from_utf16(&units).map_err(|e| parse_err(0, format!("invalid UTF-16: {e}")))
}

#[derive(Clone, Copy)]
enum State {
    Idle,
    InBlockComment,
    InLineComment,
    InQuotedKey,
    InUnquotedKey,
    ExpectingEquals,
    InQuotedValue,
}

/// Parse `.strings` file content into entries.
pub fn parse_strings(content: &str) -> Result<Vec<StringsEntry>, XcStringsError> {
    let mut entries = Vec::new();
    let mut state = State::Idle;
    let mut line: usize = 1;
    let (mut key, mut value, mut comment_buf) = (String::new(), String::new(), String::new());
    let mut pending_comment: Option<String> = None;
    let mut escape = false;
    let chars: Vec<char> = content.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let ch = chars[i];
        if ch == '\n' {
            line += 1;
        }
        match state {
            State::Idle => {
                if ch == '/' && i + 1 < len && chars[i + 1] == '*' {
                    state = State::InBlockComment;
                    comment_buf.clear();
                    i += 2;
                    continue;
                }
                if ch == '/' && i + 1 < len && chars[i + 1] == '/' {
                    state = State::InLineComment;
                    comment_buf.clear();
                    i += 2;
                    continue;
                }
                if ch == '"' {
                    state = State::InQuotedKey;
                    key.clear();
                    escape = false;
                    i += 1;
                    continue;
                }
                if ch.is_alphanumeric() || ch == '_' {
                    state = State::InUnquotedKey;
                    key.clear();
                    key.push(ch);
                    i += 1;
                    continue;
                }
                i += 1;
            }
            State::InBlockComment => {
                if ch == '*' && i + 1 < len && chars[i + 1] == '/' {
                    let trimmed = comment_buf.trim();
                    pending_comment = if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_owned())
                    };
                    state = State::Idle;
                    i += 2;
                    continue;
                }
                comment_buf.push(ch);
                i += 1;
            }
            State::InLineComment => {
                if ch == '\n' {
                    let t = comment_buf.trim();
                    pending_comment = if t.starts_with("MARK:") {
                        None
                    } else {
                        Some(t.to_owned())
                    };
                    state = State::Idle;
                    i += 1;
                    continue;
                }
                comment_buf.push(ch);
                i += 1;
            }
            State::InQuotedKey => {
                if escape {
                    push_esc(ch, &mut key, &mut i, &chars, line)?;
                    escape = false;
                    continue;
                }
                if ch == '\\' {
                    escape = true;
                    i += 1;
                    continue;
                }
                if ch == '"' {
                    state = State::ExpectingEquals;
                    i += 1;
                    continue;
                }
                key.push(ch);
                i += 1;
            }
            State::InUnquotedKey => {
                if ch.is_alphanumeric() || ch == '_' || ch == '.' || ch == '-' {
                    key.push(ch);
                    i += 1;
                    continue;
                }
                state = State::ExpectingEquals;
            }
            State::ExpectingEquals => {
                if ch.is_whitespace() {
                    i += 1;
                    continue;
                }
                if ch == '=' {
                    i += 1;
                    while i < len && chars[i].is_whitespace() {
                        if chars[i] == '\n' {
                            line += 1;
                        }
                        i += 1;
                    }
                    if i >= len || chars[i] != '"' {
                        return Err(parse_err(line, "expected '\"' after '='"));
                    }
                    state = State::InQuotedValue;
                    value.clear();
                    escape = false;
                    i += 1;
                    continue;
                }
                return Err(parse_err(
                    line,
                    format!("expected '=' after key, found '{ch}'"),
                ));
            }
            State::InQuotedValue => {
                if escape {
                    push_esc(ch, &mut value, &mut i, &chars, line)?;
                    escape = false;
                    continue;
                }
                if ch == '\\' {
                    escape = true;
                    i += 1;
                    continue;
                }
                if ch == '"' {
                    i += 1;
                    while i < len && chars[i].is_whitespace() {
                        if chars[i] == '\n' {
                            line += 1;
                        }
                        i += 1;
                    }
                    if i >= len || chars[i] != ';' {
                        return Err(parse_err(line, "missing ';' after value"));
                    }
                    entries.push(StringsEntry {
                        key: key.clone(),
                        value: value.clone(),
                        comment: pending_comment.take(),
                    });
                    state = State::Idle;
                    i += 1;
                    continue;
                }
                value.push(ch);
                i += 1;
            }
        }
    }
    if matches!(state, State::InLineComment) { /* trailing comment — ok */
    } else if !matches!(state, State::Idle) {
        return Err(parse_err(line, "unexpected end of input"));
    }
    Ok(entries)
}

fn push_esc(
    ch: char,
    buf: &mut String,
    i: &mut usize,
    chars: &[char],
    line: usize,
) -> Result<(), XcStringsError> {
    match ch {
        '"' => buf.push('"'),
        '\\' => buf.push('\\'),
        'n' => buf.push('\n'),
        't' => buf.push('\t'),
        'r' => buf.push('\r'),
        'U' => {
            *i += 1;
            let code = hex4(chars, *i, line)?;
            *i += 4;
            if (0xD800..=0xDBFF).contains(&code) {
                if *i + 1 < chars.len()
                    && chars[*i] == '\\'
                    && *i + 2 < chars.len()
                    && chars[*i + 1] == 'U'
                {
                    let low = hex4(chars, *i + 2, line)?;
                    if (0xDC00..=0xDFFF).contains(&low) {
                        let cp = 0x10000 + ((code as u32 - 0xD800) << 10) + (low as u32 - 0xDC00);
                        buf.push(char::from_u32(cp).ok_or_else(|| {
                            parse_err(
                                line,
                                format!("invalid surrogate pair: U+{code:04X} U+{low:04X}"),
                            )
                        })?);
                        *i += 6;
                        return Ok(());
                    }
                }
                return Err(parse_err(
                    line,
                    format!("high surrogate U+{code:04X} without low surrogate"),
                ));
            }
            buf.push(
                char::from_u32(code as u32)
                    .ok_or_else(|| parse_err(line, format!("invalid unicode: U+{code:04X}")))?,
            );
            return Ok(());
        }
        _ => {
            buf.push('\\');
            buf.push(ch);
        }
    }
    *i += 1;
    Ok(())
}

fn hex4(chars: &[char], start: usize, line: usize) -> Result<u16, XcStringsError> {
    if start + 4 > chars.len() {
        return Err(parse_err(line, "incomplete \\U escape: need 4 hex digits"));
    }
    let h: String = chars[start..start + 4].iter().collect();
    u16::from_str_radix(&h, 16)
        .map_err(|_| parse_err(line, format!("invalid hex in \\U escape: {h}")))
}

/// Extract locale from `.lproj` parent directory in path.
pub fn extract_locale_from_path(path: &Path) -> Result<String, XcStringsError> {
    for comp in path.components().rev() {
        if let std::path::Component::Normal(name) = comp
            && let Some(locale) = name.to_string_lossy().strip_suffix(".lproj")
        {
            return Ok(locale.to_owned());
        }
    }
    Err(parse_err(
        0,
        format!("no .lproj directory found in path: {}", path.display()),
    ))
}

/// Recursively discover `.strings` and `.stringsdict` files under `.lproj` directories.
pub fn discover_strings_files(root: &Path) -> Result<Vec<DiscoveredStringsFile>, XcStringsError> {
    let mut results = Vec::new();
    walk_lproj(root, &mut results, 0)?;
    results.sort_by(|a, b| {
        a.table_name
            .cmp(&b.table_name)
            .then(a.locale.cmp(&b.locale))
    });
    Ok(results)
}

fn walk_lproj(
    dir: &Path,
    out: &mut Vec<DiscoveredStringsFile>,
    depth: usize,
) -> Result<(), XcStringsError> {
    const MAX_DEPTH: usize = 20;
    if depth > MAX_DEPTH {
        return Ok(()); // silently stop — deep nesting is not expected in iOS projects
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if name.ends_with(".lproj") && name != "Base.lproj" {
                let locale = name.strip_suffix(".lproj").unwrap_or(&name).to_owned();
                for f in std::fs::read_dir(&path)? {
                    let fp = f?.path();
                    if !fp.is_file() {
                        continue;
                    }
                    let ft = match fp.extension().and_then(|e| e.to_str()) {
                        Some("strings") => StringsFileType::Strings,
                        Some("stringsdict") => StringsFileType::Stringsdict,
                        _ => continue,
                    };
                    let tbl = fp
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Unknown")
                        .to_owned();
                    out.push(DiscoveredStringsFile {
                        path: fp,
                        locale: locale.clone(),
                        table_name: tbl,
                        file_type: ft,
                    });
                }
            } else if !name.ends_with(".lproj") {
                walk_lproj(&path, out, depth + 1)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn decode_utf8_no_bom() {
        assert_eq!(
            decode_strings_content(b"\"k\" = \"v\";").unwrap(),
            "\"k\" = \"v\";"
        );
    }
    #[test]
    fn decode_utf8_with_bom() {
        let mut b = vec![0xEF, 0xBB, 0xBF];
        b.extend_from_slice(b"\"k\"=\"v\";");
        assert_eq!(decode_strings_content(&b).unwrap(), "\"k\"=\"v\";");
    }
    #[test]
    fn decode_utf16le_with_bom() {
        let t = "\"k\" = \"v\";";
        let mut b = vec![0xFF, 0xFE];
        for u in t.encode_utf16() {
            b.extend_from_slice(&u.to_le_bytes());
        }
        assert_eq!(decode_strings_content(&b).unwrap(), t);
    }
    #[test]
    fn decode_utf16be_with_bom() {
        let t = "\"k\" = \"v\";";
        let mut b = vec![0xFE, 0xFF];
        for u in t.encode_utf16() {
            b.extend_from_slice(&u.to_be_bytes());
        }
        assert_eq!(decode_strings_content(&b).unwrap(), t);
    }
    #[test]
    fn decode_utf16le_no_bom_fallback() {
        let t = "\"k\" = \"v\";";
        let mut b = Vec::new();
        for u in t.encode_utf16() {
            b.extend_from_slice(&u.to_le_bytes());
        }
        assert_eq!(decode_strings_content(&b).unwrap(), t);
    }
    #[test]
    fn decode_invalid_encoding() {
        assert!(decode_strings_content(&[0xFF, 0xFF, 0xFF]).is_err());
    }

    #[test]
    fn parse_basic_key_value() {
        let e = parse_strings("\"hello\" = \"world\";").unwrap();
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].key, "hello");
        assert_eq!(e[0].value, "world");
        assert!(e[0].comment.is_none());
    }
    #[test]
    fn parse_block_comment_attached() {
        let e = parse_strings("/* A greeting */\n\"hello\" = \"world\";").unwrap();
        assert_eq!(e[0].comment.as_deref(), Some("A greeting"));
    }
    #[test]
    fn parse_line_comment_attached() {
        let e = parse_strings("// A greeting\n\"hello\" = \"world\";").unwrap();
        assert_eq!(e[0].comment.as_deref(), Some("A greeting"));
    }
    #[test]
    fn parse_mark_comment_not_attached() {
        let e = parse_strings("// MARK: Section\n\"hello\" = \"world\";").unwrap();
        assert!(e[0].comment.is_none());
    }
    #[test]
    fn parse_escape_sequences() {
        let e = parse_strings(r#""key" = "a\"b\\c\nd\te\rf";"#).unwrap();
        assert_eq!(e[0].value, "a\"b\\c\nd\te\rf");
    }
    #[test]
    fn parse_unicode_escape() {
        let e = parse_strings(r#""key" = "\U00E9";"#).unwrap();
        assert_eq!(e[0].value, "é");
    }
    #[test]
    fn parse_unicode_surrogate_pair() {
        let e = parse_strings(r#""key" = "\UD83D\UDE00";"#).unwrap();
        assert_eq!(e[0].value, "\u{1F600}");
    }
    #[test]
    fn parse_empty_value() {
        assert_eq!(parse_strings("\"key\" = \"\";").unwrap()[0].value, "");
    }
    #[test]
    fn parse_multiple_entries_mixed_comments() {
        let e =
            parse_strings("/* First */\n\"a\" = \"1\";\n// Second\n\"b\" = \"2\";\n\"c\" = \"3\";")
                .unwrap();
        assert_eq!(e.len(), 3);
        assert_eq!(e[0].comment.as_deref(), Some("First"));
        assert_eq!(e[1].comment.as_deref(), Some("Second"));
        assert!(e[2].comment.is_none());
    }
    #[test]
    fn parse_duplicate_keys() {
        let e = parse_strings("\"key\" = \"first\";\n\"key\" = \"second\";").unwrap();
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].value, "first");
        assert_eq!(e[1].value, "second");
    }
    #[test]
    fn parse_missing_semicolon() {
        assert!(parse_strings("\"key\" = \"value\"").is_err());
    }
    #[test]
    fn parse_empty_input() {
        assert!(parse_strings("").unwrap().is_empty());
    }
    #[test]
    fn parse_unquoted_key() {
        let e = parse_strings("myKey = \"value\";").unwrap();
        assert_eq!(e[0].key, "myKey");
        assert_eq!(e[0].value, "value");
    }
    #[test]
    fn parse_unquoted_key_with_dots() {
        assert_eq!(
            parse_strings("my.key.name = \"value\";").unwrap()[0].key,
            "my.key.name"
        );
    }

    #[test]
    fn extract_locale_valid() {
        assert_eq!(
            extract_locale_from_path(Path::new("/p/en.lproj/L.strings")).unwrap(),
            "en"
        );
    }
    #[test]
    fn extract_locale_invalid() {
        assert!(extract_locale_from_path(Path::new("/p/Resources/L.strings")).is_err());
    }

    #[test]
    fn discover_with_lproj_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("en.lproj")).unwrap();
        fs::create_dir(tmp.path().join("es.lproj")).unwrap();
        fs::write(tmp.path().join("en.lproj/Localizable.strings"), "").unwrap();
        fs::write(tmp.path().join("es.lproj/Localizable.strings"), "").unwrap();
        let f = discover_strings_files(tmp.path()).unwrap();
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].table_name, "Localizable");
    }
    #[test]
    fn discover_both_file_types() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("en.lproj")).unwrap();
        fs::write(tmp.path().join("en.lproj/L.strings"), "").unwrap();
        fs::write(tmp.path().join("en.lproj/L.stringsdict"), "").unwrap();
        assert_eq!(discover_strings_files(tmp.path()).unwrap().len(), 2);
    }
    #[test]
    fn discover_multiple_tables() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("en.lproj")).unwrap();
        fs::write(tmp.path().join("en.lproj/Localizable.strings"), "").unwrap();
        fs::write(tmp.path().join("en.lproj/InfoPlist.strings"), "").unwrap();
        let f = discover_strings_files(tmp.path()).unwrap();
        assert_eq!(f[0].table_name, "InfoPlist");
        assert_eq!(f[1].table_name, "Localizable");
    }
    #[test]
    fn discover_no_lproj() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(discover_strings_files(tmp.path()).unwrap().is_empty());
    }
    #[test]
    fn discover_nested_directories() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("Resources/en.lproj")).unwrap();
        fs::write(tmp.path().join("Resources/en.lproj/L.strings"), "").unwrap();
        assert_eq!(discover_strings_files(tmp.path()).unwrap().len(), 1);
    }

    #[test]
    fn empty_block_comment_produces_none() {
        let e = parse_strings("/**/\n\"hello\" = \"world\";").unwrap();
        assert_eq!(e.len(), 1);
        assert!(
            e[0].comment.is_none(),
            "empty block comment should not attach as comment"
        );
    }

    #[test]
    fn whitespace_only_block_comment_produces_none() {
        let e = parse_strings("/*   */\n\"hello\" = \"world\";").unwrap();
        assert_eq!(e.len(), 1);
        assert!(
            e[0].comment.is_none(),
            "whitespace-only block comment should not attach as comment"
        );
    }

    #[test]
    fn test_unknown_escape_passthrough() {
        let e = parse_strings(r#""key" = "hello\pworld";"#).unwrap();
        assert_eq!(e[0].value, "hello\\pworld");
    }

    #[test]
    fn escape_error_reports_correct_line() {
        // Incomplete \U escape on line 3
        let input = "\"a\" = \"ok\";\n\"b\" = \"ok\";\n\"c\" = \"\\U00G\";";
        let err = match parse_strings(input) {
            Err(e) => e,
            Ok(_) => panic!("expected error on invalid \\U escape"),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("line 3"),
            "expected error on line 3, got: {msg}"
        );
    }

    #[test]
    fn discover_skips_base_lproj() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("Base.lproj")).unwrap();
        fs::create_dir(tmp.path().join("en.lproj")).unwrap();
        fs::write(tmp.path().join("Base.lproj/Main.strings"), "").unwrap();
        fs::write(tmp.path().join("en.lproj/Localizable.strings"), "").unwrap();
        let f = discover_strings_files(tmp.path()).unwrap();
        assert_eq!(f.len(), 1, "Base.lproj should be skipped");
        assert_eq!(f[0].locale, "en");
    }

    #[test]
    fn discover_respects_max_depth() {
        let tmp = tempfile::tempdir().unwrap();
        // Create a path 22 levels deep with an lproj at the bottom
        let mut deep = tmp.path().to_path_buf();
        for i in 0..22 {
            deep = deep.join(format!("d{i}"));
        }
        let lproj = deep.join("en.lproj");
        fs::create_dir_all(&lproj).unwrap();
        fs::write(lproj.join("L.strings"), "").unwrap();
        let f = discover_strings_files(tmp.path()).unwrap();
        assert!(
            f.is_empty(),
            "files beyond MAX_DEPTH should not be discovered"
        );
    }
}
