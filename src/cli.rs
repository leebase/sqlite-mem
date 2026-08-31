//! Clap CLI surface. `save`, `info`, and `ask` were built in S2/S3;
//! `forget` and `reindex` are S4 (architecture.md §15, §19).

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
    /// Mark memories forgotten (or --purge/--restore them).
    Forget(ForgetArgs),
    /// Re-embed every chunk with this binary's current embedder.
    Reindex(ReindexArgs),
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

    /// Run integrity/consistency checks (PRAGMA integrity_check, FTS
    /// backfill audit, embedding-dims audit, content_hash spot-check).
    /// Any failed check exits 7.
    #[arg(long)]
    pub verify: bool,
}

#[derive(Args, Debug)]
pub struct ForgetArgs {
    /// Database file path (overrides SQLITE_MEM_DB and the default path).
    #[arg(long)]
    pub db: Option<String>,

    /// Hard-delete instead of soft-delete: memory, chunks, FTS rows, and
    /// metadata are all removed in one transaction. Destructive; mutually
    /// exclusive with --restore.
    #[arg(long)]
    pub purge: bool,

    /// Return previously-forgotten memories to their prior status instead
    /// of forgetting them. Mutually exclusive with --purge.
    #[arg(long)]
    pub restore: bool,

    /// One or more memory IDs.
    #[arg(required = true, num_args = 1..)]
    pub ids: Vec<String>,
}

#[derive(Args, Debug)]
pub struct ReindexArgs {
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
