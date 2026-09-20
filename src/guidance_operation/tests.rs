use super::*;
use crate::tools::test_helpers::MemoryStore;
use serde_json::json;

fn file() -> XcStringsFile {
    crate::service::parser::parse(&json!({"sourceLanguage":"en","version":"1.0","strings":{"profile":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"Account"}},"fr":{"stringUnit":{"state":"needs_review","value":"Profil"}},"de":{"stringUnit":{"state":"translated","value":"Konto"}}}}}}).to_string()).unwrap()
}
#[test]
fn absent_policy_is_explicit_without_unavailable_warning() {
    let snapshot = GuidanceSnapshot::load(&MemoryStore::new(), Path::new("/glossary.json"));
    let report = snapshot.check_catalog(&file(), &CatalogContexts::new(), None);
    assert_eq!(
        report,
        GuidanceReport {
            status: GuidanceStatus::Absent,
            revision: Some(byte_revision(None)),
            issues: vec![],
            unavailable: None
        }
    );
}
#[test]
fn corrupt_policy_keeps_exact_revision_and_reports_unavailable() {
    let store = MemoryStore::new();
    store.add_file("/glossary.json", "{bad");
    let snapshot = GuidanceSnapshot::load(&store, Path::new("/glossary.json"));
    let report = snapshot.check_catalog(&file(), &CatalogContexts::new(), None);
    assert_eq!(report.status, GuidanceStatus::Unavailable);
    assert_eq!(report.revision, Some(byte_revision(Some(b"{bad"))));
    assert_eq!(
        report.unavailable.unwrap().code,
        "glossary_parse_unavailable"
    );
    assert_eq!(report.issues, vec![]);
}
#[test]
fn staged_and_existing_leaf_checks_use_same_policy_and_preserve_catalog() {
    let store = MemoryStore::new();
    store.add_file("/glossary.json", r#"{"en→fr":{"Account":"Compte"}}"#);
    let snapshot = GuidanceSnapshot::load(&store, Path::new("/glossary.json"));
    let catalog = file();
    let before = serde_json::to_value(&catalog).unwrap();
    let destination = TranslationDestination {
        key: "profile".into(),
        locale: "fr".into(),
        path: vec![],
    };
    let staged = snapshot.check_destinations(
        &catalog,
        &CatalogContexts::new(),
        &[destination.clone(), destination],
    );
    let existing = snapshot.check_catalog(&catalog, &CatalogContexts::new(), Some("fr"));
    assert_eq!(staged, existing);
    assert_eq!(staged.issues.len(), 1);
    assert_eq!(
        staged.issues[0].code,
        crate::model::glossary::TerminologyCode::PreferredMissing
    );
    assert_eq!(staged.issues[0].locale, "fr");
    assert_eq!(serde_json::to_value(&catalog).unwrap(), before);
}
#[test]
fn loaded_snapshot_does_not_mix_rules_after_policy_changes() {
    let store = MemoryStore::new();
    let old = r#"{"en→fr":{"Account":"Compte"}}"#;
    store.add_file("/glossary.json", old);
    let snapshot = GuidanceSnapshot::load(&store, Path::new("/glossary.json"));
    store.update_file("/glossary.json", r#"{"en→fr":{"Account":"Profil"}}"#);
    let report = snapshot.check_catalog(&file(), &CatalogContexts::new(), Some("fr"));
    assert_eq!(report.issues.len(), 1);
    assert_eq!(report.revision, Some(byte_revision(Some(old.as_bytes()))));
    assert_eq!(
        GuidanceSnapshot::load(&store, Path::new("/glossary.json"))
            .check_catalog(&file(), &CatalogContexts::new(), Some("fr"))
            .issues,
        vec![]
    );
}
