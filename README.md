# sqlite-mem

[![CI](https://github.com/leebase/sqlite-mem/actions/workflows/ci.yml/badge.svg)](https://github.com/leebase/sqlite-mem/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org)

**A single offline binary that gives any AI harness durable, retrievable
memory in one user-owned SQLite file.**

No server. No daemon. No cloud provider. No API key. No Python environment.
No first-run download. The embedding model is compiled into the executable
itself — drop the binary anywhere and it works, forever, with no network
connection ever required or attempted. That's the whole product.

```console
$ sqlite-mem save --content "We rejected Mastra because suspend/resume durability violated the Factory invariants." --meta project=factory --meta kind=decision --source "decisions.md#D012"
{"ok":true,"op":"save","id":"01M1DC2Z17JDEG0AAXDA5ZFYN6","deduplicated":false,"chunks":1,"content_hash":"sha256:6724fd17b09b70eafa0eba8406367bdf41240a60de4a57d83f88540ff2fce727","created_at":"2026-09-01T02:17:05Z","superseded":[],"embedder":{"id":"granite-embedding-small-english-r2","dims":384}}

$ sqlite-mem ask --query "Why didn't we use that agent framework?"
{"ok":true,"op":"ask","mode":"hybrid","query":"Why didn't we use that agent framework?","results":[{"id":"01M1DC2Z17JDEG0AAXDA5ZFYN6","content":"We rejected Mastra because suspend/resume durability violated the Factory invariants.","score":0.01639,"ranks":{"semantic":1},"metadata":{"kind":"decision","project":"factory"},"system":{"created_at":"2026-09-01T02:17:05Z","source":"decisions.md#D012","status":"active","content_hash":"sha256:6724fd17b09b70eafa0eba8406367bdf41240a60de4a57d83f88540ff2fce727"}}],"stats":{"candidates":1,"returned":1,"elapsed_ms":639}}
```

That transcript is real: it was captured against the release `x86_64-unknown-linux-musl`
binary running in an empty directory with **no environment variables set at
all** — `env -i`, no `SQLITE_MEM_MODEL_DIR`, no config file, nothing. The
only artifact `sqlite-mem` left behind afterward was `.sqlite-mem/memory.db`.
Every JSON example in this document was run against the real binary the same
way; none of it is hand-written.

## Why this exists

sqlite-mem answers exactly one question: *"What have I been asked to
remember that is relevant now?"* It exposes two cognitive primitives —
`save` and `ask` — plus three mechanical lifecycle verbs — `forget`,
`reindex`, `info`. It never crawls folders, never inspects files, never
mutates your other files, never runs as a service, never becomes an agent,
and never opens a network connection. `ask` returns evidence (the saved
text, its provenance, its metadata), never a synthesized answer — the
calling AI does the reasoning; sqlite-mem does the retrieval.

## The offline guarantee

This is the product's headline claim, so it is structural, not a promise:

- The embedding model (`granite-embedding-small-english-r2`, Apache-2.0,
  384-dim, IBM's ModernBERT-based embedder) is linked into the binary via
  `include_bytes!` — weights, tokenizer, and config all ship inside the
  executable. There is nothing to download on first run.
- No HTTP, DNS, or socket-capable crate appears anywhere in the dependency
  tree. the CI workflow enforces this on every push (authored; first executed once the repo has a remote) with a `cargo tree` denylist gate
  (`reqwest`, `hyper`, `ureq`, `tokio-net`, ...); the check runs against
  every feature combination the product ships.
- The only files sqlite-mem ever opens are: the database you point it at,
  that database's WAL/SHM files, and its own `.bak` pre-migration backups.
  Nothing else, ever.

## Install

Download the binary for your platform from the
[releases page](../../releases) and put it on your `PATH`. That's the whole
install:

```console
$ curl -LO https://github.com/<org>/sqlite-mem/releases/latest/download/sqlite-mem-linux-x86_64-musl.tar.gz
$ tar xzf sqlite-mem-linux-x86_64-musl.tar.gz
$ chmod +x sqlite-mem
$ ./sqlite-mem info
```

Each release publishes a `SHA256SUMS.txt` alongside the archives; verify
before trusting a downloaded binary:

```console
$ sha256sum -c SHA256SUMS.txt --ignore-missing
```

Platforms (see [`.github/workflows/release.yml`](.github/workflows/release.yml)
for the exact build matrix):

| Target | Notes |
|---|---|
| `x86_64-unknown-linux-musl` | fully static, no libc dependency |
| `aarch64-unknown-linux-musl` | fully static, no libc dependency |
| `x86_64-apple-darwin` | signed + notarized when release secrets are configured |
| `aarch64-apple-darwin` | signed + notarized when release secrets are configured |
| `x86_64-pc-windows-msvc` | |

To build from source: `cargo build --release --features embed-model` (see
[Building from source](#building-from-source) below).

## Quick start

```console
$ sqlite-mem save --content "Runs are identified by ULID, not UUIDv4, so log lines sort chronologically." --meta project=factory --meta kind=decision
$ sqlite-mem ask --query "how do we identify a run"
$ sqlite-mem info
```

By default the database lives at `./.sqlite-mem/memory.db`, created on first
write. See [Database location](#database-location) to point it elsewhere.

## The CLI contract

Every invocation prints **exactly one JSON document to stdout** and nothing
else — no banners, no progress text, no partial lines. Diagnostics (only
emitted with `RUST_LOG`/`--verbose`-style tracing) go to stderr. This makes
every command safe to pipe straight into `jq` or a harness's JSON parser.

A successful response always carries `"ok": true`; a failure always carries
`"ok": false` and an `error` object (`code`, `message`, and an optional
`hint`). The process exit code always agrees with `ok` — see
[Exit codes](#exit-codes).

### `save`

```
sqlite-mem save [--db PATH] [--meta KEY=VALUE]... [--source STR]
                 [--supersedes ID]... [--if-new] (--content TEXT | --stdin)
```

Saves a distilled memory (not a file dump — 1–10 sentences is the intended
size; see [`folder-chief-conventions.md`](folder-chief-conventions.md) for
what belongs in a memory). Content and metadata are validated, the content
is chunked and embedded, and one row lands in the database inside a single
transaction. `--supersedes` marks the named prior memories `superseded` —
memories are immutable, so "editing" a memory means saving a new one that
supersedes the old one.

**Dedup:** if an `active` memory with identical content already exists,
`save` returns it with `"deduplicated": true` instead of inserting a
duplicate — this makes `save` safe to retry from an AI loop. `--supersedes`
targets are still marked superseded even on a deduplicated save. `--if-new`
turns a duplicate into a failure (exit 3, `not_new`) instead.

```console
$ sqlite-mem save --content "dup test content"
{"ok":true,"op":"save","id":"01M1DC5DCEQ7XYG7BMBRHKR2KX","deduplicated":false,"chunks":1,"content_hash":"sha256:adaac2bbfb097abd2286add06956bdc5a08fc16be087525be3b30596588bad1c","created_at":"2026-09-01T02:18:25Z","superseded":[],"embedder":{"id":"granite-embedding-small-english-r2","dims":384}}

$ sqlite-mem save --content "dup test content"
{"ok":true,"op":"save","id":"01M1DC5DCEQ7XYG7BMBRHKR2KX","deduplicated":true, ...}

$ sqlite-mem save --content "dup test content" --if-new
{"ok":false,"error":{"code":"not_new","message":"an active memory with identical content already exists (id 01M1DC5DCEQ7XYG7BMBRHKR2KX) and --if-new was set"}}
```

Supersession:

```console
$ ID1=$(sqlite-mem save --content "Old approach: poll every 5s." --meta project=demo | python3 -c "import json,sys;print(json.load(sys.stdin)['id'])")
$ sqlite-mem save --content "New approach: event-driven, no polling." --meta project=demo --supersedes "$ID1"
{"ok":true,"op":"save","id":"01M1DC5NHF42CB337PZGVHNTYG", ...,"superseded":["01M1DC5MZX13MZEEYEAB9A1ZES"], ...}
```

Validation caps (violations exit 3): content non-empty after trim, ≤ 1 MiB;
≤ 64 metadata pairs; metadata keys ≤ 128 bytes matching `[A-Za-z0-9_.-]+`;
metadata values ≤ 4 KiB.

```console
$ sqlite-mem save --content "x" --meta "bad key!=v"
{"ok":false,"error":{"code":"invalid_meta_key","message":"metadata key 'bad key!' must match [A-Za-z0-9_.-]+"}}

$ sqlite-mem save --content "   "
{"ok":false,"error":{"code":"empty_content","message":"content is empty after trimming"}}
```

### `ask`

```
sqlite-mem ask [--db PATH] [--k N (default 5, max 50)]
               [--where KEY=VALUE]... [--where KEY!=VALUE]... [--where KEY=*]...
               [--include-superseded] [--include-forgotten]
               [--mode hybrid|lexical|semantic (default hybrid)]
               [--min-score F] (--query TEXT | --stdin)
```

Hybrid retrieval: an FTS5/BM25 lexical leg and a brute-force cosine semantic
leg, fused with Reciprocal Rank Fusion (k=60), filtered by caller metadata.
**Scale-adaptive default:** below 4,096 stored chunks the lexical leg is
switched off and the default mode ranks purely semantically — benchmarking
showed lexical fusion actively hurts retrieval on small corpora and starts
helping around the several-thousand-chunk mark (so on a small store,
`ranks` carries only `semantic`, as in the examples below). The threshold
is an empirically calibrated policy, revisable from benchmark evidence.
Explicit `--mode lexical` / `--mode semantic` always run exactly the mode
you name, at any scale.
Returns **evidence, never a synthesized answer** — full memory content, its
score, its per-leg ranks, its metadata, and its provenance (`source`,
`created_at`, `status`, `content_hash`). Status filtering is `active`-only
by default; `--include-superseded`/`--include-forgotten` opt in, and
included results are always labeled via their `system.status` field.

```console
$ sqlite-mem ask --query "how do we get notified of changes"
{"ok":true,"op":"ask","mode":"hybrid","query":"how do we get notified of changes","results":[{"id":"01M1DC5NHF42CB337PZGVHNTYG","content":"New approach: event-driven, no polling.","score":0.01639,"ranks":{"semantic":1},"metadata":{"project":"demo"},"system":{"created_at":"2026-09-01T02:18:33Z","source":null,"status":"active","content_hash":"sha256:3d5ded23a5f45a92a7a10fc09aaac6ff34e8db40cf50dc7d2c01f2571c98fa4f"}}],"stats":{"candidates":1,"returned":1,"elapsed_ms":554}}
```

Filters (`--where`, repeatable, ANDed — `KEY=VALUE` equality, `KEY!=VALUE`
exclusion, `KEY=*` existence):

```console
$ sqlite-mem ask --query "note" --where "status!=draft"
$ sqlite-mem ask --query "note" --where "kind=*"
```

`--mode lexical` and `--mode semantic` each run a single leg — useful for
benchmarking and as the degraded path if you want retrieval without paying
the ~500ms model-load cost (`--mode lexical` never loads the embedder):

```console
$ echo "alpha" | sqlite-mem ask --stdin --mode lexical
{"ok":true,"op":"ask","mode":"lexical","query":"alpha","results":[{"id":"01M1DC615EE314XRYNE174KWWB","content":"alpha beta gamma","score":0.01639,"ranks":{"lexical":1}, ...}],"stats":{"candidates":1,"returned":1,"elapsed_ms":2}}
```

No results is still a success (exit 0, empty `results`):

```console
$ sqlite-mem ask --query "anything" --db ./fresh-empty.db
{"ok":true,"op":"ask","mode":"hybrid","query":"anything","results":[],"stats":{"candidates":0,"returned":0,"elapsed_ms":...}}
```

### `forget`

```
sqlite-mem forget [--db PATH] ID... [--purge | --restore]
```

Soft-deletes by default (`status: forgotten`, excluded from `ask` unless
`--include-forgotten`, fully recoverable). `--purge` hard-deletes the
memory, its chunks, FTS rows, and metadata in one transaction — the only
destructive operation in the product, and its response says so
(`"destructive": true`). `--restore` returns a forgotten memory to its
prior status. `forget` is all-or-nothing per invocation: if any listed ID
doesn't exist, nothing changes and it exits 4.

```console
$ sqlite-mem forget 01M1DC5MZX13MZEEYEAB9A1ZES
{"ok":true,"op":"forget","mode":"forget","destructive":false,"results":[{"id":"01M1DC5MZX13MZEEYEAB9A1ZES","status":"forgotten","forgotten_at":"2026-09-01T02:18:34Z","changed":true}],"count":1}

$ sqlite-mem forget 01M1DC5MZX13MZEEYEAB9A1ZES --restore
{"ok":true,"op":"forget","mode":"restore","destructive":false,"results":[{"id":"01M1DC5MZX13MZEEYEAB9A1ZES","status":"superseded","changed":true}],"count":1}

$ sqlite-mem forget 01M1DC60KBDABE920TMHR7PM6W --purge
{"ok":true,"op":"forget","mode":"purge","destructive":true,"results":[{"id":"01M1DC60KBDABE920TMHR7PM6W","status":"purged","changed":true}],"count":1}

$ sqlite-mem forget 01ARZ3NDEKTSV4RRFFQ69G5FAV
{"ok":false,"error":{"code":"not_found","message":"unknown memory id(s): 01ARZ3NDEKTSV4RRFFQ69G5FAV","hint":"no changes were made -- forget/purge/restore are all-or-nothing per invocation"}}
```

### `reindex`

```
sqlite-mem reindex [--db PATH]
```

Re-embeds every chunk with the binary's current embedder and updates
`db_info` — the recovery/upgrade path when a newer sqlite-mem ships a
different bundled model. Takes a timestamped `.bak` backup first.

```console
$ sqlite-mem reindex
{"ok":true,"op":"reindex","chunks_reindexed":2,"backup":".sqlite-mem/memory.db.bak.20260901T021834Z","previous_embedder":{"id":"granite-embedding-small-english-r2","dims":384},"embedder":{"id":"granite-embedding-small-english-r2","dims":384}}
```

If the binary's bundled embedder doesn't match what a database was created
with, `save` and `ask --mode hybrid|semantic` refuse with exit 6 and a hint
naming `reindex`; `ask --mode lexical`, `forget`, and `info` still work
against a mismatched database.

### `info`

```
sqlite-mem info [--db PATH] [--verify]
```

Reports schema version, embedder identity, memory counts by status, chunk
count, and database size:

```console
$ sqlite-mem info
{"ok":true,"op":"info","path":"/tmp/doccheck2/.sqlite-mem/memory.db","schema_version":1,"embedder":{"id":"granite-embedding-small-english-r2","dims":384},"counts":{"active":1,"superseded":1,"forgotten":0},"chunks":2,"db_size_bytes":69632}
```

`--verify` additionally runs `PRAGMA integrity_check`, an FTS-index
backfill audit, an embedding-dimension audit, and a content-hash
spot-check, reporting each as a `checks.<name>: {pass, detail}` entry. Any
failed check exits 7 with `ok:false` and `error.code = "integrity_failed"`
— **every non-zero exit pairs with `ok:false`**, uniformly, including this
one:

```console
$ sqlite-mem info --verify
{"ok":true,"op":"info","verify":true,"path":"/tmp/doccheck2/.sqlite-mem/memory.db","schema_version":1,"embedder":{"id":"granite-embedding-small-english-r2","dims":384},"counts":{"active":1,"superseded":1,"forgotten":0},"chunks":2,"db_size_bytes":69632,"checks":{"integrity_check":{"pass":true,"detail":"ok"},"fts_consistency":{"pass":true,"detail":"2 chunk(s) in sync with the fts index"},"embedding_dims":{"pass":true,"detail":"2 chunk(s), all 1536-byte embeddings"},"content_hash":{"pass":true,"detail":"2 memory content_hash(es) verified"}}}
```

## Exit codes

| Code | Meaning |
|---|---|
| 0 | success |
| 2 | usage / invalid arguments |
| 3 | validation failure (oversized input, bad metadata key, empty content/query, `--if-new` on a duplicate) |
| 4 | not found (unknown memory ID passed to `forget`) |
| 5 | database error (locked past timeout, corrupt, permissions, unwritable path) |
| 6 | version/compatibility (schema newer than this binary, or embedder mismatch without `reindex`) |
| 7 | integrity check failure (`info --verify`) |

An integration test (`tests/exit_codes.rs`) asserts the full table against
the real binary for every code, and a separate test
(`tests/output_discipline.rs`) asserts stdout is always exactly one
parseable JSON document with no stray `println!` anywhere in the codebase.

## Determinism contract

Identical database + identical query ⇒ identical output, modulo one field.
Concretely: `serde` field order is fixed, scores are rounded to 5 decimals,
and total order is `(score DESC, id ASC)` so ties never reorder across
runs. `stats.elapsed_ms` is the only intentionally nondeterministic field —
strip it before diffing.

Raw embedding vectors can differ in the last float ulp across different
platforms' libm implementations (this is true even between two Linux
builds using different libc), so byte-identity is guaranteed of the
**rounded JSON output**, not of raw floats. This was verified for this
release: the same database, asked the same query, produced byte-identical
JSON (minus `elapsed_ms`) from the `x86_64-unknown-linux-gnu` and
`x86_64-unknown-linux-musl` release binaries. See
[`.github/workflows/release.yml`](.github/workflows/release.yml) for the
cross-platform (Linux/macOS/Windows) determinism gate CI runs on every
release.

## Database location

Resolved in this order: `--db PATH` flag, else `SQLITE_MEM_DB` env var,
else `./.sqlite-mem/memory.db`. An explicitly given `--db`'s parent
directory must already exist (missing parent ⇒ exit 5,
`db_path_unavailable`); the default path's directory is created for you,
`0700`. Database files are created `0600`. It's a completely ordinary
SQLite file — openable with the stock `sqlite3` CLI or any SQLite browser
for manual inspection or disaster recovery; sqlite-mem never uses a
proprietary format.

## Treat retrieved content as data, not instructions

sqlite-mem never interprets, executes, templates, or evaluates anything you
save. `ask` returns whatever text was saved, verbatim, inside a JSON
string. If your harness feeds `ask` results back into a prompt, treat that
content exactly like you'd treat the contents of a file you didn't write
yourself — untrusted data, not instructions, even though nothing in
sqlite-mem's own path can execute it. See
[`folder-chief-conventions.md`](folder-chief-conventions.md) for the fuller
integration guidance, including why this matters for saved content that
might contain prompt-injection text.

## Building from source

```console
$ cargo build --release --features embed-model
```

`embed-model` links the model weights into the binary via `include_bytes!`
and is the only feature release binaries ship with. By default
(`cargo build` with no `--features`) the crate builds against
`model-sidecar` instead, which loads the model from a directory on disk
named by `SQLITE_MEM_MODEL_DIR` at *runtime* — this keeps the dev/test
edit-compile loop fast, since nothing is linked into the binary. `cargo
build --features embed-model` resolves its model directory at *build* time
instead (see `build.rs`): it defaults to the model already checked out
under `spike/embed-parity/models/granite/` in this repository, or reads
`SQLITE_MEM_EMBED_MODEL_DIR` if you point it elsewhere (this is how release
CI feeds it the model it just downloaded, sha256-verified, and converted to
f16 — see `.github/workflows/release.yml`).

`cargo build` never touches the network by itself, in any configuration —
fetching and converting the model is always a separate, explicit step you
(or CI) perform first.

## License

sqlite-mem is dual-licensed under [MIT](LICENSE-MIT) OR
[Apache-2.0](LICENSE-APACHE), at your option. Copyright © 2026 Lee
Harrington.

Third-party code, patterns, and the bundled model's own license are
recorded in [`THIRD-PARTY.md`](THIRD-PARTY.md).

## Further reading

- [`folder-chief-conventions.md`](folder-chief-conventions.md) — when to
  `save`/`ask`, what not to save, metadata conventions, provenance and
  supersession usage for AI harnesses (written for, but not limited to,
  Folder Chief integration).
- [`architecture.md`](architecture.md) — the full system design, schema,
  and rationale.
- [`THIRD-PARTY.md`](THIRD-PARTY.md) — code, pattern, and model
  attributions.
