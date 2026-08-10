# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

### Added
- `merge_xcstrings` MCP tool and `merge` CLI command now provide conservative ordered two-sided/three-way String Catalog merges. Dry-runs return raw-byte fingerprints, automatic choices, validation deltas, and stable paginated conflicts; apply accepts only `current`/`incoming`/`base` resolutions and refuses unresolved, newly invalid, or stale results.

### Changed
- **BREAKING (Rust library API):** the public `XcStringsMcpServer` now implements `rmcp` 3.1.2's `ServerHandler` and requires Rust 1.88. Rust embedders with a direct `rmcp` dependency must update it from 2.x to 3.1.2. `XcStringsError` is now `#[non_exhaustive]`, so downstream exhaustive matches must add a wildcard arm. The CLI binary, stdio transport, and MCP client configuration are unchanged.
- Format validation now parses Foundation arguments deterministically, including the integer `j` length modifier, preserves position, conversion, length modifier, flags, width, and precision per occurrence, permits valid positional reordering and repeated ABI-compatible argument references, and rejects incompatible reuse across simple, plural, and substitution validation paths.
- `submit_translations`, MCP XLIFF import, CLI XLIFF import, and file validation now expose non-blocking, machine-readable warnings when ambiguous percent-in-prose sequences differ.
- Filesystem writes now coordinate through a stable sibling advisory lock instead of locking the replaceable destination inode. The public `FileStore` has a source-compatible conditional-write method whose default fails closed; the real store compares exact expected bytes or expected absence under the lock before temp-file fsync and atomic rename.

### Fixed
- Merge dry-runs with unresolved conflicts now retain the complete CLI JSON report while returning validation exit status 2. Validation deltas list only issues that remain in the candidate, MCP pagination schema matches the 1–500 runtime range, conflict metadata handles marker-like key names, and cached path aliases refresh by canonical file identity without switching the active file.
- Filesystem temp cleanup is now target-owned and performed under that target's stable lock, so starting another cooperating store cannot remove an active writer's temp. Expected-absence writes also reject and preserve dangling symlinks, existing BOM-prefixed output reports the correct key count, and unused merge resolutions are diagnosed deterministically in request order.
- Catalog aliases can no longer resolve to internal lock/temp sidecars or non-catalog files, and stable locks are opened without following redirects and accepted only as uniquely linked regular files. Reparsing a retargeted symlink now evicts its obsolete cache identity instead of leaving a duplicate `list_files` entry.
- Natural percentage prose such as `100% Local Storage`, `You've logged 85% of...`, and `7.0-8.0% - Acceptable...` is no longer rejected as a definite format mismatch.
- Definite format arguments next to unspaced Han, Hiragana, Katakana, or Hangul text (for example `%lld日`) remain blocking validation requirements, while other Unicode word continuations remain ambiguous; substitution plurals require exact `%arg` placeholders instead of accepting longer Unicode words or escaped percent sequences.
- Zero or overflowing value, dynamic-width, and dynamic-precision positional indices now fail with `invalid_positional_argument` instead of being interpreted as sequential arguments.
- Foundation integer length modifiers now reject unsupported uppercase legacy, `i`, and `n` conversions instead of accepting malformed pairs such as `%hD` or `%llO`.
- XLIFF import now decodes and XML-normalizes `trans-unit@id` and `file@target-language`, preserving named and numeric character references in keys and locales.
- XLIFF import now accepts both default-namespace and prefix-qualified XLIFF 1.2 documents from external tools while retaining legacy fully unqualified input compatibility. Imports require exactly one `<xliff>` document root; scope `file` locales and `trans-unit` data to their enclosing frames; enforce required `body`/`source`, parent order, and target cardinality; support recursive groups and schema-positioned bound extensions; and reject malformed structure, mixed file locales or namespace modes, unbound prefixes, malformed namespace references, and duplicate raw or expanded attributes before any catalog write.
- XLIFF export now omits variation-only entries from both output and counts while preserving unlocalized simple keys. Import accepts Xcode's empty simple-unit IDs safely, rejects unsupported Apple `|==|` variation paths, enforces per-file `trans-unit`/`bin-unit` ID uniqueness after XML attribute normalization, and fails closed on raw line-break/space or cross-file ID collisions before any catalog write.
- XLIFF import now preserves decoded CDATA segments when concatenating mixed target text, CDATA, and entity content; CDATA outside the document root or left unclosed remains a no-write parse error.

## [1.3.4] - 2026-07-20

### Changed
- **rmcp** (MCP SDK) 1.6 → 2.2. The server no longer emits MCP `logging/message` notifications — the `logging` capability was never advertised, so those notifications were off-spec. Tool progress messages now go to stderr via `tracing` only.
- Dependency refresh: quick-xml 0.41, tokio 1.53, regex 1.13, clap 4.6.2, clap_complete 4.6.7. CI actions: `actions/checkout` v7, `schneegans/dynamic-badges-action` 1.9.0.

### Security
- **quick-xml** 0.41 closes RUSTSEC-2026-0194 and RUSTSEC-2026-0195 (quadratic attribute parsing, unbounded namespace allocation).
- **crossbeam-epoch** 0.9.20 closes RUSTSEC-2026-0204 (dev-dependency chain).

### Fixed
- CI: nightly clippy lints `chunks_exact_to_as_chunks` (UTF-16 strings parser) and `cloned_ref_to_slice_refs` (merger tests) no longer fail the pipeline.

## [1.3.3] - 2026-05-01

### Changed
- **rmcp** 1.3 → 1.6 (manifest + lockfile). Required code changes:
  - `#[prompt_router]` in rmcp-macros 1.4+ generates a private associated function by default — now passes `vis = "pub(crate)"` so `XcStringsMcpServer::new()` (in a different module) can call `Self::prompt_router()` once at startup.
  - The handler attributes now point at cached struct fields (`#[tool_handler(router = self.tool_router)]`, `#[prompt_handler(router = self.prompt_router)]`) so the routers are built once in `XcStringsMcpServer::new()` instead of being rebuilt on every JSON-RPC dispatch (rmcp 1.4+ macros default to `Self::tool_router()` / `Self::prompt_router()` function calls, which would allocate a fresh `HashMap` + per-route `Arc<closure>` on each `tools/call`, `tools/list`, `prompts/get`, `prompts/list`).
- **libc** 0.2.183 → 0.2.186 (manifest + lockfile).
- Lockfile-only refreshes (manifest constraints unchanged): tokio 1.50 → 1.52.1 (`spawn_blocking` regression fix), indexmap 2.13 → 2.14 (hashbrown 0.16 → 0.17), clap 4.6.0 → 4.6.1, clap_complete 4.6.0 → 4.6.3, assert_cmd 2.2.0 → 2.2.1 (dev).

## [1.3.2] - 2026-03-31

### Changed
- Dependency refresh: rmcp 1.2 → 1.3, quick-xml 0.37 → 0.39, insta 1.42 → 1.47, proptest 1.7 → 1.11.
- quick-xml 0.39 migration: replace removed `BytesText::unescape()` with `decode()`; handle new `Event::GeneralRef` variant for XML entity resolution using `quick_xml::escape::resolve_xml_entity()` and `BytesRef::resolve_char_ref()`.

## [1.3.1] - 2026-03-25

### Improved
- **AI-facing documentation overhaul** across all 27 MCP tools for better AI client comprehension:
  - Server instructions restructured from flat text into workflow categories (SETUP → TRANSLATE → REVIEW → MANAGE → MIGRATE → GLOSSARY) with pagination rules
  - 8 tool descriptions enriched with cross-references (e.g., get_stale → delete_keys, get_plurals → submit_translations with plural_forms) and severity levels for validate_translations
  - Doc comments added to all fields of 11 model types (CompletedTranslation, TranslationUnit, PluralUnit, SubmitResult, etc.) — these generate JSON Schema descriptions visible to AI clients
  - Pagination hints added to all batched tool parameters ("set to previous offset + batch_size to get the next page")
  - Parameter descriptions enriched with examples, constraints, and usage guidance across 9 tool files
  - Prompts improved: rejection handling guidance in translate_batch, glossary check in full_translate

## [1.3.0] - 2026-03-25

### Added
- **11 CLI commands** for direct terminal access to localization operations:
  - `info` -- show file summary (source language, keys, locales)
  - `coverage` -- translation coverage per locale with percentage table
  - `validate` -- check format specifiers, plural forms, empty translations
  - `search` -- find keys by pattern (case-insensitive)
  - `stale` -- list stale/removed keys
  - `add-locale` -- add a new locale to all translatable keys
  - `remove-locale` -- remove a locale and all its translations
  - `export` -- export translations to XLIFF 1.2 format
  - `import` -- import translations from XLIFF files with validation
  - `migrate` -- migrate legacy .strings/.stringsdict to .xcstrings
  - `completions` -- generate shell completions (bash/zsh/fish/powershell/elvish)
- **Auto-discovery**: CLI commands automatically find .xcstrings files in the current directory tree -- no path required
- **`--json` flag**: machine-readable JSON output for CI/CD integration on all commands
- **`--dry-run` flag**: preview changes without writing on all mutation commands (add-locale, remove-locale, import, migrate)
- Shell completions for bash, zsh, fish, powershell, elvish via `clap_complete`

### Changed
- `service` module now public for CLI reuse (removed `_test_support` re-export hack)
- Extracted `service::discovery` module from `tools::files` for file discovery logic reuse
- Extracted `service::migrate` module from `tools::strings` for legacy migration logic reuse
- Binary now returns `ExitCode` instead of `Result` for proper exit code handling

## [1.2.0] - 2026-03-22

### Added
- **`delete_keys` tool** -- delete localization keys and all their translations, completes the `get_stale` → `delete_keys` cleanup workflow
- **`rename_key` tool** -- rename a localization key preserving all existing translations across all locales
- **`get_key` tool** -- get all translations for a specific key across all locales in a single call
- **`delete_translations` tool** -- remove translations for specific keys in a locale, resetting them to untranslated state
- **`cleanup_stale` prompt** -- guided workflow to find and remove stale/unused localization keys
- Claude Code skill (`skills/xcstrings-mcp/SKILL.md`) with workflows and best practices for all 26 tools
- GitHub community profile: Code of Conduct, issue templates, PR template, Discussions enabled

### Changed
- Write operations (`delete_keys`, `delete_translations`) skip disk write when no changes are made
- Repository topics expanded to 19 for better discoverability
- README enhanced with Features section, additional badges, and skill installation instructions

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
