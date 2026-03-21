# xcstrings-mcp

[![CI](https://github.com/Murzav/xcstrings-mcp/actions/workflows/ci.yml/badge.svg)](https://github.com/Murzav/xcstrings-mcp/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/xcstrings-mcp)](https://crates.io/crates/xcstrings-mcp)
[![License](https://img.shields.io/crates/l/xcstrings-mcp)](LICENSE-MIT)

MCP server for iOS/macOS `.xcstrings` localization file management.

## The Problem

Xcode String Catalogs (`.xcstrings`) are large JSON files that waste LLM context windows when loaded whole. Manual editing risks corrupting Xcode's specific formatting, there's no validation for format specifiers or plural rules, and plural-aware translation requires understanding CLDR categories across 40+ locales.

## Quick Start

### Install

```sh
brew install Murzav/tap/xcstrings-mcp
# or
cargo install xcstrings-mcp
# or
cargo binstall xcstrings-mcp
```

### Configure Claude Code

```sh
claude mcp add xcstrings-mcp -- xcstrings-mcp
```

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
| `search_keys` | Search keys by substring (matches key names and source text) |

### Prompts

| Prompt | Description |
|--------|-------------|
| `translate_batch` | Step-by-step instructions for batch translation |
| `review_translations` | Instructions for quality review of translations |
| `full_translate` | Complete workflow for translating an entire file |
| `localization_audit` | Full audit: coverage, validation, stale keys, glossary |
| `fix_validation_errors` | Guided workflow to fix all validation issues |
| `add_language` | Add a new locale and translate all strings |

### CLI Options

```sh
xcstrings-mcp --glossary-path ./my-glossary.json
```

| Flag | Default | Description |
|------|---------|-------------|
| `--glossary-path` | `glossary.json` | Path to glossary file for consistent terminology |

## Performance

Binary size: **3.6 MB** (stripped + LTO). Zero CPU at idle.

| File | Parse | Get untranslated | Validate | RAM |
|------|-------|-----------------|----------|-----|
| 968KB (638 keys × 10 loc) | 0.2ms | 0.02ms | 0.04ms | 7.6 MB |
| 4.1MB (2K keys × 10 loc) | 24ms | 5ms | 7ms | 40 MB |
| 10.3MB (5K keys × 10 loc) | 60ms | 11ms | 23ms | 49 MB |
| 56.7MB (10K keys × 30 loc) | 333ms | 62ms | 221ms | 287 MB |

Scaling is linear — no degradation cliffs. Typical iOS projects (2-5K keys) parse in under 60ms.

## Architecture

```
┌─────────────┐    stdio/JSON-RPC    ┌─────────────────┐    File I/O    ┌──────────────────────┐
│ Claude Code  │◄───────────────────►│  xcstrings-mcp   │◄────────────►│ Localizable.xcstrings │
│ (translates) │                     │ (Rust MCP server)│              │ (JSON on disk)        │
└─────────────┘                     └─────────────────┘              └──────────────────────┘
```

Layered architecture: `server` -> `tools` -> `service` -> `model`, with `io` injected via the `FileStore` trait.

- **server** -- MCP tool routing and handler dispatch
- **tools** -- individual tool implementations
- **service** -- pure logic (parser, extractor, merger, validator, formatter); no I/O
- **model** -- serde types for `.xcstrings` format, CLDR plural rules, format specifiers
- **io** -- `FileStore` trait + real filesystem implementation with atomic writes

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines.
