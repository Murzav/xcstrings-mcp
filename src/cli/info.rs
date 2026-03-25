use std::path::PathBuf;
use std::process::ExitCode;

use xcstrings_mcp::service::parser;

use super::common::{EXIT_OK, handle_error, load_file};

pub fn run(file: Option<PathBuf>, json: bool) -> ExitCode {
    let (path, parsed) = match load_file(file) {
        Ok(v) => v,
        Err(e) => return handle_error(e),
    };

    let summary = parser::summarize(&parsed);

    if json {
        match serde_json::to_string_pretty(&summary) {
            Ok(out) => println!("{out}"),
            Err(e) => {
                eprintln!("error: failed to serialize: {e}");
                return ExitCode::from(super::common::EXIT_ERROR);
            }
        }
    } else {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());

        println!("File: {file_name}");
        println!("Source language: {}", summary.source_language);
        println!("Total keys: {}", summary.total_keys);
        println!("Translatable keys: {}", summary.translatable_keys);
        println!("Locales: {}", summary.locales.join(", "));

        if !summary.keys_by_state.is_empty() {
            println!("Keys by state:");
            for (state, count) in &summary.keys_by_state {
                println!("  {state}: {count}");
            }
        }
    }

    ExitCode::from(EXIT_OK)
}

#[cfg(test)]
mod tests {
    use xcstrings_mcp::model::translation::FileSummary;

    fn make_summary() -> FileSummary {
        let mut keys_by_state = std::collections::BTreeMap::new();
        keys_by_state.insert("translated".to_string(), 35);
        keys_by_state.insert("manual".to_string(), 5);

        FileSummary {
            source_language: "en".to_string(),
            total_keys: 42,
            translatable_keys: 40,
            locales: vec!["en".to_string(), "uk".to_string(), "de".to_string()],
            keys_by_state,
        }
    }

    #[test]
    fn json_output_is_valid() {
        let summary = make_summary();
        let json = serde_json::to_string_pretty(&summary).expect("serialize");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(parsed["source_language"], "en");
        assert_eq!(parsed["total_keys"], 42);
    }

    #[test]
    fn summary_fields_correct() {
        let summary = make_summary();
        assert_eq!(summary.source_language, "en");
        assert_eq!(summary.total_keys, 42);
        assert_eq!(summary.translatable_keys, 40);
        assert_eq!(summary.locales.len(), 3);
        assert_eq!(summary.keys_by_state["translated"], 35);
    }

    #[test]
    fn empty_keys_by_state() {
        let summary = FileSummary {
            source_language: "en".to_string(),
            total_keys: 0,
            translatable_keys: 0,
            locales: vec![],
            keys_by_state: std::collections::BTreeMap::new(),
        };
        assert!(summary.keys_by_state.is_empty());
        // JSON still valid
        let json = serde_json::to_string_pretty(&summary).expect("serialize");
        assert!(json.contains("\"total_keys\": 0"));
    }
}
