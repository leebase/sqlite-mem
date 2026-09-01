# Security Policy

## Reporting a vulnerability

Please report suspected vulnerabilities privately via
[GitHub Security Advisories](https://github.com/leebase/sqlite-mem/security/advisories/new)
rather than opening a public issue. You should receive a response within
a week.

## Scope and threat model

sqlite-mem is designed to be safely callable by autonomous AI harnesses.
Reports are especially welcome for anything that violates the guarantees
in `architecture.md` §20/§24:

- any network activity, from the binary or its build (there must be none)
- SQL or FTS5 query injection through content, metadata, or filters
- stored content being interpreted or executed rather than round-tripped
  as inert data
- escaping the documented filesystem footprint (the DB file, its
  WAL/SHM, and timestamped `.bak` copies)
- output-contract violations (anything other than exactly one JSON
  document on stdout, or an undocumented exit code)
- corruption that `info --verify` fails to detect
- denial of service through oversized or pathological inputs

The database file itself is user-owned and unencrypted by design;
protecting it at rest is the operator's responsibility.

## Audit history

v1 underwent an independent security audit (checklist, findings, and the
fix/re-audit cycle are summarized in `result-review.md`). `cargo audit`
runs in CI.
