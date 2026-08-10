//! Synthetic scale acceptance only. This is not a replay of unavailable source catalogs.

use serde_json::{Map, Value, json};
use xcstrings_mcp::service::semantic_merge::{
    ConflictChoice, ConflictResolution, MergeOptions, prepare_merge,
};

const BASE_KEYS: usize = 2_574;
const CURRENT_ONLY_KEYS: usize = 133;
const RETRY_CONFLICTS: usize = 5;

fn complete_entry(value: String) -> Value {
    json!({
        "localizations": {
            "en": {"stringUnit": {"state": "translated", "value": value}},
            "de": {"stringUnit": {"state": "translated", "value": format!("de:{value}")}}
        }
    })
}

fn retry_entry(index: usize, value: String) -> Value {
    json!({
        "localizations": {
            "en": {"stringUnit": {"state": "translated", "value": value}},
            "de": {"stringUnit": {"state": "translated", "value": format!("de:base value {index}")}}
        }
    })
}

fn base_strings() -> Map<String, Value> {
    (0..BASE_KEYS)
        .map(|index| {
            let key = if index < RETRY_CONFLICTS {
                format!("Retry.formulation.{index}")
            } else {
                format!("existing.{index:04}")
            };
            (key, complete_entry(format!("base value {index}")))
        })
        .collect()
}

fn catalog(strings: Map<String, Value>) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "sourceLanguage": "en",
        "strings": strings,
        "version": "1.0"
    }))
    .unwrap()
}

#[test]
fn synthetic_2574_plus_133_merge_uses_exactly_five_retry_resolutions() {
    let base_map = base_strings();
    let mut current_map = base_map.clone();
    let mut incoming_map = base_map.clone();
    (0..RETRY_CONFLICTS).for_each(|index| {
        current_map.insert(
            format!("Retry.formulation.{index}"),
            retry_entry(index, format!("current retry {index}")),
        );
        incoming_map.insert(
            format!("Retry.formulation.{index}"),
            retry_entry(index, format!("incoming retry {index}")),
        );
    });
    current_map.extend((0..CURRENT_ONLY_KEYS).map(|index| {
        (
            format!("current.only.{index:03}"),
            complete_entry(format!("new value {index}")),
        )
    }));
    // A friend-side non-conflicting edit proves the incoming catalog is not ignored.
    incoming_map.insert(
        "existing.0100".into(),
        complete_entry("incoming friend edit".into()),
    );

    let base = catalog(base_map);
    let current = catalog(current_map);
    let incoming = catalog(incoming_map);
    let dry = prepare_merge(&base, &current, &incoming, &MergeOptions::default()).unwrap();
    assert_eq!(dry.report.conflict_total, RETRY_CONFLICTS);
    assert_eq!(dry.report.unresolved_conflict_total, RETRY_CONFLICTS);
    assert_eq!(
        dry.report
            .conflicts
            .iter()
            .filter(|conflict| conflict.pointer.contains("/Retry.formulation."))
            .count(),
        RETRY_CONFLICTS
    );

    let resolutions = dry
        .report
        .conflicts
        .iter()
        .map(|conflict| ConflictResolution {
            conflict_id: conflict.id.clone(),
            choice: ConflictChoice::Current,
        })
        .collect::<Vec<_>>();
    assert_eq!(resolutions.len(), RETRY_CONFLICTS);
    let merged = prepare_merge(
        &base,
        &current,
        &incoming,
        &MergeOptions {
            resolutions,
            ..MergeOptions::default()
        },
    )
    .unwrap();
    let result: Value = serde_json::from_str(&merged.content).unwrap();
    let strings = result["strings"].as_object().unwrap();
    assert_eq!(strings.len(), BASE_KEYS + CURRENT_ONLY_KEYS);
    assert_eq!(merged.report.fingerprints.result.key_count, 2_707);
    assert_eq!(merged.report.resolutions_applied, RETRY_CONFLICTS);
    assert_eq!(merged.report.unresolved_conflict_total, 0);
    assert_eq!(
        strings
            .values()
            .filter(|entry| {
                entry["localizations"]["en"]["stringUnit"]["state"] == "translated"
                    && entry["localizations"]["de"]["stringUnit"]["state"] == "translated"
            })
            .count(),
        2_707
    );
    assert_eq!(
        strings["existing.0100"]["localizations"]["en"]["stringUnit"]["value"],
        "incoming friend edit"
    );
    assert_eq!(
        strings["Retry.formulation.4"]["localizations"]["en"]["stringUnit"]["value"],
        "current retry 4"
    );
}
