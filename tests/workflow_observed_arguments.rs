use xcstrings_mcp::model::specifier::{FormatArgumentRole as Role, observed_format_arguments};

#[test]
fn dynamic_components_consume_positions_before_the_value() {
    let actual = observed_format_arguments("Value %*.*f then %@").unwrap();
    let descriptors = actual
        .iter()
        .map(|a| (a.position, a.role, a.raw.as_str(), a.format.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        descriptors,
        [
            (1, Role::Width, "%*.*f", "%d"),
            (2, Role::Precision, "%*.*f", "%d"),
            (3, Role::Value, "%*.*f", "%f"),
            (4, Role::Value, "%@", "%@"),
        ]
    );
}

#[test]
fn explicit_positions_and_leaf_gaps_are_preserved() {
    let actual = observed_format_arguments("%4$*2$.*1$lld").unwrap();
    assert_eq!(
        actual
            .iter()
            .map(|a| (a.position, a.role, a.format.as_str()))
            .collect::<Vec<_>>(),
        [
            (2, Role::Width, "%d"),
            (1, Role::Precision, "%d"),
            (4, Role::Value, "%lld"),
        ]
    );
}

#[test]
fn mixed_positions_are_reported_instead_of_guessed() {
    let errors = observed_format_arguments("%2$@ %@").unwrap_err();
    assert_eq!(
        errors.iter().map(|e| e.code).collect::<Vec<_>>(),
        ["mixed_positional_arguments"]
    );
}

#[test]
fn literal_percent_and_ambiguous_prose_are_not_arguments() {
    assert_eq!(
        observed_format_arguments("100%% done; 100% Local Storage").unwrap(),
        []
    );
}
