# sqlite-mem Decisions

Only accepted product decisions belong here. Research findings, examples, and
candidate implementation techniques remain proposals until explicitly decided.

## Accepted Decisions

### D001 — Standalone product

sqlite-mem is a standalone product, separate from Folder Chief. Folder Chief
may consume it, but the product must remain generally useful to arbitrary AI
harnesses.

### D002 — Local file and transient process model

Persistent state lives in a user-owned local SQLite file. The sqlite-mem
process starts for an operation, reads or updates that file, returns
deterministic machine-readable output, and exits. It is not a server, daemon,
service, container, or autonomous agent.

### D003 — Portable, no-install distribution target

The target distribution is self-contained, no-install binaries for macOS,
Linux, and Windows. The precise language, runtime, model packaging, supported
CPU architectures, and build mechanism remain undecided.

### D004 — Two initial conceptual primitives

The initial interface is organized around `SAVE THIS CONTENT` and
`ANSWER THIS QUESTION`. Exact command syntax and protocol schemas remain open.

### D005 — Calling AI owns judgment; sqlite-mem owns mechanics

The calling AI decides what to inspect, what matters, what to save, and what
metadata to supply. sqlite-mem does not crawl whole folders. It owns safe
storage and transactions, indexing and retrieval mechanics, deterministic
local embedding generation, hybrid ranking, metadata persistence/filtering,
deterministic output, and appropriate provenance/system metadata.

### D006 — No user-configured AI or embedding infrastructure

The user must not need to configure an LLM, provider, API key, Ollama, LM
Studio, embedding server, required cloud service, or required Python
environment. sqlite-mem owns the stable deterministic local encoding required
by its memory format; the exact implementation remains undecided.

### D007 — Metadata is configurable and first-class

Caller metadata is configurable at ingestion and retrieval. The design must
distinguish caller-supplied metadata, sqlite-mem system/provenance metadata,
and retrieval/ranking metadata without imposing a Folder-Chief-specific
ontology.

### D008 — Narrow authority and caller-source safety

sqlite-mem must remain operationally simple, local-first, user-owned, and
narrow in authority. It must not silently inspect arbitrary files or mutate
caller-owned source material.

## Explicit Non-Decisions

- FTS5, vector extensions, vector search, hybrid retrieval, and Reciprocal Rank
  Fusion are research candidates, not selected architecture.
- No implementation language, database schema, embedding model/runtime,
  packaging design, protocol syntax, query grammar, or ranking method has been
  selected.
