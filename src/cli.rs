//! Clap CLI surface. `save`, `info`, and `ask` exist as of Sprint S3
//! (`forget`/`reindex` are later-sprint scope -- invoking them today
//! is an unrecognized-subcommand usage error from clap itself, exit 2).

use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "sqlite-mem",
    version,
    about = "Fully offline, single-file AI memory"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Save a distilled memory.
    Save(SaveArgs),
    /// Report database/schema/embedder status.
    Info(InfoArgs),
    /// Retrieve memories relevant to a query via hybrid FTS5+vector search.
    Ask(AskArgs),
}

#[derive(Args, Debug)]
pub struct SaveArgs {
    /// Database file path (overrides SQLITE_MEM_DB and the default path).
    #[arg(long)]
    pub db: Option<String>,

    /// Caller metadata as KEY=VALUE; may be repeated.
    #[arg(long = "meta", value_name = "KEY=VALUE")]
    pub meta: Vec<String>,

    /// Caller-supplied provenance string.
    #[arg(long)]
    pub source: Option<String>,

    /// Memory ID(s) this save supersedes; may be repeated.
    #[arg(long = "supersedes", value_name = "ID")]
    pub supersedes: Vec<String>,

    /// Fail instead of returning a deduplicated result when identical
    /// active content already exists.
    #[arg(long = "if-new")]
    pub if_new: bool,

    /// Content to save, given inline.
    #[arg(long)]
    pub content: Option<String>,

    /// Read content from stdin instead of --content.
    #[arg(long)]
    pub stdin: bool,
}

#[derive(Args, Debug)]
pub struct InfoArgs {
    /// Database file path (overrides SQLITE_MEM_DB and the default path).
    #[arg(long)]
    pub db: Option<String>,
}

#[derive(Args, Debug)]
pub struct AskArgs {
    /// Database file path (overrides SQLITE_MEM_DB and the default path).
    #[arg(long)]
    pub db: Option<String>,

    /// Maximum number of results to return.
    #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u32).range(1..=50))]
    pub k: u32,

    /// Metadata filter: KEY=VALUE (equality), KEY!=VALUE (exclusion), or
    /// KEY=* (existence); may be repeated, terms are ANDed.
    #[arg(long = "where", value_name = "KEY=VALUE|KEY!=VALUE|KEY=*")]
    pub where_: Vec<String>,

    /// Include memories with status=superseded (excluded by default).
    #[arg(long = "include-superseded")]
    pub include_superseded: bool,

    /// Include memories with status=forgotten (excluded by default).
    #[arg(long = "include-forgotten")]
    pub include_forgotten: bool,

    /// Retrieval mode.
    #[arg(long, value_enum, default_value = "hybrid")]
    pub mode: crate::ask::Mode,

    /// Drop fused results scoring below this threshold (applied after RRF
    /// fusion, before truncating to --k).
    #[arg(long = "min-score")]
    pub min_score: Option<f64>,

    /// Query text, given inline.
    #[arg(long)]
    pub query: Option<String>,

    /// Read query text from stdin instead of --query.
    #[arg(long)]
    pub stdin: bool,
}
