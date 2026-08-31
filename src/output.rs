//! The one place this crate writes to stdout.
//!
//! Pattern lifted from sqlite-graphrag's `src/output/` (MIT OR Apache-2.0,
//! danilo-aguiar-br/sqlite-graphrag; see THIRD-PARTY.md): a single sink
//! function, `BrokenPipe` treated as success (a caller piping into `head`
//! closes the read end early -- that is not a failure), and stdout carries
//! exactly one JSON document per invocation (architecture.md §17, invariant
//! §24.5). Every other module must go through `emit`/`emit_ok`/`emit_err`
//! instead of calling `println!` directly -- enforced by the
//! `no_stray_println` integration test.

use crate::error::AppError;
use serde::Serialize;
use std::io::Write;

/// Writes `bytes` followed by a newline, then flushes stdout.
///
/// A `BrokenPipe` is silenced (see module docs); any other I/O failure is
/// swallowed too -- by the time we are writing the *only* output document
/// of the process, there is nowhere left to report a secondary failure.
fn write_line(bytes: &[u8]) {
    let mut out = std::io::stdout().lock();
    let _ = out
        .write_all(bytes)
        .and_then(|()| out.write_all(b"\n"))
        .and_then(|()| out.flush());
}

/// Emits `value` as-is: the one sink for a response struct that already
/// decides its own `ok`/`error` fields, such as `info --verify`'s envelope
/// (architecture.md §18, amended: a failed verify carries `ok:false` +
/// `error` alongside its `checks` detail, which doesn't fit the plain
/// `AppError` -> `emit_err` path). `emit_ok` is the common case built on
/// top of this for verbs whose response is always a plain success.
pub fn emit<T: Serialize>(value: &T) {
    match serde_json::to_string(value) {
        Ok(s) => write_line(s.as_bytes()),
        Err(e) => {
            // Serialization of our own response struct should never fail;
            // if it somehow does, still emit exactly one parseable JSON
            // document rather than nothing.
            write_line(
                format!(
                    r#"{{"ok":false,"error":{{"code":"internal_serialize_error","message":"{}"}}}}"#,
                    e.to_string().replace('\\', "\\\\").replace('"', "\\\"")
                )
                .as_bytes(),
            );
        }
    }
}

/// Emits a successful envelope: `value` must serialize with `"ok": true`
/// already present (each verb's response struct carries it).
pub fn emit_ok<T: Serialize>(value: &T) {
    emit(value)
}

#[derive(Serialize)]
struct ErrorField<'a> {
    code: &'a str,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<&'a str>,
}

#[derive(Serialize)]
struct ErrorEnvelope<'a> {
    ok: bool,
    error: ErrorField<'a>,
}

/// Emits the `{"ok":false,"error":{code,message,hint}}` envelope per
/// architecture.md §17. Returns the process exit code the caller should
/// use (`main` is the only place that actually calls `process::exit`).
pub fn emit_err(err: &AppError) -> i32 {
    let envelope = ErrorEnvelope {
        ok: false,
        error: ErrorField {
            code: err.code,
            message: &err.message,
            hint: err.hint.as_deref(),
        },
    };
    match serde_json::to_string(&envelope) {
        Ok(s) => write_line(s.as_bytes()),
        Err(_) => {
            // Hand-rolled fallback so a serializer bug never means empty
            // stdout on an error path (sqlite-graphrag pattern).
            let escape = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
            let mut line = format!(
                r#"{{"ok":false,"error":{{"code":"{}","message":"{}""#,
                escape(err.code),
                escape(&err.message)
            );
            if let Some(hint) = &err.hint {
                line.push_str(&format!(r#","hint":"{}""#, escape(hint)));
            }
            line.push_str("}}");
            write_line(line.as_bytes());
        }
    }
    err.exit.code()
}
