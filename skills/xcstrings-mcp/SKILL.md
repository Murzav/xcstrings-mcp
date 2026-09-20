---
name: xcstrings-mcp
description: >
  Use this skill for ANY task involving iOS/macOS localization: translating strings,
  adding or removing languages, checking coverage, finding missing or stale translations,
  validating format specifiers, managing plural forms, migrating from legacy .strings format,
  exporting/importing XLIFF for external translators, merging catalog changes across branches,
  resolving translation conflicts, managing glossary, adding new keys, extracting hardcoded
  strings from Swift code, or auditing localization quality.
  Activate whenever the user mentions languages, translations, localization, xcstrings,
  App Store markets, or anything related to making an iOS/macOS app multilingual.
---

# xcstrings-mcp Skill

## Trigger Conditions

### Translation & Language Management
- "translate app / translate to [language]"
- "add [language] / add Ukrainian / add Spanish"
- "remove [language] / remove locale"
- "app is not localized / missing translations"
- "переведи приложение / добавь украинский язык"
- "want to release on App Store in N languages"
- "localize my app / i18n / l10n"

### Coverage & Validation
- "check translation coverage / how many strings are translated in [language]"
- "find missing localization keys / find untranslated strings in xcstrings"
- "why are some strings not translated / yellow strings in Xcode String Catalog"
- "validate translations / check format specifiers in xcstrings"
- "find stale localization keys / unused translation keys in xcstrings"
- "xcstrings translation progress / localization completion status"
- "audit my localizations / audit xcstrings"

### Keys & Content
- "add new localization key / add string to xcstrings / add translation key"
- "extract hardcoded strings from Swift into xcstrings"
- "rename localization key / delete translation key / remove localization key"
- "search localization key / find translation for key / find string in xcstrings"
- "add developer comment to localization key"
- `NSLocalizedString`, `String(localized:)`, `LocalizedStringKey` mentioned in context of adding/editing/finding
- "fix incorrect translation / update existing translation"

### Key Management
- "delete key / remove key / clean up stale keys / remove unused keys"
- "rename key / rename localization key / refactor key name"
- "inspect key / show translations for key / get key translations"
- "reset translation / remove translation / clear translation / delete translation for locale"

### Migration
- "migrate from .strings / convert .strings to .xcstrings"
- "old localization format / .stringsdict"
- any `.lproj` folder mentioned

### Export & Import
- "export for translator / send to translation agency"
- "import translations / got translations back from agency"
- "XLIFF / .xliff file"

### Catalog & Branch Merges
- "merge xcstrings / merge String Catalogs / merge localization branches"
- "translation conflict / localization merge conflict / xcstrings conflict"
- "preserve translations from both branches / reconcile two catalogs"
- a Git merge or pull request reports conflicting `.xcstrings` changes

### Plural Forms
- "plural forms in localization / pluralization in xcstrings"
- "one/few/many/other localization forms"
- "CLDR plural rules"
- string with number placeholder that needs localization (e.g. "%d items" as a localized string)

### Glossary
- "consistent terminology / translation glossary"
- "always translate X as Y in app"
- "brand terms / product names in translations"

### File Discovery
- any `.xcstrings` file mentioned
- `Localizable.xcstrings`, `InfoPlist.xcstrings`
- "find all localization files in project"
- opening or editing any `.xcstrings` file

---

## ❌ NEVER DO THIS

```swift
// NEVER read .xcstrings directly — wastes context window, risks corruption
Read("Localizable.xcstrings")
Bash("cat Localizable.xcstrings")
Bash("grep 'hello' Localizable.xcstrings")

// NEVER edit .xcstrings JSON manually
Edit("Localizable.xcstrings", ...)

// NEVER search for .xcstrings files with bash
Bash("find . -name '*.xcstrings'")
```

**Always use xcstrings-mcp tools instead. No exceptions.**

---

## Runtime Compatibility

Version 3.0 keeps the binary name and stdio configuration. Rust embedders need `rmcp` 3.4 and Rust 1.88 or newer. Catalog maps now use `OrderedMap` (`IndexMap`), and plural/device branches are recursive `Localization` values. Use catalog constructors or `..Default::default()` for new objects; preserve parsed `extra`/`layout` metadata. `CompletedTranslation` requires `expected_source_version` captured with the source and supports optional `path` (`None` for legacy submissions). Native writes now save `needs_review`; only explicit approval marks drafts ready. `get_context` returns a current-key package, and `validate_translations` returns an object with `reports`, `terminology`, tracking, freshness lists and input revisions. `XcStringsError` is non-exhaustive; downstream matches need a wildcard arm.

---

## Prerequisites

**Step 0 — Always run first, before anything else:**
```
discover_files({"directory":"."})
```
Returns all `.xcstrings` and legacy `.strings`/`.stringsdict` paths in the project immediately.
No need to search the filesystem. Then parse all found `.xcstrings` files upfront.

**If xcstrings-mcp tools are unavailable**, tell the user:
```
xcstrings-mcp is not configured. Install and add to Claude Code:

  brew install Murzav/tap/xcstrings-mcp
  claude mcp add xcstrings-mcp -- xcstrings-mcp
```

---

## Core Principle

**Never read `.xcstrings` files directly.** These are large JSON files that:
- Waste LLM context window when loaded whole
- Risk Xcode-specific formatting corruption on manual edit
- Have no validation for format specifiers or CLDR plural rules

Always use xcstrings-mcp tools for every operation.

---

## Workflows

### Translate entire file (parallel subagents per language)

```
discover_files({"directory":"."})                         // Step 0: always first
→ parse_xcstrings({"file_path":"Localizable.xcstrings"}) // load into cache
→ list_locales({})                                       // get all locale codes
→ get_coverage({})                                       // see what needs work

// Spawn one subagent per locale simultaneously:
Subagent per locale:
  read all get_untranslated pages before writing to capture a finite worklist:
    get_untranslated({"locale":"uk","batch_size":50})
    retain each source_version; preserve existing drafts for review
    inspect leaves and diagnostics; translate each planned leaf once
    submit_translations({"translations":[{"key":"button.save","locale":"uk","expected_source_version":"<copy source_version captured with this input>","path":[],"value":"Зберегти"}]})
    accepted values stay needs_review; do not resubmit them because coverage is incomplete
  validate_translations and report get_review_queue for separate acceptance
```

**Always parallelize — one subagent per locale. Writes are atomic and lock-protected.**

### Review drafts and track source changes

Native submissions save `needs_review`, preserving text without counting it as complete. Draft generation must process a finite captured worklist once. Do not repeatedly submit existing drafts or approve them merely to improve coverage.

1. Preview `sync_source_changes` with `dry_run:true`. On first initialization the default `review` mode moves ready targets to review; `adopt_existing` is an explicit trust decision that preserves native states while acknowledging unknown historical freshness.
2. Apply the chosen mode with `expected` copied from the preview's `input_revisions`. Check `catalog_written`, `checkpoint_written`, `retry_required`, and `phase_error`. Two guarded file writes are not one transaction; on partial failure obtain a fresh preview and retry.
3. Read `get_review_queue`. Inspect source, previous-source evidence, actual draft text, diagnostics, and `get_context`. Missing translations are drafting work, not approval items.
4. Only after review, submit `approve_translations` with each item's `destination` flattened to `key`/`locale`/`path`, `expected_source_version` from `source_version`, and `expected_target_version` from `target_version`. Preview first. Approval changes state only and rejects the entire batch if any item is stale or invalid.
5. After any mutation restart the queue at offset zero. Later unchanged pages require `expected_queue_version` from the same first page. Report unresolved items.

Keep `<catalog>.xcstrings-mcp.json` alongside its catalog in version control: it stores source checkpoints, authored context and bounded pending-review evidence. Source/context edits immediately reduce effective readiness on reads; synchronization persists `needs_review` without losing text. Before initialization existing native ready states retain their counts but are explicitly untracked. External editors can author ready states after a current checkpoint; this is native readiness, not proof of approval through MCP. Pending evidence follows matching content and is not an audit log.

### Author context without inventing facts

Read `get_context` for the current key even when neighbor count is zero. `current` includes the source token, `authored_contexts` exposes the raw record for get-modify-set, and `leaf_contexts` contains exact variation overrides. Explicit neighbors rank before same-screen neighbors and the labeled prefix heuristic. Variable positions/formats are observed; their meanings must be authored.

Use `update_context` with `edits:[{"action":"set","key":"button.save","context":{"context":{"screen":"Editor","role":"button","purpose":"Save the current document"}}}]`. `set` replaces the whole addressed record: preserve unknown fields and leaf overrides when editing. Preview with `dry_run:true`, then pass the preview `input_revisions` as `expected`. `remove` deletes one authored record. Authored context changes invalidate source versions and require tracked translations to be reviewed. Screenshot references are inert metadata; they are not fetched or analyzed.

### Add a new language

```
discover_files({"directory":"."})
→ parse_xcstrings({"file_path":"Localizable.xcstrings"})
→ add_locale({"locale":"uk"})                            // creates empty entries
→ get_untranslated({"locale":"uk","batch_size":50})
→ submit_translations({"translations":[{"key":"button.save","locale":"uk","expected_source_version":"<copy source_version captured with this input>","value":"Зберегти"}]})
→ get_review_queue({"locale":"uk"})                   // drafts await separate review
→ get_coverage({})                                      // drafts remain incomplete
```

### Remove a language

```
discover_files({"directory":"."})
→ parse_xcstrings({"file_path":"Localizable.xcstrings"})
→ list_locales({})                                      // confirm locale code
→ remove_locale({"locale":"fr"})
```

### Check coverage & audit

```
discover_files({"directory":"."})
→ parse_xcstrings({"file_path":"Localizable.xcstrings"}) for each file
→ get_coverage({})                                      // per-locale percentages
→ validate_translations({})                             // format specifier errors, plural issues
→ get_stale({"locale":"uk"})                          // unused/removed keys to clean up
→ get_diff({})                                          // cached vs on-disk changes
```

Use built-in prompt `localization_audit` for full automated audit.

### Fix validation errors

Use built-in prompt: `fix_validation_errors`
Or manually:
```
validate_translations({})
→ search_keys({"pattern":"button.save","locale":"uk"})
→ submit_translations({"translations":[{"key":"button.save","locale":"uk","expected_source_version":"<copy source_version captured with this input>","value":"Зберегти"}]})
```

### Handle plurals, devices, substitutions, and chains

```
discover_files({"directory":"."})
→ parse_xcstrings({"file_path":"Localizable.xcstrings"})
→ get_plurals({"locale":"uk"})                        // inspect leaves[].path and diagnostics
→ submit_translations({"translations":[
    {"key":"phone_items","locale":"uk","expected_source_version":"<copy source_version captured with this input>","path":[{"device":"iphone"},{"plural":"few"}],"value":"%lld елементи"},
    {"key":"bird_count","locale":"uk","expected_source_version":"<copy source_version captured with this input>","path":[{"substitution":"BIRDS"},{"plural":"other"}],"value":"%arg птаха"}
  ],"dry_run":true})
→ inspect rejected/warnings, then submit with dry_run=false
```

Copy paths returned by the tools. `path: []` means the root stringUnit. Omit `path` for legacy simple requests or `plural_forms`/`substitution_name` aggregates; never combine those selectors with `path`. Preserve each leaf's own format arguments and substitution metadata. Native `accepted` counts input requests; `accepted_destinations` identifies actual key/locale/path leaves. Use `continue_on_error:false` for an all-or-nothing native batch.

All seven devices are supported: iphone, ipad, mac, applewatch, appletv, applevision, other. Supported chains include device→plural. Device text can reference substitutions declared at the localization root; their leaf paths start with `substitution`, independently of the device path. Nested substitutions inside a device branch are unsupported. A plural case cannot have further variations; device.other is a simple fallback. Target-only variations are valid. Unknown fields survive editing, but unsupported axes/shapes and unknown locales remain diagnostic and incomplete.

**Use the returned CLDR 48.2.1 requirements:** Ukrainian one/few/many/other, English one/other, French one/many/other, Japanese other. These completion recommendations exceed Xcode's compiler minimum in some locales. All required leaves must be translated or machine_translated; intentional empty text in those states is complete. Missing, new, needs_review, and unknown states are incomplete. Do not overwrite an intentional blank or mark source fallback text translated merely to increase coverage.

### Add new localization keys

```
discover_files({"directory":"."})
→ parse_xcstrings({"file_path":"Localizable.xcstrings"})
→ add_keys({"keys":[{"key":"button.save","source_text":"Save","comment":"Save button title"}]})
→ get_untranslated({"locale":"uk"}) for each locale
→ submit_translations({"translations":[{"key":"button.save","locale":"uk","expected_source_version":"<copy source_version captured with this input>","value":"Зберегти"}]})
```

### Extract hardcoded strings from Swift code

Use built-in prompt: `extract_strings`
This scans Swift source for hardcoded user-facing strings and adds them to `.xcstrings`.

### Migrate from legacy .strings / .stringsdict

```
discover_files({"directory":"."})                        // finds .xcstrings AND .strings files
→ import_strings({
    "directory":"./Resources",
    "source_language":"en",
    "output_path":"Localizable.xcstrings",
    "dry_run":true
  })                                                   // preview first, no writes
→ review dry_run output carefully
→ import_strings({
    "directory":"./Resources",
    "source_language":"en",
    "output_path":"Localizable.xcstrings"
  })                                                   // actual write
→ parse_xcstrings({"file_path":"Localizable.xcstrings"})
→ get_plurals({"locale":"uk"})                        // verify .stringsdict plural rules imported
→ get_untranslated({"locale":"uk"}) for remaining gaps
→ submit_translations({"translations":[{"key":"button.save","locale":"uk","expected_source_version":"<copy source_version captured with this input>","value":"Зберегти"}]})
```

Supports UTF-8 and UTF-16, merge into existing `.xcstrings`.

### Export for external translators (XLIFF)

Synchronize source tracking before exporting. Save the returned `source_versions` map together with the XML; it belongs to this catalog identity and this export. Import requires that captured map, including every imported key. Never replace old tokens with current ones to force outdated work through. CLI export also writes a `.source-versions.json` companion; CLI import requires `--source-versions PATH`.

Export supports Apple XLIFF 1.2 IDs for simple, plural, device, substitution, and supported chained leaves. It includes incomplete leaves by default; set `untranslated_only:false` for all leaves. Set `original` to the exact file scope expected by the recipient.

```
discover_files({"directory":"."})
→ parse_xcstrings({"file_path":"Localizable.xcstrings"})
→ export_xliff({"locale":"uk","original":"App/Localizable.xcstrings","output_path":"translations_uk.xliff"})
```

### Import back from translators (XLIFF)

Import one exact `original` when several catalog scopes exist; omitted selection is allowed only for one distinct scope. Do not guess a basename. Check `skipped_scopes`, `rejected`, `accepted_destinations`, and `missing_targets`. Import is all-or-nothing for the selected scope, using a fresh catalog and conditional atomic write. Rejection, stale-write failure, and dry run do not change the catalog or active cache.

Draft new/needs-review targets retain their text and state; translated + leveraged-mt maps to machine_translated. Missing target is a no-op, but an explicit empty translated target intentionally clears a leaf and is complete. This also applies to the exact empty catalog key. Xcode may skip drafts; do not mistake its behavior for the server's import policy. XLIFF accepted/exported counts are trans-units, which may share a catalog key.

Ambiguous destinations, duplicate IDs after XML normalization, malformed structures/namespaces, unsupported states, opaque inline placeholders, and unsafe shapes fail before writes. The importer accepts Xcode’s source-less plural units only when they contain a target and resolve to an existing target plural leaf; source-less simple units remain invalid. XLIFF 2.x is unsupported. Xcode can silently ignore literal keys that look like valid variation IDs or lose target-only substitution leaves. Unsafe literal-ID exports and delimiter-bearing substitution names fail safely; ordinary keys containing `|==|` remain supported. Keep native typed paths for supported catalogs that cannot safely roundtrip through Xcode, and compare changed content after external imports.

```
discover_files({"directory":"."})
→ parse_xcstrings({"file_path":"Localizable.xcstrings"})
→ import_xliff({"xliff_path":"translations_uk.xliff","original":"App/Localizable.xcstrings","expected_source_versions":{"button.save":"<copy this key from the saved export source_versions map>"},"dry_run":true})
→ review result, then repeat with dry_run=false
→ validate_translations({})
→ get_coverage({})
```

### Manage glossary for consistent terminology

```
get_glossary({"source_locale":"en","target_locale":"uk"}) // check existing terms
→ update_glossary({
    "source_locale":"en",
    "target_locale":"uk",
    "entries":{"Dashboard":"Панель","Settings":"Налаштування"},
    "expected_revision":"<copy revision from get_glossary>",
    "dry_run":true
  })
```

Preview, inspect `rejected`, then apply with `dry_run:false` and the same captured revision. Rich `upsert` rules support `preferred`, `forbidden`, `do_not_translate`, source/accepted variants and scoped exceptions. Checks are advisory: `status:unavailable` means policy could not be evaluated, while `absent` means no policy exists. Legacy files migrate only on explicit update.

**Always consult glossary before translating to ensure brand/product term consistency.**

### Clean up stale/unused keys

Use built-in prompt: `cleanup_stale`
Or manually:
```
discover_files({"directory":"."})
→ parse_xcstrings({"file_path":"Localizable.xcstrings"})
→ get_stale({"locale":"en","batch_size":100})
→ review stale keys
→ delete_keys({"keys":["unused_key_1","unused_key_2"]})
→ get_coverage({})                                      // verify no impact
```

### Rename a key (after Swift refactoring)

```
discover_files({"directory":"."})
→ parse_xcstrings({"file_path":"Localizable.xcstrings"})
→ search_keys({"pattern":"old_key_name","locale":"uk"}) // find the key
→ rename_key({"old_key":"old_key_name","new_key":"new_key_name"})
```

**All translations are preserved during rename.**

### Inspect a specific key

```
discover_files({"directory":"."})
→ parse_xcstrings({"file_path":"Localizable.xcstrings"})
→ get_key({"key":"button.save"})                         // all locales at once
```

Returns source text, comment, and per-locale translation states, leaves, and diagnostics.

### Fix broken translations

```
discover_files({"directory":"."})
→ parse_xcstrings({"file_path":"Localizable.xcstrings"})
→ validate_translations({})
→ delete_translations({"keys":["broken_key"],"locale":"uk"}) // reset to untranslated
→ get_untranslated({"locale":"uk"})                            // re-translate
→ submit_translations({"translations":[{"key":"broken_key","locale":"uk","expected_source_version":"<copy source_version captured with this input>","value":"Виправлено"}]})
```

### Merge conflicting String Catalog branches

```
discover_files({"directory":"."})
→ merge_xcstrings({
    "base_path":"/tmp/base.xcstrings",
    "current_path":"/tmp/current.xcstrings",
    "incoming_path":"/tmp/incoming.xcstrings",
    "output_path":"/tmp/merged.xcstrings",
    "dry_run":true
  })
→ review conflicts and introduced_validation_issues
→ copy each complete `conflicts[].id` and the complete `expected_fingerprints`
  values from that same dry-run; never construct, shorten, or reuse them
→ merge_xcstrings({
    "base_path":"/tmp/base.xcstrings",
    "current_path":"/tmp/current.xcstrings",
    "incoming_path":"/tmp/incoming.xcstrings",
    "output_path":"/tmp/merged.xcstrings",
    "dry_run":false,
    "resolutions":[{"conflict_id":"merge-v1:<placeholder; copy exact dry-run conflicts[].id over this entire string>","choice":"current"}],
    "expected_fingerprints":{
      "base":"sha256:<placeholder; copy exact dry-run expected_fingerprints.base over this entire string>",
      "current":"sha256:<placeholder; copy exact dry-run expected_fingerprints.current over this entire string>",
      "incoming":"sha256:<placeholder; copy exact dry-run expected_fingerprints.incoming over this entire string>",
      "output":null
    }
  })
→ parse_xcstrings({"file_path":"/tmp/merged.xcstrings"})
→ validate_translations({})
```

The `merge-v1:` and `sha256:` placeholders above show the exact wire shape. Overwrite each entire quoted placeholder, including the displayed prefix, with the exact complete string returned by dry-run. Never invent conflict values or edit the raw catalog. Choose only one authored side. Repeat dry-run after any stale-fingerprint error; do not reuse old fingerprints. A CLI dry-run with unresolved conflicts emits the JSON report but exits with status 2. Both merge and native typed mutation preserve unknown fields and existing member order.

---

## Error Handling

| Error | Action |
|---|---|
| `parse_xcstrings` fails | Check path from `discover_files`, verify file exists |
| `submit_translations` write error | Cooperating writers wait on the stable lock; on an actual error, re-read the file and follow the reported filesystem cause |
| path resolves to an internal sidecar | Use the real `.xcstrings` catalog path; never target an `xcstrings-mcp` lock or temp file |
| `validate_translations` returns errors | Use `fix_validation_errors` prompt |
| `import_strings` encoding error | Verify the input is supported UTF-8 or UTF-16; encoding is detected automatically |
| XLIFF rejected or stale write | No selected translations were written; resolve diagnostics and preview again against fresh catalog bytes |
| Unsupported locale or variation diagnostics | Preserve the catalog data and report the unsupported shape; never claim complete coverage |
| `merge_xcstrings` reports conflicts | Choose `current`, `incoming`, or `base` for every stable conflict ID, then apply with fresh fingerprints |
| `merge_xcstrings` reports stale fingerprints | Discard the failed apply inputs, run a new dry-run, and never retry apply with the old fingerprint object |
| MCP tool not found | Ask user to run `brew install Murzav/tap/xcstrings-mcp` |

---

## Optimal Parameters

| Parameter | Recommended value | Reason |
|---|---|---|
| `batch_size` | 50 | Fits context window, fast atomic writes |
| `dry_run` | Always `true` first on migration or merge | Preview changes and obtain apply fingerprints |
| `directory` | `"."` | Finds all files recursively from project root |

---

## Tool Quick Reference

| Tool | Trigger |
|---|---|
| `discover_files` | **Always first** — find all localization files |
| `parse_xcstrings` | Load file into cache before any operation |
| `list_locales` | See what languages exist |
| `add_locale` | Add a new language |
| `remove_locale` | Remove a language |
| `get_untranslated` | Get next batch to translate |
| `submit_translations` | Save drafts atomically using captured source versions |
| `get_review_queue` | Inspect existing drafts and source-change evidence |
| `approve_translations` | Mark explicitly reviewed drafts ready without changing text |
| `sync_source_changes` | Preview/apply source checkpoints and invalidate stale ready states |
| `update_context` | Set/remove authored context with revision guards |
| `get_coverage` | Check translation progress per locale |
| `validate_translations` | Find format/plural errors |
| `get_stale` | Find unused keys |
| `get_plurals` | Inspect required typed leaves for plural/device/substitution chains |
| `get_context` | Current source, authored context, variables, terms and neighbors with provenance |
| `search_keys` | Search by key name or source text |
| `add_keys` | Add new localization keys |
| `create_xcstrings` | Create new empty catalog from scratch |
| `update_comments` | Add/update developer comments on keys |
| `import_strings` | Migrate legacy .strings/.stringsdict |
| `export_xliff` | Export supported Apple leaves with exact original scope |
| `import_xliff` | Atomically import one scope, preserving draft states and explicit blank targets |
| `get_glossary` | Check consistent terminology |
| `update_glossary` | Add/update glossary terms |
| `get_diff` | Compare cache vs on-disk state |
| `delete_keys` | Delete keys found by `get_stale` or manually |
| `delete_translations` | Reset specific translations to untranslated |
| `get_key` | Inspect one key across all locales |
| `rename_key` | Rename key preserving all translations |
| `list_files` | List all currently cached files |
| `merge_xcstrings` | Three-way merge whole catalogs with dry-run fingerprints and explicit conflict choices |
