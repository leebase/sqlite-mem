mod ask;
mod chunk;
mod cli;
mod db;
mod embed;
mod error;
mod filter;
mod forget;
mod info;
mod output;
mod paths;
mod rank;
mod reindex;
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

    // Every verb returns the exit code it wants on success (0 for all of
    // them except `info --verify`, which reports 7 when any check failed
    // -- see `run_info`); an `Err` always routes through `emit_err`, which
    // owns exit-code selection for every failure path (architecture.md
    // §17, error.rs's module doc).
    let result: Result<i32, AppError> = match cli.command {
        cli::Command::Save(args) => run_save(args).map(|()| 0),
        cli::Command::Info(args) => run_info(args),
        cli::Command::Ask(args) => run_ask(args).map(|()| 0),
        cli::Command::Forget(args) => run_forget(args).map(|()| 0),
        cli::Command::Reindex(args) => run_reindex(args).map(|()| 0),
    };

    match result {
        Ok(code) => code,
        Err(e) => output::emit_err(&e),
    }
}

/// Reads stdin bounded to `cap + 1` bytes (S6 audit F5): an unbounded
/// `read_to_string` over the whole pipe let a 1 GiB stdin balloon into a
/// ~1 GB buffer before the verb's own oversized-content rejection ever got
/// a chance to run. Reading only `cap + 1` bytes is enough for that
/// rejection to still fire correctly -- if the input is at most `cap`
/// bytes, `take` reaches EOF first and this returns the whole thing
/// unchanged; if it's longer, the read stops at `cap + 1` bytes (strictly
/// over the cap even after trimming, since the caller trims at most a
/// handful of edge whitespace bytes) and the rest of the stream is never
/// buffered at all.
///
/// Invalid UTF-8 within an in-cap prefix is a validation failure, not a
/// usage error (S6 audit F10; architecture.md §17 treats malformed
/// *content*, not malformed *invocation*, as exit 3). The size check runs
/// on raw bytes BEFORE UTF-8 decoding so an over-cap stream that splits a
/// multibyte character reports `input_too_large`, not `invalid_utf8`
/// (S6 re-audit LOW-R1); other stdin I/O failures stay usage errors.
fn read_stdin_bounded(cap: usize) -> Result<String, AppError> {
    // Read bytes first so an over-cap stream that happens to split a
    // multibyte character reports input_too_large, not invalid_utf8
    // (see S6 re-audit LOW-R1). The size check downstream still owns
    // the exact cap message; here we only need at most cap+1 bytes.
    let mut buf = Vec::new();
    std::io::stdin()
        .take(cap as u64 + 1)
        .read_to_end(&mut buf)
        .map_err(|e| AppError::usage(format!("failed to read stdin: {e}")))?;
    if buf.len() > cap {
        return Err(AppError::validation(
            "input_too_large",
            format!("stdin exceeds the {cap}-byte limit"),
        ));
    }
    String::from_utf8(buf)
        .map_err(|e| AppError::validation("invalid_utf8", format!("stdin is not valid UTF-8: {e}")))
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
        (None, true) => read_stdin_bounded(save::MAX_CONTENT_BYTES)?,
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

fn run_info(args: cli::InfoArgs) -> Result<i32, AppError> {
    let db_path = paths::resolve_db_path(args.db.as_deref())?;
    if args.verify {
        let (response, passed) = info::run_verify(&db_path)?;
        // Not the AppError/emit_err path: the envelope's own ok/error
        // fields already agree with the exit code below (architecture.md
        // §18, amended -- "every non-zero exit pairs with ok:false"), so
        // this goes through the neutral `emit` sink rather than
        // `emit_ok` (which documents an ok:true-only contract).
        output::emit(&response);
        return Ok(if passed { 0 } else { 7 });
    }
    let response = info::run(&db_path)?;
    output::emit_ok(&response);
    Ok(0)
}

fn run_forget(args: cli::ForgetArgs) -> Result<(), AppError> {
    if args.purge && args.restore {
        return Err(AppError::usage(
            "--purge and --restore are mutually exclusive",
        ));
    }
    let mode = if args.purge {
        forget::ForgetMode::Purge
    } else if args.restore {
        forget::ForgetMode::Restore
    } else {
        forget::ForgetMode::Forget
    };

    let db_path = paths::resolve_db_path(args.db.as_deref())?;
    let response = forget::run(
        &db_path,
        forget::ForgetInput {
            ids: args.ids,
            mode,
        },
    )?;
    output::emit_ok(&response);
    Ok(())
}

fn run_reindex(args: cli::ReindexArgs) -> Result<(), AppError> {
    let db_path = paths::resolve_db_path(args.db.as_deref())?;
    let response = reindex::run(&db_path)?;
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
        (None, true) => read_stdin_bounded(ask::MAX_QUERY_BYTES)?,
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
