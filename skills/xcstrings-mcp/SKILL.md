---
name: xcstrings-mcp
description: >
  Use this skill for ANY task involving iOS/macOS localization: translating strings,
  adding or removing languages, checking coverage, finding missing or stale translations,
  validating format specifiers, managing plural forms, migrating from legacy .strings format,
  exporting/importing XLIFF for external translators, managing glossary, adding new keys,
  extracting hardcoded strings from Swift code, or auditing localization quality.
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

## Prerequisites

**Step 0 — Always run first, before anything else:**
```
discover_files(directory: ".")
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
discover_files(directory: ".")                        // Step 0: always first
→ parse_xcstrings(path: "Localizable.xcstrings")      // load into cache
→ list_locales()                                      // get all locale codes
→ get_coverage()                                      // see what needs work

// Spawn one subagent per locale simultaneously:
Subagent per locale:
  loop:
    get_untranslated(locale: "uk", batch_size: 50)    // optimal batch size: 50
    if empty → done
    submit_translations(locale: "uk", entries: [...]) // atomic write, lock-safe
```

**Always parallelize — one subagent per locale. Writes are atomic and lock-protected.**

### Add a new language

```
discover_files(directory: ".")
→ parse_xcstrings(path: "Localizable.xcstrings")
→ add_locale(locale: "uk")                            // creates empty entries
→ get_untranslated(locale: "uk", batch_size: 50)
→ submit_translations(locale: "uk", entries: [...])
→ get_coverage()                                      // verify result
```

### Remove a language

```
discover_files(directory: ".")
→ parse_xcstrings(path: "Localizable.xcstrings")
→ list_locales()                                      // confirm locale code
→ remove_locale(locale: "fr")
```

### Check coverage & audit

```
discover_files(directory: ".")
→ parse_xcstrings(path) for each file
→ get_coverage()                                      // per-locale percentages
→ validate_translations()                             // format specifier errors, plural issues
→ get_stale()                                         // unused/removed keys to clean up
→ get_diff()                                          // cached vs on-disk changes
```

Use built-in prompt `localization_audit` for full automated audit.

### Fix validation errors

Use built-in prompt: `fix_validation_errors`
Or manually:
```
validate_translations()
→ search_keys(query: "<problematic key>")
→ submit_translations(locale, corrected_entries)
```

### Handle plural forms

```
discover_files(directory: ".")
→ parse_xcstrings(path)
→ get_plurals()                                       // keys needing one/few/many/other forms
→ submit_translations with all required CLDR categories per locale:
    // uk needs: one, few, many
    // en needs: one, other
    // ja needs: other only
```

**Always check CLDR categories per locale — they differ significantly.**

### Add new localization keys

```
discover_files(directory: ".")
→ parse_xcstrings(path)
→ add_keys(keys: [{ key: "button.save", value: "Save", comment: "Save button title" }])
→ get_untranslated(locale) for each locale
→ submit_translations(locale, entries)
```

### Extract hardcoded strings from Swift code

Use built-in prompt: `extract_strings`
This scans Swift source for hardcoded user-facing strings and adds them to `.xcstrings`.

### Migrate from legacy .strings / .stringsdict

```
discover_files(directory: ".")                        // finds .xcstrings AND .strings files
→ import_strings(
    directory: "./Resources",
    source_language: "en",
    output_path: "Localizable.xcstrings",
    dry_run: true                                     // preview first, no writes
  )
→ review dry_run output carefully
→ import_strings(
    directory: "./Resources",
    source_language: "en",
    output_path: "Localizable.xcstrings"              // actual write
  )
→ parse_xcstrings("Localizable.xcstrings")
→ get_plurals()                                       // verify .stringsdict plural rules imported
→ get_untranslated(locale) for remaining gaps
→ submit_translations(locale, entries)
```

Supports UTF-8 and UTF-16, merge into existing `.xcstrings`.

### Export for external translators (XLIFF)

```
discover_files(directory: ".")
→ parse_xcstrings(path)
→ export_xliff(locale: "uk", output_path: "translations_uk.xliff")
```

### Import back from translators (XLIFF)

```
discover_files(directory: ".")
→ parse_xcstrings(path)
→ import_xliff(path: "translations_uk.xliff")
→ validate_translations()                             // always validate after import
→ get_coverage()
```

### Manage glossary for consistent terminology

```
get_glossary(source_locale: "en", target_locale: "uk")  // check existing terms
→ update_glossary(source_locale: "en", target_locale: "uk", terms: [
    { source: "Dashboard", target: "Панель" },
    { source: "Settings", target: "Налаштування" }
  ])
```

**Always consult glossary before translating to ensure brand/product term consistency.**

### Clean up stale/unused keys

Use built-in prompt: `cleanup_stale`
Or manually:
```
discover_files(directory: ".")
→ parse_xcstrings(path)
→ get_stale(locale: "en", batch_size: 100)
→ review stale keys
→ delete_keys(keys: ["unused_key_1", "unused_key_2"])
→ get_coverage()                                      // verify no impact
```

### Rename a key (after Swift refactoring)

```
discover_files(directory: ".")
→ parse_xcstrings(path)
→ search_keys(query: "old_key_name")                  // find the key
→ rename_key(old_key: "old_key_name", new_key: "new_key_name")
```

**All translations are preserved during rename.**

### Inspect a specific key

```
discover_files(directory: ".")
→ parse_xcstrings(path)
→ get_key(key: "button.save")                         // all locales at once
```

Returns source text, comment, and translation state for every locale.

### Fix broken translations

```
discover_files(directory: ".")
→ parse_xcstrings(path)
→ validate_translations()
→ delete_translations(keys: ["broken_key"], locale: "uk")  // reset to untranslated
→ get_untranslated(locale: "uk")                            // re-translate
→ submit_translations(...)
```

---

## Error Handling

| Error | Action |
|---|---|
| `parse_xcstrings` fails | Check path from `discover_files`, verify file exists |
| `submit_translations` conflict | Retry — lock is held <1ms, contention is transient |
| `validate_translations` returns errors | Use `fix_validation_errors` prompt |
| `import_strings` encoding error | Try with `encoding: "utf16"` parameter |
| MCP tool not found | Ask user to run `brew install Murzav/tap/xcstrings-mcp` |

---

## Optimal Parameters

| Parameter | Recommended value | Reason |
|---|---|---|
| `batch_size` | 50 | Fits context window, fast atomic writes |
| `dry_run` | Always `true` first on migration | Preview before irreversible changes |
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
| `submit_translations` | Write completed translations atomically |
| `get_coverage` | Check translation progress per locale |
| `validate_translations` | Find format/plural errors |
| `get_stale` | Find unused keys |
| `get_plurals` | Get keys needing plural forms |
| `get_context` | Find related keys by shared prefix |
| `search_keys` | Search by key name or source text |
| `add_keys` | Add new localization keys |
| `create_xcstrings` | Create new empty catalog from scratch |
| `update_comments` | Add/update developer comments on keys |
| `import_strings` | Migrate legacy .strings/.stringsdict |
| `export_xliff` | Export for external translators |
| `import_xliff` | Import from external translators |
| `get_glossary` | Check consistent terminology |
| `update_glossary` | Add/update glossary terms |
| `get_diff` | Compare cache vs on-disk state |
| `delete_keys` | Delete keys found by `get_stale` or manually |
| `delete_translations` | Reset specific translations to untranslated |
| `get_key` | Inspect one key across all locales |
| `rename_key` | Rename key preserving all translations |
| `list_files` | List all currently cached files |
