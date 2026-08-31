//! Clap CLI surface. Only `save` and `info` exist in Sprint S2
//! (`ask`/`forget`/`reindex` are later-sprint scope -- invoking them today
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
