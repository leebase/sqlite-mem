mod ask;
mod chunk;
mod cli;
mod db;
mod embed;
mod error;
mod filter;
mod info;
mod output;
mod paths;
mod rank;
mod save;
mod vector;

use clap::Parser;
use error::AppError;
use std::io::Read;

fn main() {
    // Diagnostics only, stderr, never stdout (architecture.md §17, §24.5).
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let exit = run();
    std::process::exit(exit);
}

fn run() -> i32 {
    let cli = match cli::Cli::try_parse() {
        Ok(cli) => cli,
        Err(clap_err) => {
            // clap's own exit codes line up with ours (2 for usage errors,
            // 0 for --help/--version), but its default reporting goes to
            // stdout/stderr as plain text, not our JSON envelope. Route
            // everything through emit_err so stdout always carries exactly
            // one JSON document (architecture.md §17).
            if clap_err.exit_code() == 0 {
                // --help / --version: clap's own text is the correct,
                // expected output for these; let it print and exit 0.
                clap_err.print().ok();
                return 0;
            }
            let err = AppError::usage(clap_err.to_string());
            return output::emit_err(&err);
        }
    };

    let result = match cli.command {
        cli::Command::Save(args) => run_save(args),
        cli::Command::Info(args) => run_info(args),
        cli::Command::Ask(args) => run_ask(args),
    };

    match result {
        Ok(()) => 0,
        Err(e) => output::emit_err(&e),
    }
}

fn run_save(args: cli::SaveArgs) -> Result<(), AppError> {
    let content = match (&args.content, args.stdin) {
        (Some(_), true) => {
            return Err(AppError::usage(
                "--content and --stdin are mutually exclusive",
            ));
        }
        (None, false) => {
            return Err(AppError::usage("one of --content or --stdin is required"));
        }
        (Some(c), false) => c.clone(),
        (None, true) => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| AppError::usage(format!("failed to read stdin: {e}")))?;
            buf
        }
    };

    let meta = save::parse_and_validate_meta(&args.meta)?;
    let db_path = paths::resolve_db_path(args.db.as_deref())?;

    let input = save::SaveInput {
        content,
        source: args.source,
        meta,
        supersedes: args.supersedes,
        if_new: args.if_new,
    };

    let response = save::run(&db_path, input)?;
    output::emit_ok(&response);
    Ok(())
}

fn run_info(args: cli::InfoArgs) -> Result<(), AppError> {
    let db_path = paths::resolve_db_path(args.db.as_deref())?;
    let response = info::run(&db_path)?;
    output::emit_ok(&response);
    Ok(())
}

fn run_ask(args: cli::AskArgs) -> Result<(), AppError> {
    let query = match (&args.query, args.stdin) {
        (Some(_), true) => {
            return Err(AppError::usage(
                "--query and --stdin are mutually exclusive",
            ));
        }
        (None, false) => {
            return Err(AppError::usage("one of --query or --stdin is required"));
        }
        (Some(q), false) => q.clone(),
        (None, true) => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| AppError::usage(format!("failed to read stdin: {e}")))?;
            buf
        }
    };

    let where_terms = filter::parse_where_terms(&args.where_)?;
    let db_path = paths::resolve_db_path(args.db.as_deref())?;

    let input = ask::AskInput {
        query,
        k: args.k,
        where_terms,
        include_superseded: args.include_superseded,
        include_forgotten: args.include_forgotten,
        mode: args.mode,
        min_score: args.min_score,
    };

    let response = ask::run(&db_path, input)?;
    output::emit_ok(&response);
    Ok(())
}
