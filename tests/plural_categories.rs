use xcstrings_mcp::error::XcStringsError;
use xcstrings_mcp::model::plural::{PluralCategory, plural_categories};

#[test]
fn french_categories_include_the_many_form() {
    assert_eq!(
        plural_categories("fr").unwrap(),
        vec![
            PluralCategory::One,
            PluralCategory::Many,
            PluralCategory::Other
        ]
    );
}

#[test]
fn hebrew_categories_include_the_two_form() {
    assert_eq!(
        plural_categories("he").unwrap(),
        vec![
            PluralCategory::One,
            PluralCategory::Two,
            PluralCategory::Other
        ]
    );
}

#[test]
fn japanese_has_only_the_other_category() {
    assert_eq!(
        plural_categories("ja").unwrap(),
        vec![PluralCategory::Other]
    );
}

#[test]
fn regional_script_and_underscore_locales_use_known_parent_rules() {
    assert_eq!(
        plural_categories("UK_ua").unwrap(),
        vec![
            PluralCategory::One,
            PluralCategory::Few,
            PluralCategory::Many,
            PluralCategory::Other
        ]
    );
    assert_eq!(
        plural_categories("zh-Hant-TW").unwrap(),
        vec![PluralCategory::Other]
    );
}

#[test]
fn unknown_locale_does_not_invent_two_form_rules() {
    let error = plural_categories("xx-ZZ").unwrap_err();
    assert!(
        matches!(error, XcStringsError::InvalidFormat(ref message) if message == "no CLDR cardinal plural rules for locale 'xx-ZZ'")
    );
}

#[test]
fn empty_locale_reports_missing_rules() {
    let error = plural_categories("").unwrap_err();
    assert!(
        matches!(error, XcStringsError::InvalidFormat(ref message) if message == "no CLDR cardinal plural rules for locale ''")
    );
}

#[test]
fn malformed_region_does_not_fall_back_to_a_known_language() {
    let error = plural_categories("en-?").unwrap_err();
    assert!(
        matches!(error, XcStringsError::InvalidFormat(ref message) if message == "invalid locale identifier 'en-?'")
    );
}

#[test]
fn empty_locale_subtag_does_not_fall_back_to_a_known_language() {
    let error = plural_categories("en--US").unwrap_err();
    assert!(
        matches!(error, XcStringsError::InvalidFormat(ref message) if message == "invalid locale identifier 'en--US'")
    );
}

#[test]
fn malformed_extension_and_overlong_subtag_do_not_invent_known_rules() {
    for locale in ["en-1", "en-123456789", "en-u", "en-US-DE", "en-Latn-Latn"] {
        let error = plural_categories(locale).unwrap_err();
        assert!(matches!(error, XcStringsError::InvalidFormat(ref message)
            if message == &format!("invalid locale identifier '{locale}'")));
    }
}

#[test]
fn valid_extension_and_private_use_keep_language_rules() {
    for locale in ["en-u-nu-latn", "en-x-a", "de-DE-u-co-phonebk"] {
        assert_eq!(
            plural_categories(locale).unwrap(),
            vec![PluralCategory::One, PluralCategory::Other]
        );
    }
}
