# sqlite-mem Product Definition

**Status:** Authoritative initial product definition

**Phase:** Pre-implementation

**Last updated:** 2026-08-31

## Product

sqlite-mem is a tiny, safe, local-first persistent-memory primitive for AI
execution harnesses such as Codex, Claude Code, Gemini CLI, Folder Chief, and
other AI systems.

It is a separate product from Folder Chief. Folder Chief may become an
important consumer, but sqlite-mem must remain generally useful to arbitrary AI
harnesses.

## Core Operating Model

SQLite is a file, not a database server.

sqlite-mem must therefore not introduce:

- a SQL server
- a daemon or background service
- Docker
- a required Python environment
- a hosted vector database
- a required cloud service
- an LLM provider that the user must configure
- an installer

The intended experience is that the appropriate sqlite-mem executable exists
in a folder and an AI can invoke it. Target distribution should ultimately
include portable, self-contained, no-install binaries for macOS, Linux, and
Windows.

Persistent memory lives in an ordinary, user-owned local SQLite database file.
The sqlite-mem process is transient:

1. Start.
2. Perform the requested operation.
3. Read or update the SQLite file safely.
4. Return deterministic machine-readable output.
5. Exit.

The executable is a tool, not an employee, agent, service, or orchestrator.

## First Two Conceptual Primitives

The initial external interface is designed around only two conceptual
primitives:

1. **SAVE THIS CONTENT** — The calling AI explicitly supplies content it
   believes should become durable memory.
2. **ANSWER THIS QUESTION** — The calling AI supplies a question and
   sqlite-mem retrieves the most relevant saved memory needed to answer it.

Do not prematurely expand this into a large RAG CLI with many verbs. Exact
command syntax and request/response schemas are not yet decided.

## Division of Responsibility

sqlite-mem does not process, crawl, understand, or recursively index an entire
folder. Modern AI harnesses already have filesystem tools and can inspect
folders themselves.

The calling AI decides:

- what files or other sources to inspect
- what information matters
- what content deserves persistence
- what content to submit to sqlite-mem
- what metadata should accompany it

sqlite-mem owns the mechanical memory functions:

- durable storage
- safe transactions
- lexical indexing
- deterministic local embedding generation
- vector storage and retrieval
- hybrid retrieval and ranking
- metadata persistence and filtering
- deterministic machine-readable output
- provenance and system metadata as appropriate

In shorthand:

> Filesystem tools answer: What exists here?
>
> The AI answers: What matters?
>
> sqlite-mem answers: What have I been asked to remember that is relevant now?

## Embeddings and Stable Encoding

The user must not have to configure sqlite-mem with its own LLM or external AI
service. Embedding generation is an implementation responsibility of
sqlite-mem, not of the calling AI.

The operational experience must not require:

- API keys
- provider configuration
- Ollama or LM Studio
- an embedding server
- Python package installation

sqlite-mem must own the deterministic local embedding mechanism required by its
memory format so that stored representations do not depend on which AI harness
calls it. Research is still required to choose the portable implementation and
packaging strategy. Do not prematurely select a model, runtime, vector
extension, language, or executable-packaging mechanism.

## Metadata

Metadata is configurable and first-class at both sides of the protocol.

At ingestion, the calling AI can attach arbitrary useful metadata. Illustrative
examples include project, kind/type, authority, source, date, entity, topic,
and status. These examples are not a mandatory ontology.

At retrieval, the calling AI can use metadata to filter, constrain, prefer,
exclude, or otherwise influence retrieval. sqlite-mem must not require a
Folder-Chief-specific schema.

The design must distinguish:

- caller-supplied metadata
- sqlite-mem system and provenance metadata
- retrieval and ranking metadata

Exact metadata storage, schema, validation, and query grammar remain open
questions.

## Retrieval Direction: Candidates, Not Architecture

Research indicates potential value from combining SQLite relational storage,
SQLite FTS5 lexical retrieval, vector embeddings, hybrid lexical and semantic
retrieval, rank fusion such as Reciprocal Rank Fusion, and metadata filtering
or ranking.

These are research findings and candidate implementation techniques. They are
not committed product architecture. The product must remain operationally
simple from the caller's perspective regardless of internal sophistication.

## Safety and Simplicity Principles

sqlite-mem should be:

- local-first
- user-owned
- portable
- deterministic at its protocol boundary
- small in operational complexity
- safe for autonomous AI invocation
- narrow in authority
- infrastructure-free
- useful without becoming an autonomous agent

It must not silently inspect arbitrary files or mutate caller-owned source
material. The SQLite memory file is persistent state owned by the user.

## Product Kernel to Prove

A successful sqlite-mem makes this possible:

1. An AI in one session decides something is worth remembering and saves it.
2. A different AI or a later session asks a conceptually related question
   using different wording.
3. sqlite-mem returns the relevant prior memory with appropriate metadata and
   provenance, without an always-running service or configured AI provider.

This is the technological and product kernel to prove.

## Explicit Non-Goals for Initial Design

- A database server, daemon, or hosted service
- An autonomous crawler, folder indexer, agent, or orchestrator
- A user-configured LLM, embedding provider, or model-serving stack
- A broad RAG command suite
- A Folder-Chief-specific data model
- Silent inspection or mutation of caller-owned files

## Unresolved Implementation Questions

Record these as open questions rather than resolving them during project setup:

- implementation language
- SQLite embedding and vector implementation
- embedded versus adjacent embedding-model packaging
- candidate embedding model
- binary-size targets
- supported CPU architectures
- exact save protocol
- exact ask protocol and whether it returns evidence, an answer, or both
- metadata query grammar
- chunking behavior
- update and supersession semantics
- deletion and forget semantics
- database schema
- hybrid-ranking strategy
- benchmark corpus
- security boundaries
- concurrency behavior
- database portability and versioning
- licensing implications of the embedding runtime and model

## Current Product State

The product concept is established, but no implementation architecture or
implementation decision has been accepted. Research exists, but it is not
architecture. The next step is architecture and research narrowing before any
implementation begins.
