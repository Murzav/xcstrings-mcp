use serde_json::json;
use xcstrings_mcp::model::xcstrings::paths::{
    LeafDiagnosticCode, LeafStep, collect_leaves, find_leaf, find_leaf_mut,
};
use xcstrings_mcp::model::xcstrings::{DeviceCategory, Localization, TranslationState};

#[test]
fn typed_paths_resolve_recursive_devices_plurals_and_substitution_context() {
    let mut localization: Localization = serde_json::from_value(json!({
        "stringUnit":{"state":"translated","value":"Root"},
        "variations":{"device":{"iphone":{"variations":{"plural":{"other":{"stringUnit":{"state":"machine_translated","value":"%lld things"}}}}},"other":{"stringUnit":{"state":"translated","value":"Fallback"}}}},
        "substitutions":{"COUNT":{"argNum":2,"formatSpecifier":"lld","variations":{"plural":{"one":{"stringUnit":{"state":"translated","value":"%arg item"}}}}}}
    })).unwrap();

    let traversal = collect_leaves(&localization);
    assert_eq!(traversal.diagnostics, vec![]);
    assert_eq!(
        traversal
            .leaves
            .iter()
            .map(|leaf| (&leaf.path, leaf.unit.value.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (&vec![], "Root"),
            (
                &vec![
                    LeafStep::Device(DeviceCategory::IPhone),
                    LeafStep::Plural("other".into())
                ],
                "%lld things"
            ),
            (&vec![LeafStep::Device(DeviceCategory::Other)], "Fallback"),
            (
                &vec![
                    LeafStep::Substitution("COUNT".into()),
                    LeafStep::Plural("one".into())
                ],
                "%arg item"
            ),
        ]
    );
    assert_eq!(
        traversal.leaves[1].unit.state,
        TranslationState::MachineTranslated
    );
    assert_eq!(traversal.leaves[3].substitutions.len(), 1);
    assert_eq!(traversal.leaves[3].substitutions[0].name, "COUNT");
    assert_eq!(
        traversal.leaves[3].substitutions[0].substitution.arg_num,
        Some(2)
    );
    assert_eq!(
        traversal.leaves[3].substitutions[0]
            .substitution
            .format_specifier
            .as_deref(),
        Some("lld")
    );
    let path = vec![
        LeafStep::Device(DeviceCategory::IPhone),
        LeafStep::Plural("other".into()),
    ];
    assert_eq!(
        serde_json::to_value(&path).unwrap(),
        json!([{"device":"iphone"},{"plural":"other"}])
    );
    assert_eq!(
        find_leaf(&localization, &path).unwrap().value,
        "%lld things"
    );
    find_leaf_mut(&mut localization, &path).unwrap().value = "%lld updated".into();
    assert_eq!(
        find_leaf(&localization, &path).unwrap().value,
        "%lld updated"
    );
    assert_eq!(find_leaf(&localization, &[]).unwrap().value, "Root");
    assert!(find_leaf(&localization, &[LeafStep::Plural("missing".into())]).is_none());
}

#[test]
fn traversal_diagnoses_unknown_axis_without_discarding_known_leaves() {
    let node: Localization = serde_json::from_value(json!({"variations":{"future":{"other":{}},"plural":{"other":{"stringUnit":{"state":"new","value":"draft"}}}}})).unwrap();

    let traversal = collect_leaves(&node);

    assert_eq!(traversal.leaves.len(), 1);
    assert_eq!(traversal.leaves[0].unit.value, "draft");
    assert_eq!(traversal.diagnostics.len(), 1);
    assert_eq!(traversal.diagnostics[0].path, vec![]);
    assert_eq!(
        traversal.diagnostics[0].code,
        LeafDiagnosticCode::UnknownAxis
    );
    assert_eq!(
        traversal.diagnostics[0].detail,
        "unsupported variation axis 'future'"
    );
}

#[test]
fn traversal_diagnoses_empty_localization_and_empty_axis() {
    let node: Localization =
        serde_json::from_value(json!({"variations":{"device":{"iphone":{}},"plural":{}}})).unwrap();

    let traversal = collect_leaves(&node);

    assert_eq!(traversal.leaves.len(), 0);
    assert_eq!(
        traversal
            .diagnostics
            .iter()
            .map(|d| (&d.path, &d.code))
            .collect::<Vec<_>>(),
        vec![
            (&vec![], &LeafDiagnosticCode::EmptyAxis),
            (
                &vec![LeafStep::Device(DeviceCategory::IPhone)],
                &LeafDiagnosticCode::EmptyLocalization
            ),
        ]
    );
}

#[test]
fn all_seven_apple_devices_have_known_typed_variants() {
    let variants: Vec<DeviceCategory> = serde_json::from_value(json!([
        "appletv",
        "applevision",
        "applewatch",
        "ipad",
        "iphone",
        "mac",
        "other"
    ]))
    .unwrap();
    assert_eq!(
        variants,
        vec![
            DeviceCategory::AppleTv,
            DeviceCategory::AppleVision,
            DeviceCategory::AppleWatch,
            DeviceCategory::IPad,
            DeviceCategory::IPhone,
            DeviceCategory::Mac,
            DeviceCategory::Other
        ]
    );
}
