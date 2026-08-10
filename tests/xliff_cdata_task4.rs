use xcstrings_mcp::service::xliff;

const NS: &str = "urn:oasis:names:tc:xliff:document:1.2";

fn document(contents: &str) -> String {
    format!(r#"<xliff xmlns="{NS}" version="1.2">{contents}</xliff>"#)
}

#[test]
fn import_preserves_mixed_text_cdata_and_entity_content() {
    let xml = document(
        r#"<file target-language="de"><body><trans-unit id="greeting"><source>Hello</source><target>Start <![CDATA[<b>&raw]]> + &amp; end</target></trans-unit></body></file>"#,
    );

    let (locale, translations) = xliff::import_xliff(&xml).unwrap();

    assert_eq!(locale, "de");
    assert_eq!(translations.len(), 1);
    assert_eq!(translations[0].key, "greeting");
    assert_eq!(translations[0].value, "Start <b>&raw + & end");
}

#[test]
fn import_rejects_cdata_before_document_root() {
    let xml = format!(
        r#"<![CDATA[outside]]><xliff xmlns="{NS}" version="1.2"><file target-language="de"><body/></file></xliff>"#
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: CDATA is not allowed outside <xliff> document root"
    );
}

#[test]
fn import_rejects_unclosed_cdata() {
    let xml = document(
        r#"<file target-language="de"><body><trans-unit id="greeting"><source>Hello</source><target><![CDATA[unterminated</target></trans-unit></body></file>"#,
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: syntax error: CDATA not closed: `]]>` not found before end of input"
    );
}
