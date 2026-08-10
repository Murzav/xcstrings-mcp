use super::*;
use crate::tools::test_helpers::{MemoryStore, SIMPLE_FIXTURE};
use std::path::Path;

const NS: &str = "urn:oasis:names:tc:xliff:document:1.2";

async fn assert_rejected_without_write(contents: &str, expected: &str) {
    let store = MemoryStore::new();
    store.add_file("/test/catalog.xcstrings", SIMPLE_FIXTURE);
    store.add_file(
        "/test/input.xliff",
        &format!(r#"<xliff xmlns="{NS}" version="1.2">{contents}</xliff>"#),
    );
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());
    let before = store
        .get_content(Path::new("/test/catalog.xcstrings"))
        .unwrap();

    let error = handle_import_xliff(
        &store,
        &cache,
        &write_lock,
        ImportXliffParams {
            file_path: Some("/test/catalog.xcstrings".to_string()),
            xliff_path: "/test/input.xliff".to_string(),
            dry_run: false,
        },
    )
    .await
    .unwrap_err();

    assert_eq!(error.to_string(), format!("XLIFF parse error: {expected}"));
    assert_eq!(
        store
            .get_content(Path::new("/test/catalog.xcstrings"))
            .unwrap(),
        before
    );
}

macro_rules! malformed_mcp_case {
    ($name:ident, $contents:expr, $error:literal) => {
        #[tokio::test]
        async fn $name() {
            assert_rejected_without_write($contents, $error).await;
        }
    };
}

malformed_mcp_case!(
    mcp_rejects_trans_unit_before_file_without_write,
    r#"<trans-unit id="greeting"><source>Hello</source><target>Injected</target></trans-unit>
<file target-language="de"><body/></file>"#,
    "element <trans-unit> is not allowed as a child of <xliff>; expected <file>"
);

malformed_mcp_case!(
    mcp_rejects_file_without_body_without_write,
    r#"<file target-language="de"><trans-unit id="greeting"><source>Hello</source><target>Injected</target></trans-unit></file>"#,
    "element <trans-unit> is not allowed as a child of <file>"
);

malformed_mcp_case!(
    mcp_rejects_target_before_source_without_write,
    r#"<file target-language="de"><body><trans-unit id="greeting"><target>Injected</target><source>Hello</source></trans-unit></body></file>"#,
    "element <target> must follow <source> inside <trans-unit>"
);

malformed_mcp_case!(
    mcp_rejects_duplicate_target_without_write,
    r#"<file target-language="de"><body><trans-unit id="greeting"><source>Hello</source><target>First</target><target>Second</target></trans-unit></body></file>"#,
    "element <trans-unit> may contain at most one <target> child"
);

malformed_mcp_case!(
    mcp_rejects_nested_file_without_write,
    r#"<file target-language="de"><file target-language="fr"><body/></file><body/></file>"#,
    "element <file> is not allowed as a child of <file>"
);

malformed_mcp_case!(
    mcp_rejects_bin_target_nested_in_header_without_write,
    r#"<file target-language="de"><header><bin-target><external-file href="binary.dat"/></bin-target></header>
<body><trans-unit id="greeting"><source>Hello</source><target>Injected</target></trans-unit></body></file>"#,
    "element <bin-target> is not allowed as a child of <header>"
);

malformed_mcp_case!(
    mcp_rejects_bin_unit_without_bin_source_without_write,
    r#"<file target-language="de"><body><bin-unit/></body></file>"#,
    "element <bin-unit> is missing required <bin-source> child"
);

malformed_mcp_case!(
    mcp_rejects_group_metadata_out_of_order_without_write,
    r#"<file target-language="de"><body><group><note>late context</note>
<context-group><context>screen</context></context-group>
<trans-unit id="greeting"><source>Hello</source><target>Injected</target></trans-unit>
</group></body></file>"#,
    "element <context-group> is out of order inside <group>"
);
