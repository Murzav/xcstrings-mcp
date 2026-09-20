use xcstrings_mcp::model::translation::CompletedTranslation;
use xcstrings_mcp::model::xcstrings::{TranslationState, paths::find_leaf};
use xcstrings_mcp::service::{merger, parser};

#[test]
fn native_submission_saves_reviewable_draft_instead_of_ready_translation() {
    let mut file = parser::parse(r#"{"sourceLanguage":"en","version":"1.0","strings":{"delete":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"Delete permanently"}}}}}}"#).unwrap();
    let result = merger::merge_translations(
        &mut file,
        &[CompletedTranslation {
            key: "delete".into(),
            locale: "de".into(),
            value: "Dauerhaft löschen".into(),
            path: Some(vec![]),
            ..Default::default()
        }],
    );
    assert_eq!(result.accepted, 1);
    assert_eq!(result.rejected.len(), 0);
    let target = find_leaf(
        &file.strings["delete"].localizations.as_ref().unwrap()["de"],
        &[],
    )
    .unwrap();
    assert_eq!(target.value, "Dauerhaft löschen");
    assert_eq!(target.state, TranslationState::NeedsReview);
}
