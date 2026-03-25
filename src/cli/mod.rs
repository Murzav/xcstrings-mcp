mod common;
mod completions;
mod coverage;
mod export_cmd;
mod import_cmd;
mod info;
mod locale;
mod migrate;
mod search;
mod stale;
mod validate;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Subcommand;

#[derive(Subcommand)]
pub enum Command {
    /// Show file summary (source language, keys, locales)
    Info {
        /// Path to .xcstrings file (auto-discovered if omitted)
        file: Option<PathBuf>,
    },
    /// Show translation coverage per locale
    Coverage {
        /// Path to .xcstrings file (auto-discovered if omitted)
        file: Option<PathBuf>,
        /// Filter to a specific locale
        #[arg(long)]
        locale: Option<String>,
    },
    /// Validate translations (format specifiers, plurals)
    Validate {
        /// Path to .xcstrings file (auto-discovered if omitted)
        file: Option<PathBuf>,
        /// Validate only a specific locale
        #[arg(long)]
        locale: Option<String>,
    },
    /// Search keys by pattern
    Search {
        /// Substring pattern to match against key names and source text
        pattern: String,
        /// Path to .xcstrings file (auto-discovered if omitted)
        file: Option<PathBuf>,
        /// Filter results to a specific locale
        #[arg(long)]
        locale: Option<String>,
        /// Maximum number of results
        #[arg(long, default_value = "50")]
        limit: usize,
    },
    /// Find stale/removed keys
    Stale {
        /// Path to .xcstrings file (auto-discovered if omitted)
        file: Option<PathBuf>,
        /// Locale to check for stale keys (defaults to source language)
        #[arg(long)]
        locale: Option<String>,
        /// Maximum number of results
        #[arg(long, default_value = "100")]
        limit: usize,
    },
    /// Add a new locale
    AddLocale {
        /// BCP 47 locale code (e.g. fr, de, ja)
        locale: String,
        /// Path to .xcstrings file (auto-discovered if omitted)
        file: Option<PathBuf>,
        /// Preview changes without writing
        #[arg(long)]
        dry_run: bool,
    },
    /// Remove a locale
    RemoveLocale {
        /// BCP 47 locale code to remove
        locale: String,
        /// Path to .xcstrings file (auto-discovered if omitted)
        file: Option<PathBuf>,
        /// Preview changes without writing
        #[arg(long)]
        dry_run: bool,
    },
    /// Export translations to XLIFF
    Export {
        /// Path to .xcstrings file (auto-discovered if omitted)
        file: Option<PathBuf>,
        /// Locale to export
        #[arg(long)]
        locale: String,
        /// Output file path
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Export all strings (including already translated)
        #[arg(long)]
        all: bool,
    },
    /// Import translations from XLIFF
    Import {
        /// Path to .xcstrings file (auto-discovered if omitted)
        file: Option<PathBuf>,
        /// Path to XLIFF file to import
        #[arg(long)]
        xliff: PathBuf,
        /// Preview changes without writing
        #[arg(long)]
        dry_run: bool,
    },
    /// Migrate legacy .strings/.stringsdict files
    Migrate {
        /// Source language code (e.g. en)
        #[arg(long)]
        source_language: String,
        /// Output .xcstrings file path
        #[arg(short, long)]
        output: PathBuf,
        /// Directory to scan for .lproj folders
        #[arg(long, conflicts_with = "files")]
        directory: Option<PathBuf>,
        /// Explicit list of .strings/.stringsdict files
        #[arg(long, conflicts_with = "directory", num_args = 1..)]
        files: Vec<PathBuf>,
        /// Preview changes without writing
        #[arg(long)]
        dry_run: bool,
    },
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for (bash, zsh, fish, elvish, powershell)
        shell: clap_complete::Shell,
    },
}

pub fn run(cmd: Command, json: bool) -> ExitCode {
    match cmd {
        Command::Info { file } => info::run(file, json),
        Command::Coverage { file, locale } => coverage::run(file, locale, json),
        Command::Validate { file, locale } => validate::run(file, locale, json),
        Command::Search {
            pattern,
            file,
            locale,
            limit,
        } => search::run(pattern, file, locale, limit, json),
        Command::Stale {
            file,
            locale,
            limit,
        } => stale::run(file, locale, limit, json),
        Command::AddLocale {
            locale,
            file,
            dry_run,
        } => locale::run_add(locale, file, dry_run, json),
        Command::RemoveLocale {
            locale,
            file,
            dry_run,
        } => locale::run_remove(locale, file, dry_run, json),
        Command::Export {
            file,
            locale,
            output,
            all,
        } => export_cmd::run(file, locale, output, all, json),
        Command::Import {
            file,
            xliff,
            dry_run,
        } => import_cmd::run(file, xliff, dry_run, json),
        Command::Migrate {
            source_language,
            output,
            directory,
            files,
            dry_run,
        } => migrate::run(source_language, output, directory, files, dry_run, json),
        Command::Completions { shell } => completions::run_shell(shell),
    }
}
