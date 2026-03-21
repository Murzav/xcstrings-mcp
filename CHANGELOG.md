# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [1.1.0] - 2026-03-21

### Added
- **`import_strings` tool** -- migrate legacy `.strings` and `.stringsdict` files to `.xcstrings` format
- `.strings` parser with full escape sequence support (`\Unnnn` surrogate pairs, `\"`, `\\`, `\n`, `\t`, `\r`)
- `.stringsdict` XML plist parser with CLDR plural rule support (single and multi-variable)
- UTF-16LE/BE encoding auto-detection via BOM with UTF-16LE heuristic fallback
- Recursive `.lproj` directory scanning (`directory` param)
- Unquoted key support for legacy old-style ASCII plist files
- **`discover_files` now detects legacy files** -- returns `.strings`/`.stringsdict` in `legacy_files` alongside `.xcstrings`
- `read_bytes` method on `FileStore` trait for raw byte access
- Merge mode: import into existing `.xcstrings` without overwriting
- `dry_run` mode for previewing migration results
- Positional format specifier handling (`%1$lld` → `%arg`) in stringsdict substitutions
- CDATA content support in `.stringsdict` XML plist parsing
- `Base.lproj` filtering in directory discovery (not a real locale)
- Symlink depth protection (max 20 levels) in `.lproj` directory scanner
- Warnings for skipped `.stringsdict` entries with unsupported rule types (device/width variants)

## [1.0.0] - 2026-03-21

### Added
- **`create_xcstrings` tool** -- create a new empty .xcstrings file with a given source language
- **`add_keys` tool** -- add new localization keys with source text to an .xcstrings file
- **`discover_files` tool** -- recursively search a directory for .xcstrings files
- **`update_comments` tool** -- update developer comments on existing localization keys
- **`extract_strings` prompt** -- guided workflow to extract hardcoded strings from Swift code
- **Multi-locale `get_untranslated`** -- `locales` parameter to find strings untranslated in any of multiple locales
- `FileAlreadyExists` error variant for create_xcstrings safety
- Service layer `creator` module with pure functions for file creation, key addition, and comment updates

## [0.5.0] - 2025-03-21

### Added
- **MCP Logging** -- real-time structured log notifications to clients via MCP protocol
- **`search_keys` tool** -- search keys by substring (case-insensitive), matches key names and source text
- **Xcode 26 compatibility** -- verified format version 1.1 roundtrip with test fixture
- **`localization_audit` prompt** -- complete audit: coverage, validation, stale keys, glossary
- **`fix_validation_errors` prompt** -- guided workflow to fix issues by severity
- **`add_language` prompt** -- add a new locale and translate all strings step-by-step

### Changed
- Extracted shared `build_translation_unit` helper to eliminate code duplication in extractor

### Removed
- 5 unused error variants (`ValidationFailed`, `FormatSpecifierMismatch`, `MissingPluralForm`, `ShouldNotTranslate`, `Unexpected`)

## [0.4.0] - 2025-03-21

### Added
- **Multi-file cache** -- parse and switch between multiple .xcstrings files
- **`list_files` tool** -- list all cached files with active status
- **`remove_locale` tool** -- remove a locale from all entries
- **`get_diff` tool** -- compare cached vs on-disk file changes
- **`get_glossary` / `update_glossary` tools** -- persistent translation glossary
- **`export_xliff` / `import_xliff` tools** -- XLIFF 1.2 export/import with validation
- **`translate_batch` prompt** -- batch translation instructions
- **`review_translations` prompt** -- quality review workflow
- **`full_translate` prompt** -- complete translation workflow
- `continue_on_error` parameter on `submit_translations`
- `accepted_keys` field in `SubmitResult`
- Separate glossary write lock
- Output path validation for XLIFF export (.xliff/.xlf required)
- Extension validation on `get_diff` file_path
- XLIFF import re-validates after write lock
- Sorted `list_files` output
- Integration tests and property-based tests for all new features

## [0.3.2] - 2025-03-20

### Added
- Homebrew tap support (`brew install Murzav/tap/xcstrings-mcp`)
- `--version` CLI flag
- Performance benchmarks in README

## [0.3.0] - 2025-03-20

### Added
- `get_plurals` tool -- extract keys needing plural/device/substitution translation
- `get_context` tool -- find related keys by shared prefix
- Substitution roundtrip -- merge plural forms into substitution JSON structure
- CLDR plural rules for 40+ locales

### Fixed
- Validator no longer rejects substitution plural forms for specifier mismatch

## [0.2.0] - 2025-03-19

### Added
- `get_coverage` -- per-locale coverage statistics
- `get_stale` -- find stale/removed keys
- `validate_translations` -- file-wide validation report
- `list_locales` -- locale listing with stats
- `add_locale` -- add new locale with empty translations

## [0.1.0] - 2025-03-18

### Added
- Initial release
- `parse_xcstrings` -- parse and cache .xcstrings files
- `get_untranslated` -- extract untranslated strings with batching
- `submit_translations` -- validate and write translations atomically
- Xcode-compatible JSON formatting (" : " colon spacing)
- Format specifier validation
- Atomic file writes with crash safety
