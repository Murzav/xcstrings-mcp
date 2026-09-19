use xcstrings_mcp::service::{parser, xliff};

const NS: &str = "urn:oasis:names:tc:xliff:document:1.2";

fn document(contents: &str) -> String {
    format!(r#"<xliff xmlns="{NS}" version="1.2">{contents}</xliff>"#)
}

fn assert_parse_error(xml: &str, expected: &str) {
    let error = xliff::import_xliff(xml).unwrap_err();
    assert_eq!(error.to_string(), format!("XLIFF parse error: {expected}"));
}

#[test]
fn import_accepts_real_xcode_empty_id_unit_with_empty_target() {
    let xml = include_str!("fixtures/xcode_26_6_empty_id.xliff");

    let file =
        parser::parse(r#"{"sourceLanguage":"en","strings":{"":{}},"version":"1.0"}"#).unwrap();
    let document = xliff::parse_document(xml).unwrap();
    let plan = xliff::plan_import(&file, &document, None).unwrap();

    assert_eq!(plan.report.locale, "ca");
    assert_eq!(plan.report.accepted, 1);
    assert!(plan.report.rejected.is_empty());
    let updated = plan.candidate.unwrap();
    assert_eq!(
        serde_json::to_value(&updated.strings[""].localizations.as_ref().unwrap()["ca"]).unwrap(),
        serde_json::json!({"stringUnit":{"state":"new","value":""}})
    );
}

#[test]
fn import_preserves_nonempty_target_for_empty_id() {
    let xml = document(
        r#"<file target-language="de"><body><trans-unit id=""><source></source><target>Leerzeichenlos</target></trans-unit></body></file>"#,
    );

    let (locale, translations) = xliff::import_xliff(&xml).unwrap();

    assert_eq!(locale, "de");
    assert_eq!(translations.len(), 1);
    assert_eq!(translations[0].key, "");
    assert_eq!(translations[0].locale, "de");
    assert_eq!(translations[0].value, "Leerzeichenlos");
}

#[test]
fn export_includes_variation_units_and_required_locale_categories() {
    let file = parser::parse(include_str!("fixtures/with_plurals.xcstrings")).unwrap();
    let (xml, count) = xliff::export_xliff(&file, "uk", "Localizable.xcstrings", false).unwrap();
    let doc = xliff::parse_document(&xml).unwrap();
    assert_eq!(count, 13);
    assert_eq!(
        doc.files[0]
            .units
            .iter()
            .map(|u| u.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "days_remaining|==|plural.few",
            "days_remaining|==|plural.many",
            "days_remaining|==|plural.one",
            "days_remaining|==|plural.other",
            "items_count|==|plural.few",
            "items_count|==|plural.many",
            "items_count|==|plural.one",
            "items_count|==|plural.other",
            "photos_count|==|plural.few",
            "photos_count|==|plural.many",
            "photos_count|==|plural.one",
            "photos_count|==|plural.other",
            "simple_key"
        ]
    );
}

#[test]
fn export_preserves_unlocalized_simple_entry() {
    let file = parser::parse(r#"{"sourceLanguage":"en","strings":{"new_key":{}},"version":"1.0"}"#)
        .unwrap();

    let (xml, count) = xliff::export_xliff(&file, "de", "Localizable.xcstrings", false).unwrap();

    assert_eq!(count, 1);
    assert_eq!(
        xliff::parse_document(&xml).unwrap().files[0].units[0].id,
        "new_key"
    );
    assert!(xml.contains("<source>new_key</source>"));
}

#[test]
fn export_preserves_entry_when_any_localization_has_simple_string_unit() {
    let file = parser::parse(
        r#"{
          "sourceLanguage":"en",
          "strings":{
            "mixed_key":{"localizations":{
              "en":{"variations":{"plural":{"other":{"stringUnit":{"state":"translated","value":"%lld values"}}}}},
              "de":{"stringUnit":{"state":"translated","value":"Einfach"}}
            }}
          },
          "version":"1.0"
        }"#,
    )
    .unwrap();

    let (xml, count) = xliff::export_xliff(&file, "de", "Localizable.xcstrings", false).unwrap();

    assert_eq!(count, 1);
    assert_eq!(
        xliff::parse_document(&xml).unwrap().files[0].units[0].id,
        "mixed_key"
    );
    assert!(xml.contains("<target state=\"translated\">Einfach</target>"));
}

#[test]
fn import_rejects_apple_variation_unit_id() {
    assert_parse_error(
        &document(
            r#"<file target-language="uk"><body><trans-unit id="days_remaining|==|plural.one"><source>%lld day remaining</source><target>%lld day left</target></trans-unit></body></file>"#,
        ),
        "Apple XLIFF variation unit id 'days_remaining|==|plural.one' is unsupported; import simple stringUnit ids only",
    );
}

#[test]
fn import_rejects_duplicate_trans_unit_id_in_one_file() {
    assert_parse_error(
        &document(
            r#"<file target-language="de"><body>
<trans-unit id="greeting"><source>Hello</source><target>Hallo</target></trans-unit>
<trans-unit id="greeting"><source>Hello again</source><target>Guten Tag</target></trans-unit>
</body></file>"#,
        ),
        "duplicate XLIFF unit id 'greeting' inside <file>",
    );
}

#[test]
fn import_rejects_ids_that_collide_after_xml_attribute_normalization() {
    assert_parse_error(
        &document(
            r#"<file target-language="de"><body>
<trans-unit id="Blood
Pressure"><source>Stacked</source><target>Gestapelt</target></trans-unit>
<trans-unit id="Blood Pressure"><source>Inline</source><target>Inline</target></trans-unit>
</body></file>"#,
        ),
        "duplicate XLIFF unit id 'Blood Pressure' inside <file>",
    );
}

#[test]
fn import_rejects_duplicate_id_shared_by_trans_and_bin_unit() {
    assert_parse_error(
        &document(
            r#"<file target-language="de"><body>
<trans-unit id="shared"><source>Hello</source><target>Hallo</target></trans-unit>
<bin-unit id="shared"><bin-source><external-file href="source.dat"/></bin-source></bin-unit>
</body></file>"#,
        ),
        "duplicate XLIFF unit id 'shared' inside <file>",
    );
}

#[test]
fn import_rejects_duplicate_translation_id_across_files_before_flattening() {
    assert_parse_error(
        &document(
            r#"<file target-language="de"><body><trans-unit id="greeting"><source>Hello</source><target>Hallo</target></trans-unit></body></file>
<file target-language="de"><body><trans-unit id="greeting"><source>Hello again</source><target>Guten Tag</target></trans-unit></body></file>"#,
        ),
        "XLIFF unit id 'greeting' is repeated across <file> elements and cannot be flattened safely",
    );
}

#[test]
fn import_accepts_distinct_unit_ids_across_files() {
    let xml = document(
        r#"<file target-language="de"><body><trans-unit id="first"><source>One</source><target>Eins</target></trans-unit></body></file>
<file target-language="de"><body><trans-unit id="second"><source>Two</source><target>Zwei</target></trans-unit></body></file>"#,
    );

    let (locale, translations) = xliff::import_xliff(&xml).unwrap();

    assert_eq!(locale, "de");
    assert_eq!(translations.len(), 2);
    assert_eq!(translations[0].key, "first");
    assert_eq!(translations[1].key, "second");
}
