# 🌐 xcstrings-mcp

MCP server for iOS/macOS .xcstrings (String Catalog) localization. Parse, translate, validate, and export from any AI coding assistant.

[![CI](https://github.com/Murzav/xcstrings-mcp/actions/workflows/ci.yml/badge.svg)](https://github.com/Murzav/xcstrings-mcp/actions/workflows/ci.yml)
![Coverage](https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/Murzav/b7cd209fd268df81711e04622ff051e8/raw/coverage.json)
[![Crates.io](https://img.shields.io/crates/v/xcstrings-mcp)](https://crates.io/crates/xcstrings-mcp)
[![Downloads](https://img.shields.io/crates/d/xcstrings-mcp)](https://crates.io/crates/xcstrings-mcp)
[![MSRV](https://img.shields.io/badge/MSRV-1.88-blue)](https://blog.rust-lang.org/)
[![License](https://img.shields.io/crates/l/xcstrings-mcp)](LICENSE-MIT)
[![MCP](https://img.shields.io/badge/MCP-compatible-green)](https://modelcontextprotocol.io)

## Why this exists

`.xcstrings` files are big JSON. Loading one whole into a model burns a noticeable chunk of the context window for almost no reason — most translation work touches a handful of keys at a time. Hand-editing is also fragile: Xcode formats these files in a very specific way (the `" : "` spacing, key order), and a stray reformat shows up as pure diff noise on the next commit. And then there are CLDR plurals, where every locale wants its own subset of `one/few/many/other` — easy to miss, painful to debug.

xcstrings-mcp is a small Rust process that sits between the assistant and the file. The assistant calls structured tools (read this batch, validate that translation, write the result atomically); the bytes on disk stay byte-identical to what Xcode would produce.

## Features

- 28 MCP tools, 8 prompts, and 12 CLI commands for the full translation lifecycle
- Batch translation that fits the context window: pull 50–100 keys at a time
- Deterministic Foundation format-argument and CLDR plural validation: definite `%d`/`%@`/`%jd` mismatches block (including arguments next to unspaced Han, Hiragana, Katakana, or Hangul text such as `%lld日`), while percent signs in prose such as `85% of` are accepted with explicit warnings; substitution plurals require exact `%arg` placeholders and reject longer Unicode words
- Atomic writes that match Xcode's JSON formatting exactly (`" : "` colon spacing, preserved key order, BOM stripped on read and never re-emitted)
- Legacy migration from `.strings` and `.stringsdict` (UTF-8/UTF-16, plural rules, positional specifiers)
- XLIFF 1.2 import/export with decoded, XML-normalized attributes and namespace URIs plus consistent official default/prefix-qualified namespace support for external translation tools; imports enforce `xliff > file > body > group/trans-unit` structure, accept bound foreign extensions only at schema extension points, reject malformed order/cardinality before writes, and retain fully unqualified legacy compatibility without allowing mixed structural modes
- Glossary support so terminology stays consistent across locales
- Conservative ordered three-way catalog merge with stable conflicts, dry-run fingerprints, and compare-and-swap apply
- Tested with Claude Code, Cursor, VS Code + Copilot, Windsurf, Zed, and OpenAI Codex; should work with any MCP client

## Quick Start

### Install

```sh
brew install Murzav/tap/xcstrings-mcp
# or
cargo install xcstrings-mcp
# or
cargo binstall xcstrings-mcp
```

### Configure

<details>
<summary><strong>Claude Code</strong></summary>

```sh
claude mcp add xcstrings-mcp -- xcstrings-mcp
```
</details>

<details>
<summary><strong>Cursor</strong></summary>

Add to `.cursor/mcp.json`:
```json
{
  "mcpServers": {
    "xcstrings-mcp": {
      "command": "xcstrings-mcp",
      "args": []
    }
  }
}
```
</details>

<details>
<summary><strong>Windsurf</strong></summary>

Add to `~/.codeium/windsurf/mcp_config.json`:
```json
{
  "mcpServers": {
    "xcstrings-mcp": {
      "command": "xcstrings-mcp",
      "args": []
    }
  }
}
```
</details>

<details>
<summary><strong>VS Code + Copilot</strong></summary>

Add to `.vscode/mcp.json`:
```json
{
  "servers": {
    "xcstrings-mcp": {
      "type": "stdio",
      "command": "xcstrings-mcp",
      "args": []
    }
  }
}
```
</details>

<details>
<summary><strong>Zed</strong></summary>

Add to Zed settings (`settings.json`):
```json
{
  "context_servers": {
    "xcstrings-mcp": {
      "command": {
        "path": "xcstrings-mcp",
        "args": []
      }
    }
  }
}
```
</details>

<details>
<summary><strong>OpenAI Codex</strong></summary>

Add to your project's `codex.json` or configure via Codex CLI:
```json
{
  "mcpServers": {
    "xcstrings-mcp": {
      "command": "xcstrings-mcp",
      "args": []
    }
  }
}
```
</details>

<details>
<summary><strong>Any MCP client (generic)</strong></summary>

xcstrings-mcp communicates via stdio using JSON-RPC. Point your MCP client to the binary:
```
command: xcstrings-mcp
transport: stdio
```
</details>

### Rust library compatibility

The CLI binary and stdio MCP configuration are unchanged. Rust applications that embed the public `XcStringsMcpServer` type must use `rmcp` 3.1.2 and Rust 1.88 or newer:

```toml
rmcp = { version = "3.1.2", features = ["server", "transport-io", "macros"] }
```

`rmcp` 2.x and 3.x expose distinct Rust traits and response types, so an embedder compiled against `rmcp` 2.x must update its direct dependency before adopting this release.

The public `XcStringsError` enum is now non-exhaustive so future error variants do not create the same source break. Downstream matches must add a wildcard arm:

```rust
match error {
    XcStringsError::FileNotFound { .. } => handle_missing(),
    XcStringsError::InvalidFormat(message) => handle_invalid(message),
    _ => handle_other(),
}
```

## Usage

The basic loop:

1. Parse the `.xcstrings` once to cache it.
2. Pull untranslated strings in batches.
3. Submit translations. The server validates and writes atomically.

```
parse_xcstrings → get_untranslated → submit_translations
```

For projects with multiple `.xcstrings` files, parse each one. The server keeps them all in memory and tracks which is "active". `list_files` shows what's loaded. If the same displayed symlink is retargeted and parsed again, its obsolete canonical identity is evicted so the list contains one current entry.

## Tools

| Tool | Description |
|------|-------------|
| `parse_xcstrings` | Parse and cache `.xcstrings` file |
| `get_untranslated` | Get untranslated strings with batching; `format_specifiers` lists definite arguments only |
| `submit_translations` | Validate and write atomically; blocking format failures go to `rejected[]`, accepted percent-prose ambiguities to `warnings[]` |
| `get_coverage` | Per-locale coverage statistics |
| `get_stale` | Find stale/removed keys; `format_specifiers` excludes percent-in-prose ambiguities |
| `validate_translations` | File-wide errors/warnings using the same simple, plural, and substitution format rules as submit |
| `list_locales` | List locales with stats |
| `add_locale` | Add new locale with empty translations |
| `remove_locale` | Remove a locale from all entries |
| `get_plurals` | Extract keys needing plural translation with definite-only `format_specifiers` |
| `get_context` | Find related keys by shared prefix |
| `list_files` | List all cached files with active status |
| `get_diff` | Compare cached vs on-disk file (added/removed/modified keys) |
| `get_glossary` | Get translation glossary entries for a locale pair |
| `update_glossary` | Add or update glossary terms |
| `export_xliff` | Export simple `stringUnit` entries to XLIFF 1.2; variation-only plural, device, and substitution entries are excluded |
| `import_xliff` | Import simple `stringUnit` IDs from one-root XLIFF 1.2; empty Xcode IDs are accepted safely, while Apple `|==|` variation IDs, XML-normalized duplicate unit IDs, malformed structure/namespaces, and unsafe multi-file collisions are rejected before writes |
| `import_strings` | Migrate legacy `.strings`/`.stringsdict` files to `.xcstrings` |
| `search_keys` | Search keys by substring; `format_specifiers` lists definite arguments only |
| `create_xcstrings` | Create a new empty .xcstrings file |
| `add_keys` | Add new localization keys with source text |
| `discover_files` | Find .xcstrings and legacy .strings/.stringsdict files |
| `update_comments` | Update developer comments on localization keys |
| `delete_keys` | Delete localization keys and all their translations |
| `delete_translations` | Remove translations for specific keys in a locale, resetting to untranslated |
| `get_key` | Get all translations for a specific key across all locales |
| `rename_key` | Rename a localization key, preserving all translations |
| `merge_xcstrings` | Three-way semantic merge of complete catalogs with stable conflicts, resolutions, validation delta, and exact-byte CAS apply |

### Merging catalog conflicts

`merge_xcstrings` takes an explicit common ancestor (`base`), the checked-out catalog (`current`), the catalog being merged (`incoming`), and an `output` path. It works on ordered raw JSON: current-side order is retained, incoming-only map entries are appended in incoming order, and future fields survive a clean merge. Known catalog maps are merged recursively, but each `stringUnit` and every unknown subtree is atomic, so the merge never invents a value/state combination or combines future schema it does not understand.

Always start with a dry-run:

```text
merge_xcstrings(
  base_path: "/tmp/base.xcstrings",
  current_path: "/tmp/current.xcstrings",
  incoming_path: "/tmp/incoming.xcstrings",
  output_path: "/tmp/merged.xcstrings",
  dry_run: true
)
```

The report contains raw-byte `sha256:` fingerprints, key counts, automatic current/incoming choices, existing and introduced validation issues, and a paginated conflict list. Resolve each stable conflict ID by selecting only `current`, `incoming`, or `base`. Then call the tool again with `dry_run: false`, the resolutions, and the report's complete `expected_fingerprints` object. Apply refuses unresolved conflicts, newly introduced blocking validation errors, changed inputs, and stale, missing, or unexpectedly created output.

The equivalent CLI is machine-readable on stdout. A dry-run with unresolved conflicts still emits the complete JSON report and performs no write, but exits with status 2 so automation cannot mistake it for a conflict-free preview:

```sh
xcstrings-mcp --json merge \
  --base /tmp/base.xcstrings \
  --current /tmp/current.xcstrings \
  --incoming /tmp/incoming.xcstrings \
  --output /tmp/merged.xcstrings

xcstrings-mcp --json merge \
  --base /tmp/base.xcstrings \
  --current /tmp/current.xcstrings \
  --incoming /tmp/incoming.xcstrings \
  --output /tmp/merged.xcstrings \
  --dry-run false \
  --resolution 'merge-v1:...=current' \
  --expected-fingerprints '{"base":"sha256:...","current":"sha256:...","incoming":"sha256:...","output":null}'
```

Filesystem apply compares the exact expected output bytes while holding a stable sibling advisory lock, then writes one target-owned temporary file, fsyncs it, and atomically renames it. Any orphan cleanup for that target happens under the same lock. Expected absence is directory-entry aware, so an unexpected dangling symlink is rejected instead of replaced. Live `.xcstrings` aliases must resolve to real `.xcstrings` catalogs; internal lock/temp sidecars are reserved, and redirected, non-regular, or multiply linked lock files fail closed. This serializes cooperating xcstrings-mcp CLI/MCP writers. It cannot prevent an external editor that ignores the advisory lock; the fingerprint/CAS checks detect stale state when it is observable, but this is not a multi-file atomic snapshot. The semantic merge preserves unknown raw JSON, while later legacy typed mutation tools do not promise to preserve unknown fields.

### Prompts

| Prompt | Description |
|--------|-------------|
| `translate_batch` | Step-by-step instructions for batch translation |
| `review_translations` | Instructions for quality review of translations |
| `full_translate` | Complete workflow for translating an entire file |
| `localization_audit` | Full audit: coverage, validation, stale keys, glossary |
| `fix_validation_errors` | Guided workflow to fix all validation issues |
| `add_language` | Add a new locale and translate all strings |
| `extract_strings` | Extract hardcoded strings from Swift code into .xcstrings |
| `cleanup_stale` | Find and remove stale/unused localization keys |

### Migrating from legacy .strings

If you're moving from `.strings` / `.stringsdict`:

```
discover_files → import_strings → get_untranslated → submit_translations
```

Always preview with `dry_run` before writing:
```
import_strings(directory: "./Resources", source_language: "en", output_path: "./Localizable.xcstrings", dry_run: true)
import_strings(directory: "./Resources", source_language: "en", output_path: "./Localizable.xcstrings")
```

If your project uses `.stringsdict` plurals, pull plural keys explicitly after import:
```
import_strings → get_plurals → get_untranslated → submit_translations
```

Migration handles UTF-8 and UTF-16, single- and multi-variable plural rules with positional specifiers, developer comments, unquoted keys, and merging into an existing `.xcstrings`.

### Starting from scratch

For a brand-new file:

```
create_xcstrings → add_keys → add_locale → get_untranslated → submit_translations
```

The `extract_strings` prompt walks the assistant through pulling hardcoded strings out of Swift source and into the catalog.

## CLI Commands

Same binary, just call it with a subcommand. Useful in CI, shell scripts, or for one-off poking around without an assistant.

CLI commands auto-discover `.xcstrings` in the current directory tree, so most invocations don't need a path:

```bash
cd MyProject/
xcstrings-mcp coverage              # finds Localizable.xcstrings on its own
xcstrings-mcp validate --locale uk
xcstrings-mcp add-locale fr
xcstrings-mcp export --locale de -o out.xliff
```

| Command | Description |
|---------|-------------|
| `info` | File summary: source language, keys, locales |
| `coverage` | Translation coverage per locale |
| `validate` | Check definite Foundation arguments, exact substitution placeholders, percent-prose warnings, plurals, and empty values |
| `search <pattern>` | Find keys by substring |
| `stale` | List stale/removed keys |
| `add-locale <locale>` | Add a new locale |
| `remove-locale <locale>` | Remove a locale |
| `export` | Export simple `stringUnit` entries to XLIFF 1.2; skip variation-only entries |
| `import` | Import simple `stringUnit` IDs from structurally validated XLIFF 1.2; empty Xcode IDs are accepted safely, while Apple `|==|` variation IDs, XML-normalized duplicate unit IDs, malformed structure/namespaces, and unsafe multi-file collisions fail before writes |
| `migrate` | Migrate legacy .strings/.stringsdict |
| `merge` | Three-way semantic catalog merge; dry-run by default, apply with exact fingerprints and resolutions |
| `completions <shell>` | Generate shell completions |

XLIFF unit IDs are compared after XML 1.0 attribute normalization. An Xcode
export whose distinct raw keys differ only by an attribute line break versus a
space therefore fails closed as a duplicate instead of silently overwriting a
catalog translation.

`--json` is available everywhere for machine-readable output. Mutating commands support `--dry-run`. Validation keeps definite format mismatches and invalid positional arguments blocking, recognizes arguments next to unspaced Han, Hiragana, Katakana, and Hangul text, and rejects `%arg` when it is merely a prefix of a longer Unicode word. XLIFF import reports accepted ambiguous percent sequences in `warnings[]` (and on stderr in human-readable mode).

### CLI Options

```sh
xcstrings-mcp --glossary-path ./my-glossary.json
```

| Flag | Default | Description |
|------|---------|-------------|
| `--glossary-path` | `glossary.json` | Path to glossary file for consistent terminology |

## Claude Code Skill

There's a [Claude Code skill](skills/xcstrings-mcp/SKILL.md) shipped with the project that teaches Claude how to drive all 28 tools well. It activates automatically on localization-related requests.

What it actually does for you:

- Stops Claude from reading raw `.xcstrings` files (which would just dump tens of thousands of tokens into the context for no benefit)
- Picks the right tool sequence per workflow (translate, migrate, audit, export, and catalog merge/conflict resolution)
- Handles CLDR plural categories per locale (Ukrainian wants `one/few/many`, Japanese only wants `other`)
- Keeps glossary terms consistent across translations
- Spawns one subagent per language for parallel multi-locale work

Install:

```sh
mkdir -p ~/.claude/skills/xcstrings-mcp && curl -sL \
  https://raw.githubusercontent.com/Murzav/xcstrings-mcp/main/skills/xcstrings-mcp/SKILL.md \
  -o ~/.claude/skills/xcstrings-mcp/SKILL.md
```

Or clone and copy:
```sh
cp -r skills/xcstrings-mcp ~/.claude/skills/
```

The skill covers full translation, language management, coverage audits, legacy migration, simple-string XLIFF roundtrips, catalog merge/conflict resolution, plural handling, and glossary work.

## Performance

Each platform release is a ~2.1–2.4 MB `.tar.gz` containing a ~4.5 MB binary (stripped, LTO). The server is event-driven on stdio, so it doesn't tick when no requests are in flight.

| File | Parse | Get untranslated | Validate | RAM |
|------|-------|-----------------|----------|-----|
| 968 KB (638 keys × 10 locales) | 0.2 ms | 0.02 ms | 0.04 ms | 7.6 MB |
| 4.1 MB (2K keys × 10 locales) | 24 ms | 5 ms | 7 ms | 40 MB |
| 10.3 MB (5K keys × 10 locales) | 60 ms | 11 ms | 23 ms | 49 MB |
| 56.7 MB (10K keys × 30 locales) | 333 ms | 62 ms | 221 ms | 287 MB |

Scaling is linear in keys × locales. A typical iOS project (2–5K keys) parses in well under 60 ms.

## Architecture

```
┌─────────────┐    stdio/JSON-RPC   ┌──────────────────┐    File I/O    ┌───────────────────────┐
│ Claude Code │◄───────────────────►│  xcstrings-mcp   │◄──────────────►│ Localizable.xcstrings │
│ (translates)│                     │ (Rust MCP server)│                │ (JSON on disk)        │
└─────────────┘                     └──────────────────┘                └───────────────────────┘
```

Plain layered architecture: `server` → `tools` → `service` → `model`, with `io` injected through the `FileStore` trait.

- `server` — MCP routing and handler dispatch
- `tools` — tool implementations, grouped by area (parse/extract/keys/translate/manage/...)
- `service` — the actual logic (parser, extractor, merger, validator, formatter), no filesystem access
- `model` — serde types for the `.xcstrings` format, CLDR plural rules, and format specifiers
- `io` — `FileStore` trait and the real filesystem implementation with stable advisory locking, exact-byte conditional writes, and atomic replacement
- `cli` — the standalone subcommands; `prompts.rs` defines the MCP prompts; `error.rs` is the single project-wide error enum

## Related

- [Model Context Protocol](https://modelcontextprotocol.io) — open protocol for AI tool integration
- [Xcode String Catalogs](https://developer.apple.com/documentation/xcode/localizing-and-varying-text-with-a-string-catalog) — Apple's localization format
- [CLDR Plural Rules](https://cldr.unicode.org/index/cldr-spec/plural-rules) — Unicode plural categories used for validation

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines.
