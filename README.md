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

- 27 MCP tools, 8 prompts, and 11 CLI commands for the full translation lifecycle
- Batch translation that fits the context window: pull 50–100 keys at a time
- Format specifier and CLDR plural validation, so `%d`/`%@` mismatches and missing plural forms get caught before they ship
- Atomic writes that match Xcode's JSON formatting exactly (`" : "` colon spacing, preserved key order, BOM stripped on read and never re-emitted)
- Legacy migration from `.strings` and `.stringsdict` (UTF-8/UTF-16, plural rules, positional specifiers)
- XLIFF 1.2 import/export for handing files off to external translators
- Glossary support so terminology stays consistent across locales
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

## Usage

The basic loop:

1. Parse the `.xcstrings` once to cache it.
2. Pull untranslated strings in batches.
3. Submit translations. The server validates and writes atomically.

```
parse_xcstrings → get_untranslated → submit_translations
```

For projects with multiple `.xcstrings` files, parse each one. The server keeps them all in memory and tracks which is "active". `list_files` shows what's loaded.

## Tools

| Tool | Description |
|------|-------------|
| `parse_xcstrings` | Parse and cache `.xcstrings` file |
| `get_untranslated` | Get untranslated strings with batching |
| `submit_translations` | Validate and write translations atomically |
| `get_coverage` | Per-locale coverage statistics |
| `get_stale` | Find stale/removed keys |
| `validate_translations` | File-wide validation report |
| `list_locales` | List locales with stats |
| `add_locale` | Add new locale with empty translations |
| `remove_locale` | Remove a locale from all entries |
| `get_plurals` | Extract keys needing plural translation |
| `get_context` | Find related keys by shared prefix |
| `list_files` | List all cached files with active status |
| `get_diff` | Compare cached vs on-disk file (added/removed/modified keys) |
| `get_glossary` | Get translation glossary entries for a locale pair |
| `update_glossary` | Add or update glossary terms |
| `export_xliff` | Export to XLIFF 1.2 for external translation tools |
| `import_xliff` | Import translations from XLIFF 1.2 file |
| `import_strings` | Migrate legacy `.strings`/`.stringsdict` files to `.xcstrings` |
| `search_keys` | Search keys by substring (matches key names and source text) |
| `create_xcstrings` | Create a new empty .xcstrings file |
| `add_keys` | Add new localization keys with source text |
| `discover_files` | Find .xcstrings and legacy .strings/.stringsdict files |
| `update_comments` | Update developer comments on localization keys |
| `delete_keys` | Delete localization keys and all their translations |
| `delete_translations` | Remove translations for specific keys in a locale, resetting to untranslated |
| `get_key` | Get all translations for a specific key across all locales |
| `rename_key` | Rename a localization key, preserving all translations |

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
| `validate` | Check format specifiers, plurals, empty values |
| `search <pattern>` | Find keys by substring |
| `stale` | List stale/removed keys |
| `add-locale <locale>` | Add a new locale |
| `remove-locale <locale>` | Remove a locale |
| `export` | Export to XLIFF 1.2 |
| `import` | Import from XLIFF with validation |
| `migrate` | Migrate legacy .strings/.stringsdict |
| `completions <shell>` | Generate shell completions |

`--json` is available everywhere for machine-readable output. Mutating commands support `--dry-run`.

### CLI Options

```sh
xcstrings-mcp --glossary-path ./my-glossary.json
```

| Flag | Default | Description |
|------|---------|-------------|
| `--glossary-path` | `glossary.json` | Path to glossary file for consistent terminology |

## Claude Code Skill

There's a [Claude Code skill](skills/xcstrings-mcp/SKILL.md) shipped with the project that teaches Claude how to drive all 27 tools well. It activates automatically on localization-related requests.

What it actually does for you:

- Stops Claude from reading raw `.xcstrings` files (which would just dump tens of thousands of tokens into the context for no benefit)
- Picks the right tool sequence per workflow (translate, migrate, audit, export)
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

The skill covers full translation, language management, coverage audits, legacy migration, XLIFF roundtrips, plural handling, and glossary work.

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
- `io` — `FileStore` trait and the real filesystem implementation with atomic writes
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
