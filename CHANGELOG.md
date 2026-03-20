# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [0.3.0] - 2026-03-20

### Added
- `get_plurals` tool -- extract keys needing plural/device/substitution translation
- `get_context` tool -- find related keys by shared prefix
- Substitution roundtrip -- merge plural forms into substitution JSON structure
- CLDR plural rules for 40+ locales
- `PluralCategory::as_str()` for reliable enum conversion

### Fixed
- Validator no longer rejects substitution plural forms for specifier mismatch

## [0.2.0] - 2026-03-19

### Added
- `get_coverage` -- per-locale coverage statistics
- `get_stale` -- find stale/removed keys
- `validate_translations` -- file-wide validation report
- `list_locales` -- locale listing with stats
- `add_locale` -- add new locale with empty translations

## [0.1.0] - 2026-03-18

### Added
- Initial release
- `parse_xcstrings` -- parse and cache .xcstrings files
- `get_untranslated` -- extract untranslated strings with batching
- `submit_translations` -- validate and write translations atomically
- Xcode-compatible JSON formatting (" : " colon spacing)
- Format specifier validation
- Atomic file writes with crash safety
