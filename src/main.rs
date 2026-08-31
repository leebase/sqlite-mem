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
