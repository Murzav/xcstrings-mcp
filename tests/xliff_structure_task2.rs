use xcstrings_mcp::service::xliff;

const NS: &str = "urn:oasis:names:tc:xliff:document:1.2";

fn document(contents: &str) -> String {
    format!(r#"<xliff xmlns="{NS}" version="1.2">{contents}</xliff>"#)
}

fn assert_parse_error(xml: &str, expected: &str) {
    let error = xliff::import_xliff(xml).unwrap_err();
    assert_eq!(error.to_string(), format!("XLIFF parse error: {expected}"));
}

#[test]
fn import_rejects_trans_unit_before_file() {
    assert_parse_error(
        &document(
            r#"<trans-unit id="greeting"><source>Hello</source><target>Hallo</target></trans-unit>
<file target-language="de"><body/></file>"#,
        ),
        "element <trans-unit> is not allowed as a child of <xliff>; expected <file>",
    );
}

#[test]
fn import_rejects_file_without_body() {
    assert_parse_error(
        &document(r#"<file target-language="de"></file>"#),
        "element <file> is missing required <body> child",
    );
}

#[test]
fn import_rejects_target_before_source() {
    assert_parse_error(
        &document(
            r#"<file target-language="de"><body><trans-unit id="greeting">
<target>Hallo</target><source>Hello</source></trans-unit></body></file>"#,
        ),
        "element <target> must follow <source> inside <trans-unit>",
    );
}

#[test]
fn import_rejects_duplicate_target() {
    assert_parse_error(
        &document(
            r#"<file target-language="de"><body><trans-unit id="greeting">
<source>Hello</source><target>Hallo</target><target>Servus</target>
</trans-unit></body></file>"#,
        ),
        "element <trans-unit> may contain at most one <target> child",
    );
}

#[test]
fn import_rejects_nested_file() {
    assert_parse_error(
        &document(
            r#"<file target-language="de"><file target-language="fr"><body/></file><body/></file>"#,
        ),
        "element <file> is not allowed as a child of <file>",
    );
}

#[test]
fn import_accepts_header_recursive_groups_inline_content_and_allowed_extensions() {
    let xml = format!(
        r#"<xliff xmlns="{NS}" xmlns:ext="urn:example:extension" version="1.2">
<ext:root-metadata/>
<file target-language="de"><header><ext:header-metadata/></header><body>
  <group><ext:group-metadata/><group><trans-unit id="greeting">
    <source>Hello <g id="1">friend</g></source>
    <target>Hallo <g id="1">Freund</g></target><ext:unit-metadata/>
  </trans-unit></group></group>
</body></file></xliff>"#
    );

    let (locale, translations) = xliff::import_xliff(&xml).unwrap();

    assert_eq!(locale, "de");
    assert_eq!(translations.len(), 1);
    assert_eq!(translations[0].key, "greeting");
    assert_eq!(translations[0].locale, "de");
    assert_eq!(translations[0].value, "Hallo Freund");
}

#[test]
fn import_accepts_multiple_files_when_target_locale_matches() {
    let xml = document(
        r#"<file target-language="de"><body><trans-unit id="first"><source>One</source><target>Eins</target></trans-unit></body></file>
<file target-language="de"><body><trans-unit id="second"><source>Two</source><target>Zwei</target></trans-unit></body></file>"#,
    );

    let (locale, translations) = xliff::import_xliff(&xml).unwrap();

    assert_eq!(locale, "de");
    assert_eq!(translations.len(), 2);
    assert_eq!(translations[0].key, "first");
    assert_eq!(translations[0].value, "Eins");
    assert_eq!(translations[1].key, "second");
    assert_eq!(translations[1].value, "Zwei");
}

#[test]
fn import_rejects_multiple_files_with_different_target_locales() {
    assert_parse_error(
        &document(
            r#"<file target-language="de"><body/></file><file target-language="fr"><body/></file>"#,
        ),
        "multiple <file> elements use different target-language values: 'de' and 'fr'",
    );
}

#[test]
fn import_rejects_blank_target_locale() {
    assert_parse_error(
        &document(r#"<file target-language="  "><body/></file>"#),
        "attribute target-language on <file> must not be empty",
    );
}

#[test]
fn import_rejects_header_after_body() {
    assert_parse_error(
        &document(r#"<file target-language="de"><body/><header/></file>"#),
        "element <header> must appear before <body> inside <file>",
    );
}

#[test]
fn import_rejects_duplicate_body() {
    assert_parse_error(
        &document(r#"<file target-language="de"><body/><body/></file>"#),
        "element <file> may contain exactly one <body> child",
    );
}

#[test]
fn import_rejects_trans_unit_without_source() {
    assert_parse_error(
        &document(
            r#"<file target-language="de"><body><trans-unit id="greeting"></trans-unit></body></file>"#,
        ),
        "element <trans-unit> is missing required <source> child",
    );
}

#[test]
fn extension_wrapper_cannot_make_nested_core_target_valid() {
    let xml = format!(
        r#"<xliff xmlns="{NS}" xmlns:ext="urn:example:extension" version="1.2">
<file target-language="de"><body><trans-unit id="greeting"><source>Hello</source>
  <ext:wrapper><target>Injected</target></ext:wrapper>
</trans-unit></body></file></xliff>"#
    );
    assert_parse_error(
        &xml,
        "element <target> is not allowed as a child of extension element <wrapper>",
    );
}

#[test]
fn import_rejects_bin_target_nested_in_header() {
    assert_parse_error(
        &document(
            r#"<file target-language="de"><header><bin-target><external-file href="binary.dat"/></bin-target></header>
<body><trans-unit id="greeting"><source>Hello</source><target>Injected</target></trans-unit></body></file>"#,
        ),
        "element <bin-target> is not allowed as a child of <header>",
    );
}

#[test]
fn import_rejects_bin_unit_without_bin_source() {
    assert_parse_error(
        &document(r#"<file target-language="de"><body><bin-unit/></body></file>"#),
        "element <bin-unit> is missing required <bin-source> child",
    );
}

#[test]
fn import_accepts_schema_positioned_header_and_binary_unit_content() {
    let xml = format!(
        r#"<xliff xmlns="{NS}" xmlns:ext="urn:example:extension" version="1.2">
<file target-language="de"><header>
  <skl><external-file href="skeleton.dat"/></skl>
  <phase-group><phase><note>Reviewed</note></phase></phase-group>
  <count-group><count>3</count></count-group>
  <tool><ext:tool-data/></tool>
</header><body><bin-unit id="binary" mime-type="application/octet-stream">
  <bin-source><external-file href="source.dat"/></bin-source>
  <bin-target><internal-file>AAEC</internal-file></bin-target>
  <context-group><context>binary</context></context-group>
  <trans-unit id="greeting"><source>Hello</source><target>Hallo</target></trans-unit>
</bin-unit></body></file></xliff>"#,
    );

    let (locale, translations) = xliff::import_xliff(&xml).unwrap();

    assert_eq!(locale, "de");
    assert_eq!(translations.len(), 1);
    assert_eq!(translations[0].key, "greeting");
    assert_eq!(translations[0].value, "Hallo");
}

macro_rules! extension_tail_case {
    ($name:ident, $contents:expr, $parent:literal) => {
        #[test]
        fn $name() {
            assert_parse_error(
                &document($contents),
                concat!(
                    "element <note> must not follow extension content inside <",
                    $parent,
                    ">"
                ),
            );
        }
    };
}

extension_tail_case!(
    import_rejects_core_metadata_after_header_extension,
    r#"<file xmlns:ext="urn:example:extension" target-language="de"><header><ext:data/><note>late</note></header><body/></file>"#,
    "header"
);

extension_tail_case!(
    import_rejects_core_metadata_after_group_extension,
    r#"<file xmlns:ext="urn:example:extension" target-language="de"><body><group><ext:data/><note>late</note></group></body></file>"#,
    "group"
);

extension_tail_case!(
    import_rejects_core_metadata_after_unit_extension,
    r#"<file xmlns:ext="urn:example:extension" target-language="de"><body><trans-unit id="greeting"><source>Hello</source><ext:data/><note>late</note></trans-unit></body></file>"#,
    "trans-unit"
);

extension_tail_case!(
    import_rejects_core_metadata_after_alt_trans_extension,
    r#"<file xmlns:ext="urn:example:extension" target-language="de"><body><trans-unit id="greeting"><source>Hello</source><alt-trans><target>Hallo</target><ext:data/><note>late</note></alt-trans></trans-unit></body></file>"#,
    "alt-trans"
);

extension_tail_case!(
    import_rejects_core_metadata_after_bin_unit_extension,
    r#"<file xmlns:ext="urn:example:extension" target-language="de"><body><bin-unit><bin-source><external-file href="source.dat"/></bin-source><ext:data/><note>late</note></bin-unit></body></file>"#,
    "bin-unit"
);

#[test]
fn import_rejects_context_group_after_note_inside_group() {
    assert_parse_error(
        &document(
            r#"<file target-language="de"><body><group><note>late context</note>
<context-group><context>screen</context></context-group>
<trans-unit id="greeting"><source>Hello</source><target>Injected</target></trans-unit>
</group></body></file>"#,
        ),
        "element <context-group> is out of order inside <group>",
    );
}

#[test]
fn import_rejects_phase_group_after_header_metadata() {
    assert_parse_error(
        &document(
            r#"<file target-language="de"><header><note>reviewed</note><phase-group><phase/></phase-group></header><body/></file>"#,
        ),
        "element <phase-group> is out of order inside <header>",
    );
}

#[test]
fn import_rejects_context_group_after_note_inside_alt_trans() {
    assert_parse_error(
        &document(
            r#"<file target-language="de"><body><trans-unit id="greeting"><source>Hello</source>
<alt-trans><target>Hallo</target><note>late context</note><context-group><context>screen</context></context-group></alt-trans>
</trans-unit></body></file>"#,
        ),
        "element <context-group> is out of order inside <alt-trans>",
    );
}

#[test]
fn import_rejects_group_without_structural_content() {
    assert_parse_error(
        &document(
            r#"<file target-language="de"><body><group><note>metadata only</note></group></body></file>"#,
        ),
        "element <group> must contain at least one <group>, <trans-unit>, or <bin-unit> child",
    );
}

#[test]
fn import_rejects_alt_trans_without_target() {
    assert_parse_error(
        &document(
            r#"<file target-language="de"><body><trans-unit id="greeting"><source>Hello</source><alt-trans><source>Hi</source></alt-trans></trans-unit></body></file>"#,
        ),
        "element <alt-trans> must contain exactly one <target> child",
    );
}

#[test]
fn import_accepts_metadata_at_every_supported_order_boundary() {
    let xml = format!(
        r#"<xliff xmlns="{NS}" xmlns:ext="urn:example:extension" version="1.2">
<file target-language="de"><header>
  <skl><external-file href="skeleton.dat"/></skl>
  <phase-group><phase><note>phase note</note></phase></phase-group>
  <glossary><internal-file>terms</internal-file></glossary>
  <count-group><count>2</count></count-group><note>header note</note><tool/>
  <ext:header-tail/>
</header><body><group>
  <context-group><context>first</context></context-group>
  <count-group><count>2</count></count-group>
  <prop-group><prop>legacy</prop></prop-group>
  <note>group note</note><ext:group-tail/>
  <trans-unit id="greeting"><source>Hello</source><target>Hallo</target>
    <alt-trans><source>Hi</source><seg-source>Hi</seg-source><target>Servus</target>
      <context-group><context>alternative</context></context-group>
      <prop-group><prop>legacy</prop></prop-group><note>alt note</note><ext:alt-tail/>
    </alt-trans>
  </trans-unit>
</group></body></file></xliff>"#,
    );

    let (locale, translations) = xliff::import_xliff(&xml).unwrap();

    assert_eq!(locale, "de");
    assert_eq!(translations.len(), 1);
    assert_eq!(translations[0].key, "greeting");
    assert_eq!(translations[0].value, "Hallo");
}
