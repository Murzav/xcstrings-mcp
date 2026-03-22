# 🌐 xcstrings-mcp

**MCP server for iOS/macOS .xcstrings (String Catalog) localization — parse, translate, validate, and export with any AI coding assistant**

[![CI](https://github.com/Murzav/xcstrings-mcp/actions/workflows/ci.yml/badge.svg)](https://github.com/Murzav/xcstrings-mcp/actions/workflows/ci.yml)
![Coverage](https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/Murzav/b7cd209fd268df81711e04622ff051e8/raw/coverage.json)
[![Crates.io](https://img.shields.io/crates/v/xcstrings-mcp)](https://crates.io/crates/xcstrings-mcp)
[![Downloads](https://img.shields.io/crates/d/xcstrings-mcp)](https://crates.io/crates/xcstrings-mcp)
[![MSRV](https://img.shields.io/badge/MSRV-1.88-blue)](https://blog.rust-lang.org/)
[![License](https://img.shields.io/crates/l/xcstrings-mcp)](LICENSE-MIT)
[![MCP](https://img.shields.io/badge/MCP-compatible-green)](https://modelcontextprotocol.io)

## Why xcstrings-mcp?

Xcode String Catalogs (`.xcstrings`) are large JSON files that waste LLM context windows when loaded whole. Manual editing risks corrupting Xcode's specific formatting, there's no validation for format specifiers or plural rules, and plural-aware translation requires understanding CLDR categories across 40+ locales.

xcstrings-mcp solves this by providing structured MCP tools that let AI assistants work with `.xcstrings` files efficiently — reading only what's needed, validating every write, and preserving Xcode's exact JSON formatting.

## Features

- **26 tools + 8 prompts** covering the full localization lifecycle
- **Batch translation** with context window management — process 50-100 keys at a time
- **Format specifier & CLDR plural validation** — catch `%d`/`%@` mismatches and missing plural forms before they ship
- **Atomic writes** preserving Xcode's exact JSON formatting (`" : "` spacing, key order, BOM handling)
- **Legacy migration** — import `.strings` and `.stringsdict` files (UTF-8/UTF-16, plural rules, positional specifiers)
- **XLIFF 1.2 export/import** — integrate with external translation tools and agencies
- **Glossary** for consistent terminology across locales
- **Works with** Claude Code, Cursor, VS Code + Copilot, Windsurf, Zed, OpenAI Codex, and any MCP client

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

Typical workflow:

1. **Parse** the `.xcstrings` file to cache it
2. **Get untranslated** strings in batches that fit the context window
3. **Submit translations** with automatic validation and atomic writes

```
parse_xcstrings → get_untranslated → submit_translations
```

Multi-file projects: parse each file — the server caches all of them and tracks the active one. Use `list_files` to see cached files.

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

### Migrating from Legacy .strings

To migrate an existing project using `.strings`/`.stringsdict` files:

```
discover_files → import_strings → get_untranslated → submit_translations
```

Preview first with `dry_run`, then write:
```
import_strings(directory: "./Resources", source_language: "en", output_path: "./Localizable.xcstrings", dry_run: true)
import_strings(directory: "./Resources", source_language: "en", output_path: "./Localizable.xcstrings")
```

For projects with `.stringsdict` plural rules, also check plural keys after import:
```
import_strings → get_plurals → get_untranslated → submit_translations
```

Supports UTF-8 and UTF-16 encodings, `.stringsdict` plural rules (single and multi-variable with positional specifiers), developer comments, unquoted keys, and merge into existing `.xcstrings` files.

### Getting Started from Scratch

To create a new localization file and begin translating:

```
create_xcstrings → add_keys → add_locale → get_untranslated → submit_translations
```

Or use the `extract_strings` prompt to automatically extract hardcoded strings from your Swift source code.

### CLI Options

```sh
xcstrings-mcp --glossary-path ./my-glossary.json
```

| Flag | Default | Description |
|------|---------|-------------|
| `--glossary-path` | `glossary.json` | Path to glossary file for consistent terminology |

## Claude Code Skill

xcstrings-mcp ships with a [Claude Code skill](skills/xcstrings-mcp/SKILL.md) that teaches Claude Code the optimal workflows, best practices, and error handling for all 26 tools. The skill auto-triggers on any localization-related request.

**What it does:**
- Prevents Claude from reading `.xcstrings` files directly (wastes context, risks corruption)
- Guides optimal tool sequences for every workflow (translate, migrate, audit, export)
- Handles CLDR plural categories per locale (uk needs one/few/many, ja needs only other)
- Manages glossary for consistent terminology across translations
- Parallelizes multi-locale translation with one subagent per language

**Install the skill:**

```sh
# One-liner
mkdir -p ~/.claude/skills/xcstrings-mcp && curl -sL \
  https://raw.githubusercontent.com/Murzav/xcstrings-mcp/main/skills/xcstrings-mcp/SKILL.md \
  -o ~/.claude/skills/xcstrings-mcp/SKILL.md
```

Or clone the repo and copy:
```sh
cp -r skills/xcstrings-mcp ~/.claude/skills/
```

The skill covers these workflows out of the box:
- **Full translation** — parallel subagents per locale with batch processing
- **Add/remove language** — locale management with coverage verification
- **Coverage audit** — coverage, validation, stale keys, glossary check
- **Legacy migration** — `.strings`/`.stringsdict` to `.xcstrings` with dry-run preview
- **XLIFF roundtrip** — export for translators, import back with validation
- **Plural handling** — CLDR-aware plural form management
- **Glossary management** — consistent terminology across locales

## Performance

Binary size: **~4 MB** (stripped + LTO). Zero CPU at idle.

| File | Parse | Get untranslated | Validate | RAM |
|------|-------|-----------------|----------|-----|
| 968KB (638 keys × 10 loc) | 0.2ms | 0.02ms | 0.04ms | 7.6 MB |
| 4.1MB (2K keys × 10 loc) | 24ms | 5ms | 7ms | 40 MB |
| 10.3MB (5K keys × 10 loc) | 60ms | 11ms | 23ms | 49 MB |
| 56.7MB (10K keys × 30 loc) | 333ms | 62ms | 221ms | 287 MB |

Scaling is linear — no degradation cliffs. Typical iOS projects (2-5K keys) parse in under 60ms.

## Architecture

```
┌─────────────┐    stdio/JSON-RPC   ┌──────────────────┐    File I/O    ┌───────────────────────┐
│ Claude Code │◄───────────────────►│  xcstrings-mcp   │◄──────────────►│ Localizable.xcstrings │
│ (translates)│                     │ (Rust MCP server)│                │ (JSON on disk)        │
└─────────────┘                     └──────────────────┘                └───────────────────────┘
```

Layered architecture: `server` -> `tools` -> `service` -> `model`, with `io` injected via the `FileStore` trait.

- **server** -- MCP tool routing and handler dispatch
- **tools** -- individual tool implementations
- **service** -- pure logic (parser, extractor, merger, validator, formatter); no I/O
- **model** -- serde types for `.xcstrings` format, CLDR plural rules, format specifiers
- **io** -- `FileStore` trait + real filesystem implementation with atomic writes

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
