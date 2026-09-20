use xcstrings_mcp::service::{parser, xliff};
#[test]
fn xliff_source_text_must_match_current_catalog_even_when_format_arguments_match() {
    let file=parser::parse(r#"{"sourceLanguage":"en","version":"1.0","strings":{"k":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"Delete permanently"}}}}}}"#).unwrap();
    let document=xliff::parse_document(r#"<xliff version="1.2"><file source-language="en" target-language="de"><body><trans-unit id="k"><source>Delete</source><target state="needs-review-l10n">Löschen</target></trans-unit></body></file></xliff>"#).unwrap();
    let plan = xliff::plan_import(&file, &document, None).unwrap();
    assert!(plan.candidate.is_none());
    assert_eq!(plan.report.accepted, 0);
    assert!(plan.report.accepted_destinations.is_empty());
    assert_eq!(plan.report.rejected.len(), 1);
    assert_eq!(plan.report.rejected[0].code, "source_text_mismatch");
}

#[test]
fn apple_positioned_source_macros_match_unpositioned_catalog_references() {
    let file=parser::parse(r#"{"sourceLanguage":"en","version":"1.0","strings":{"k":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"%#@COUNT@ selected"},"substitutions":{"COUNT":{"argNum":1,"formatSpecifier":"lld","variations":{"plural":{"other":{"stringUnit":{"state":"translated","value":"%arg items"}}}}}}}}}}}"#).unwrap();
    let document=xliff::parse_document(r#"<xliff version="1.2"><file source-language="en" target-language="de"><body><trans-unit id="k"><source>%1$#@COUNT@ selected</source><target state="needs-review-l10n">%1$#@COUNT@ gewählt</target></trans-unit></body></file></xliff>"#).unwrap();
    let plan = xliff::plan_import(&file, &document, None).unwrap();
    assert_eq!(plan.report.rejected, vec![]);
    assert_eq!(plan.report.accepted, 1);
    assert_eq!(
        serde_json::to_value(plan.candidate.unwrap()).unwrap()["strings"]["k"]["localizations"]["de"]
            ["stringUnit"]["value"],
        "%#@COUNT@ gewählt"
    );
}
