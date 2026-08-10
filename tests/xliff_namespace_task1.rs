use indexmap::IndexMap;
use xcstrings_mcp::model::xcstrings::{
    Localization, StringEntry, StringUnit, TranslationState, XcStringsFile,
};
use xcstrings_mcp::service::xliff;

const XLIFF_NAMESPACE: &str = "urn:oasis:names:tc:xliff:document:1.2";

const PREFIXED_XLIFF: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ns0:xliff xmlns:ns0="urn:oasis:names:tc:xliff:document:1.2" version="1.2">
  <ns0:file source-language="en" target-language="d&#x65;" original="Localizable.xcstrings" datatype="plaintext">
    <ns0:body>
      <ns0:trans-unit id="greet&amp;leave">
        <ns0:source>Hello &amp; goodbye</ns0:source>
        <ns0:target state="translated">Hallo &amp; auf Wiedersehen</ns0:target>
      </ns0:trans-unit>
    </ns0:body>
  </ns0:file>
</ns0:xliff>"#;

fn export_catalog() -> XcStringsFile {
    XcStringsFile {
        source_language: "en".to_string(),
        strings: IndexMap::from([(
            "greeting".to_string(),
            StringEntry {
                extraction_state: None,
                should_translate: true,
                comment: None,
                localizations: Some(IndexMap::from([(
                    "en".to_string(),
                    Localization {
                        string_unit: Some(StringUnit {
                            state: TranslationState::Translated,
                            value: "Hello".to_string(),
                        }),
                        variations: None,
                        substitutions: None,
                    },
                )])),
            },
        )]),
        version: "1.0".to_string(),
    }
}

#[test]
fn export_declares_xliff_1_2_as_default_namespace() {
    let (xml, count) =
        xliff::export_xliff(&export_catalog(), "de", "Localizable.xcstrings", false).unwrap();

    assert_eq!(count, 1);
    assert!(xml.contains(&format!(
        r#"<xliff version="1.2" xmlns="{XLIFF_NAMESPACE}">"#
    )));
}

#[test]
fn import_accepts_prefix_bound_xliff_elements_and_decodes_attributes() {
    let (locale, translations) = xliff::import_xliff(PREFIXED_XLIFF).unwrap();

    assert_eq!(locale, "de");
    assert_eq!(translations.len(), 1);
    assert_eq!(translations[0].key, "greet&leave");
    assert_eq!(translations[0].locale, "de");
    assert_eq!(translations[0].value, "Hallo & auf Wiedersehen");
    assert!(translations[0].plural_forms.is_none());
    assert!(translations[0].substitution_name.is_none());
}

#[test]
fn import_rejects_structural_elements_bound_to_wrong_namespace() {
    let xml = PREFIXED_XLIFF.replace(XLIFF_NAMESPACE, "urn:example:wrong");

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: element <xliff> uses namespace 'urn:example:wrong'; expected 'urn:oasis:names:tc:xliff:document:1.2'"
    );
}

#[test]
fn import_rejects_wrong_namespace_on_nested_structural_element() {
    let xml = format!(
        r#"<xliff xmlns="{XLIFF_NAMESPACE}" xmlns:foreign="urn:example:wrong" version="1.2">
  <foreign:file target-language="de"><body></body></foreign:file>
</xliff>"#
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: element <file> uses namespace 'urn:example:wrong'; expected 'urn:oasis:names:tc:xliff:document:1.2'"
    );
}

#[test]
fn import_rejects_unbound_namespace_prefix_explicitly() {
    let xml = r#"<ns0:xliff version="1.2"><ns0:file target-language="de"/></ns0:xliff>"#;

    let error = xliff::import_xliff(xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: element <xliff> uses unbound namespace prefix 'ns0'"
    );
}

#[test]
fn import_preserves_unqualified_legacy_xliff_compatibility() {
    let xml = r#"<xliff version="1.2">
  <file target-language="de">
    <body><trans-unit id="greeting"><source>Hello</source><target>Hallo</target></trans-unit></body>
  </file>
</xliff>"#;

    let (locale, translations) = xliff::import_xliff(xml).unwrap();

    assert_eq!(locale, "de");
    assert_eq!(translations.len(), 1);
    assert_eq!(translations[0].key, "greeting");
    assert_eq!(translations[0].value, "Hallo");
}

#[test]
fn official_document_accepts_arbitrary_prefixes_and_same_uri_shadowing() {
    let xml = format!(
        r#"<root:xliff xmlns:root="{XLIFF_NAMESPACE}" xmlns:file="{XLIFF_NAMESPACE}" xmlns:unit="{XLIFF_NAMESPACE}" version="1.2">
  <file:file target-language="de"><root:body xmlns:root="{XLIFF_NAMESPACE}">
    <unit:trans-unit id="greeting"><root:source>Hello</root:source>
      <file:target>Hallo</file:target>
    </unit:trans-unit>
  </root:body></file:file>
</root:xliff>"#
    );

    let (locale, translations) = xliff::import_xliff(&xml).unwrap();

    assert_eq!(locale, "de");
    assert_eq!(translations.len(), 1);
    assert_eq!(translations[0].key, "greeting");
    assert_eq!(translations[0].value, "Hallo");
}

#[test]
fn official_document_rejects_unqualified_structural_child() {
    let xml = format!(
        r#"<x:xliff xmlns:x="{XLIFF_NAMESPACE}" version="1.2">
  <file target-language="de"><body></body></file>
</x:xliff>"#
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: element <file> is unqualified in namespace-qualified XLIFF document; expected 'urn:oasis:names:tc:xliff:document:1.2'"
    );
}

#[test]
fn official_document_rejects_default_namespace_reset() {
    let xml = format!(
        r#"<xliff xmlns="{XLIFF_NAMESPACE}" version="1.2">
  <file target-language="de"><body xmlns=""></body></file>
</xliff>"#
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: element <body> is unqualified in namespace-qualified XLIFF document; expected 'urn:oasis:names:tc:xliff:document:1.2'"
    );
}

#[test]
fn legacy_document_rejects_officially_qualified_structural_child() {
    let xml = format!(
        r#"<xliff xmlns:x="{XLIFF_NAMESPACE}" version="1.2">
  <x:file target-language="de"><x:body></x:body></x:file>
</xliff>"#
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: element <file> uses namespace 'urn:oasis:names:tc:xliff:document:1.2' in legacy unqualified XLIFF document; expected no namespace"
    );
}

#[test]
fn official_document_rejects_same_prefix_shadowed_to_wrong_uri() {
    let xml = format!(
        r#"<x:xliff xmlns:x="{XLIFF_NAMESPACE}" version="1.2">
  <x:file target-language="de"><x:body xmlns:x="urn:example:wrong"></x:body></x:file>
</x:xliff>"#
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: element <body> uses namespace 'urn:example:wrong'; expected 'urn:oasis:names:tc:xliff:document:1.2'"
    );
}

#[test]
fn import_rejects_duplicate_named_namespace_when_last_binding_is_official() {
    let xml = format!(
        r#"<x:xliff xmlns:x="urn:example:wrong" xmlns:x="{XLIFF_NAMESPACE}" version="1.2">
  <x:file target-language="de"></x:file>
</x:xliff>"#
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: duplicate attribute on <xliff>"
    );
}

#[test]
fn import_rejects_duplicate_default_namespace_when_last_binding_is_wrong() {
    let xml = format!(
        r#"<xliff xmlns="{XLIFF_NAMESPACE}" xmlns="urn:example:wrong" version="1.2">
  <file target-language="de"></file>
</xliff>"#
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: duplicate attribute on <xliff>"
    );
}

#[test]
fn import_rejects_duplicate_semantic_attribute_on_empty_file() {
    let xml = format!(
        r#"<xliff xmlns="{XLIFF_NAMESPACE}" version="1.2">
  <file target-language="de" target-language="fr"/>
</xliff>"#
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: duplicate attribute on <file>"
    );
}

#[test]
fn import_rejects_duplicate_ordinary_attribute_on_structural_start() {
    let xml = format!(
        r#"<xliff xmlns="{XLIFF_NAMESPACE}" version="1.2">
  <file target-language="de"><body custom="first" custom="last"></body></file>
</xliff>"#
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: duplicate attribute on <body>"
    );
}

#[test]
fn import_accepts_misc_outside_root_and_extension_nesting() {
    let xml = format!(
        r#"<?xml version="1.0"?>
<!-- before --><?before import?>
<x:xliff xmlns:x="{XLIFF_NAMESPACE}" xmlns:ext="urn:example:extension" version="1.2">
  <ext:group ext:role="container"><x:file target-language="de"><x:body>
    <x:trans-unit id="greeting"><x:source>Hello</x:source><x:target>Hallo</x:target></x:trans-unit>
  </x:body></x:file></ext:group>
</x:xliff>
<?after import?><!-- after -->"#
    );

    let (locale, translations) = xliff::import_xliff(&xml).unwrap();

    assert_eq!(locale, "de");
    assert_eq!(translations.len(), 1);
    assert_eq!(translations[0].key, "greeting");
    assert_eq!(translations[0].value, "Hallo");
}

#[test]
fn import_rejects_foreign_wrapper_around_xliff_root() {
    let xml = format!(
        r#"<wrapper xmlns:x="{XLIFF_NAMESPACE}"><x:xliff version="1.2">
  <x:file target-language="de"><x:body><x:trans-unit id="greeting">
    <x:source>Hello</x:source><x:target>Wrapped</x:target>
  </x:trans-unit></x:body></x:file>
</x:xliff></wrapper>"#
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: document root must be <xliff>; found <wrapper>"
    );
}

#[test]
fn import_rejects_nested_xliff_root() {
    let xml = format!(
        r#"<x:xliff xmlns:x="{XLIFF_NAMESPACE}" version="1.2"><x:xliff version="1.2">
  <x:file target-language="de"><x:body><x:trans-unit id="greeting">
    <x:source>Hello</x:source><x:target>Nested</x:target>
  </x:trans-unit></x:body></x:file>
</x:xliff></x:xliff>"#
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: nested <xliff> element is not allowed"
    );
}

#[test]
fn import_rejects_multiple_top_level_roots() {
    let xml = format!(
        r#"<x:xliff xmlns:x="{XLIFF_NAMESPACE}" version="1.2"/>
<x:xliff xmlns:x="{XLIFF_NAMESPACE}" version="1.2"><x:file target-language="de"/></x:xliff>"#
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: element <xliff> appears after </xliff> document root"
    );
}

#[test]
fn import_rejects_empty_root_followed_by_structural_fragment() {
    let xml = format!(
        r#"<x:xliff xmlns:x="{XLIFF_NAMESPACE}" version="1.2"/>
<x:file xmlns:x="{XLIFF_NAMESPACE}" target-language="de"/>"#
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: element <file> appears after </xliff> document root"
    );
}

#[test]
fn import_rejects_structural_fragment_as_document_root() {
    let error = xliff::import_xliff(r#"<file target-language="de"/>"#).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: document root must be <xliff>; found <file>"
    );
}

#[test]
fn import_rejects_non_whitespace_text_before_root() {
    let xml = format!(
        r#"unexpected<x:xliff xmlns:x="{XLIFF_NAMESPACE}" version="1.2"><x:file target-language="de"/></x:xliff>"#
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: non-whitespace text is not allowed outside <xliff> document root"
    );
}

#[test]
fn import_rejects_non_xml_ascii_whitespace_before_root() {
    let xml = format!(
        "\u{000c}<x:xliff xmlns:x=\"{XLIFF_NAMESPACE}\" version=\"1.2\"><x:file target-language=\"de\"/></x:xliff>"
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: non-whitespace text is not allowed outside <xliff> document root"
    );
}

#[test]
fn import_rejects_cdata_after_root() {
    let xml = format!(
        r#"<x:xliff xmlns:x="{XLIFF_NAMESPACE}" version="1.2"><x:file target-language="de"/></x:xliff><![CDATA[ ]]>"#
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: CDATA is not allowed outside <xliff> document root"
    );
}

#[test]
fn import_rejects_missing_xliff_root() {
    let error = xliff::import_xliff(" \n<!-- no document element -->\n").unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: missing <xliff> document root"
    );
}

#[test]
fn import_rejects_unclosed_xliff_root() {
    let xml = format!(
        r#"<x:xliff xmlns:x="{XLIFF_NAMESPACE}" version="1.2"><x:file target-language="de"/>"#
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: start tag not closed: `</x:xliff>` not found before end of input"
    );
}

#[test]
fn import_rejects_duplicate_expanded_attribute_names_with_alias_prefixes() {
    let xml = format!(
        r#"<x:xliff xmlns:x="{XLIFF_NAMESPACE}" xmlns:a="urn:attr" xmlns:b="urn:attr" version="1.2">
  <x:file target-language="de"><x:body a:custom="first" b:custom="last"/></x:file>
</x:xliff>"#
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: duplicate expanded attribute '{urn:attr}custom' on <body>"
    );
}

#[test]
fn import_rejects_duplicate_expanded_attribute_names_in_reverse_order() {
    let xml = format!(
        r#"<x:xliff xmlns:x="{XLIFF_NAMESPACE}" xmlns:a="urn:attr" xmlns:b="urn:attr" version="1.2">
  <x:file target-language="de"><x:body b:custom="first" a:custom="last"/></x:file>
</x:xliff>"#
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: duplicate expanded attribute '{urn:attr}custom' on <body>"
    );
}

#[test]
fn import_rejects_unbound_attribute_prefix() {
    let xml = format!(
        r#"<x:xliff xmlns:x="{XLIFF_NAMESPACE}" version="1.2">
  <x:file target-language="de"><x:body missing:custom="value"/></x:file>
</x:xliff>"#
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: attribute <missing:custom> on <body> uses unbound namespace prefix 'missing'"
    );
}

#[test]
fn import_accepts_same_attribute_local_name_in_different_namespaces() {
    let xml = format!(
        r#"<x:xliff xmlns:x="{XLIFF_NAMESPACE}" xmlns:a="urn:first" xmlns:b="urn:second" version="1.2">
  <x:file target-language="de"><x:body a:custom="first" b:custom="second"/></x:file>
</x:xliff>"#
    );

    let (locale, translations) = xliff::import_xliff(&xml).unwrap();

    assert_eq!(locale, "de");
    assert_eq!(translations.len(), 0);
}

#[test]
fn import_rejects_unbound_prefix_on_non_empty_extension_element() {
    let xml = format!(
        r#"<x:xliff xmlns:x="{XLIFF_NAMESPACE}" version="1.2">
  <missing:group><x:file target-language="de"><x:body><x:trans-unit id="greeting">
    <x:source>Hello</x:source><x:target>Unbound extension</x:target>
  </x:trans-unit></x:body></x:file></missing:group>
</x:xliff>"#
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: element <group> uses unbound namespace prefix 'missing'"
    );
}

#[test]
fn import_rejects_unbound_prefix_on_empty_extension_element() {
    let xml = format!(
        r#"<x:xliff xmlns:x="{XLIFF_NAMESPACE}" version="1.2">
  <missing:marker/><x:file target-language="de"/>
</x:xliff>"#
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: element <marker> uses unbound namespace prefix 'missing'"
    );
}

#[test]
fn import_accepts_numeric_reference_in_official_namespace_uri() {
    let xml = r#"<x:xliff xmlns:x="urn:oasis:names:tc:xliff:document:1.&#50;" version="1.2">
  <x:file target-language="de"><x:body><x:trans-unit id="greeting">
    <x:source>Hello</x:source><x:target>Normalized namespace</x:target>
  </x:trans-unit></x:body></x:file>
</x:xliff>"#;

    let (locale, translations) = xliff::import_xliff(xml).unwrap();

    assert_eq!(locale, "de");
    assert_eq!(translations.len(), 1);
    assert_eq!(translations[0].key, "greeting");
    assert_eq!(translations[0].value, "Normalized namespace");
}

#[test]
fn import_rejects_normalized_alias_collision_raw_then_encoded() {
    let xml = format!(
        r#"<x:xliff xmlns:x="{XLIFF_NAMESPACE}" xmlns:a="urn:attr" xmlns:b="urn:&#97;ttr" version="1.2">
  <x:file target-language="de"><x:body a:custom="first" b:custom="last"/></x:file>
</x:xliff>"#
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: duplicate expanded attribute '{urn:attr}custom' on <body>"
    );
}

#[test]
fn import_rejects_normalized_alias_collision_encoded_then_raw() {
    let xml = format!(
        r#"<x:xliff xmlns:x="{XLIFF_NAMESPACE}" xmlns:a="urn:&#97;ttr" xmlns:b="urn:attr" version="1.2">
  <x:file target-language="de"><x:body a:custom="first" b:custom="last"/></x:file>
</x:xliff>"#
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: duplicate expanded attribute '{urn:attr}custom' on <body>"
    );
}

#[test]
fn import_rejects_named_and_numeric_normalized_alias_collision() {
    let xml = format!(
        r#"<x:xliff xmlns:x="{XLIFF_NAMESPACE}" xmlns:a="urn:attr&amp;x" xmlns:b="urn:attr&#38;x" version="1.2">
  <x:file target-language="de"><x:body a:custom="first" b:custom="last"/></x:file>
</x:xliff>"#
    );

    let error = xliff::import_xliff(&xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: duplicate expanded attribute '{urn:attr&x}custom' on <body>"
    );
}

#[test]
fn import_reports_normalized_wrong_namespace_uri() {
    let xml = r#"<x:xliff xmlns:x="urn:example:wrong&amp;other" version="1.2"><x:file target-language="de"/></x:xliff>"#;

    let error = xliff::import_xliff(xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: element <xliff> uses namespace 'urn:example:wrong&other'; expected 'urn:oasis:names:tc:xliff:document:1.2'"
    );
}

#[test]
fn import_rejects_malformed_namespace_reference_deterministically() {
    let xml = r#"<x:xliff xmlns:x="urn:oasis:names:tc:xliff:document:1.&bogus;" version="1.2"><x:file target-language="de"/></x:xliff>"#;

    let error = xliff::import_xliff(xml).unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: invalid XML namespace value on <xliff>"
    );
}
