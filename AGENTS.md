# AGENTS.md

Instructions and operational constraints for AI coding agents operating within this repository.

---

## Overview

This repository houses **Oracle Forms Workbench**, a native Rust development workbench for inspecting, reverse-engineering, authoring, modifying, comparing, and running Oracle Forms & Reports artifacts (`.fmb`, `.mmb`, `.olb`, Forms2XML exports, `.pld`, and report XML; official tooling also converts `.pll`, `.rdf`, and `.rpt`).

The delivered workflow connects XML form design and versioned saves in the Rust UI to both the native form runtime and the optional Oracle Fusion Middleware reference environment. Coding agents can author XML and PL/SQL, inspect the result, and use the same validation pipeline. The native runtime interprets form models; the Oracle toolchain converts XML to FMB, compiles FMX, and launches the updated form through FSAL. Preserve this full development loop, rather than treating the project as a read-only viewer.

---

## Invariants & Rules

### Architecture & Dependencies

- **Zero JVM in Rust Workspace**: Never introduce Java, JVM, Javalin, or Node.js runtime/build dependencies into the Rust workspace (`crates/`).
- **Zero Oracle Runtime Dependency**: Native parsing, extraction, PL/SQL interpretation, form sessions, and the Oracle Thin/SQLite/Mock database paths MUST work without Oracle Forms Builder, Oracle Client libraries, or Windows DLLs. The optional testbed SQL*Plus driver and Oracle compilation/FSAL bridge remain explicitly separate.
- **Portability**: NEVER hardcode absolute machine paths in code, configuration, or documentation. Use workspace-relative paths (`./`, `samples/`, `docs/`) and configurable connection settings.
- **Data Privacy & Git Hygiene**: Never commit or stage files from `temp/`, `temp/example-fmb/`, or `context/`. Proprietary binaries, internal form files, and session memories must remain strictly within ignored directories.
- **Save Semantics**: ALWAYS distinguish **Save Form** (a new versioned XML definition in `samples/`) from **Save Record** (database data). NEVER imply a form save rewrites the source FMB. Preserve unknown source XML for item patches; identify regenerated model summaries as potentially lossy.
- **Oracle Preview Boundary**: MUST stage conversion/compilation in ignored job directories, preserve source artifacts, and show setup/compile failures. Oracle preview launches a Java desktop window on the server host and requires the optional reference stack. Readiness or compilation success MUST NOT be presented as visual or behavioral parity.
- **Database & Credentials**: MUST retain read-only native sessions by default and explicit DML opt-in. NEVER persist passwords in source, saved XML, browser storage, or logs. Missing compile credentials MUST produce a useful error unless offline compilation was explicitly selected.
- **Honest Capability Claims**: MUST distinguish direct binary heuristics, modeled XML support, and official Oracle conversion. Do not claim exhaustive Oracle built-in coverage, lossless structural regeneration, persisted UI-only font settings, or full report paper-layout round trips. AI-assisted authoring uses coding agents and the artifact/tooling workflow; do not invent an in-app AI service.

### Coding Standards

- **Direct In-Place Execution**: When implementing tasks, make changes directly to affected files in place. Do not artificially bottleneck or route code proposals through intermediate buffers or temporary files unless requested.
- **Verification Gates**: After any modification, always verify from the repository root that:
  ```bash
  cargo check
  cargo test
  ```
  All tests must compile and pass with zero warnings.

---

## Architecture & Landmarks

See [architecture.md](architecture.md) for crate boundaries, data flows, APIs, and the two-runtime design. The eight crates are `core`, `xml`, `binary`, `db`, `plsql`, `runtime`, `cli`, and `server` under `crates/oracle-forms-*`. `ui/` contains embedded vanilla HTML/CSS/JavaScript/SVG. `samples/` contains shared fixtures and versioned form saves. `plugins/oracle-forms/` holds optional reference automation; `oracle-reference-stack/` holds its container configuration.

---

## Core Commands

### Running the Native Viewer

```bash
cargo run -p oracle-forms-server
# Serves http://127.0.0.1:7070
```

### Inspecting & Querying Modules via CLI

```bash
# Inspect module structure
cargo run -p oracle-forms-cli -- inspect samples/orders_fmb.xml
cargo run -p oracle-forms-cli -- inspect samples/simple_module.fmb

# Execute a live query (provide OFW_DB_USER / OFW_DB_PASSWORD in the environment)
cargo run -p oracle-forms-cli -- query samples/test_db_fmb.xml --backend oracle-thin
cargo run -p oracle-forms-cli -- query samples/orders_fmb.xml --backend mock

# Dump extracted PL/SQL
cargo run -p oracle-forms-cli -- dump-sql samples/orders_fmb.xml -o ./extracted-sql
```

## On-Demand References

- Read [README.md](README.md) for the complete open/design/save/run/publish workflow and AI-assisted authoring examples.
- Read [HOW-TO-RUN.md](HOW-TO-RUN.md) for server configuration, CLI usage, and database connections.
- Read [Oracle automation](plugins/oracle-forms/README.md) and the relevant `.apm/skills/` instructions when converting, compiling, provisioning, or comparing with Oracle. The Preview dialog also provides dependency checks and startup guidance. Do not start or replace user containers merely to update documentation.
- Read [native architecture](docs/NATIVE-RUST-WORKBENCH-ARCHITECTURE.md) for detailed implementation context, and [roadmap](docs/plans/06-IMPLEMENTATION-ROADMAP-AND-MILESTONES.md) / [continuation status](docs/plans/10-NEXT-PHASE-SESSION-PROMPT.md) before extending the current implementation. Keep completed work and unverified behavior distinct in status updates.

