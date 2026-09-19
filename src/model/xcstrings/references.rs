//! Parse named substitution references without changing their positional identity.
#[derive(Debug)]
pub struct SubstitutionReference {
    pub start: usize,
    pub end: usize,
    pub name: String,
    pub position: Option<u32>,
}

pub fn substitution_references(text: &str) -> Result<Vec<SubstitutionReference>, String> {
    let bytes = text.as_bytes();
    let mut result = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            i += 1;
            continue;
        }
        if bytes.get(i + 1) == Some(&b'%') {
            i += 2;
            continue;
        }
        let start = i;
        i += 1;
        let digits = i;
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        let position = if i > digits && bytes.get(i) == Some(&b'$') {
            let n = text[digits..i]
                .parse::<u32>()
                .map_err(|_| "invalid substitution position")?;
            if n == 0 {
                return Err("substitution argument positions begin at 1".into());
            }
            i += 1;
            Some(n)
        } else {
            i = digits;
            None
        };
        if !text[i..].starts_with("#@") {
            continue;
        }
        let name_start = i + 2;
        let end = text[name_start..]
            .find('@')
            .ok_or("unterminated substitution reference")?
            + name_start;
        if end == name_start {
            return Err("empty substitution name".into());
        }
        result.push(SubstitutionReference {
            start,
            end: end + 1,
            name: text[name_start..end].into(),
            position,
        });
        i = end + 1;
    }
    Ok(result)
}
