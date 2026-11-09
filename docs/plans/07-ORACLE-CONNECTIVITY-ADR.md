# 07: Architecture Decision Record (ADR) — Oracle Database Connectivity

> **Status**: Accepted  
> **Date**: 2026-11-09  
> **Context**: Phase A implementation delivered a working query path against Oracle Database 23ai Free, but relied on shelling out to `podman exec ... sqlplus` inside the testbed container. The workbench needs a clear, honest, and staged database connectivity architecture that balances immediate developer usability against our core invariant: **Zero mandatory Oracle runtime/client software**.

---

## 1. Problem Statement

Oracle Forms applications are inextricably bound to relational databases (historically Oracle 7 through 23ai). To enable live data queries, record browsing, LOV execution, and transaction processing, Oracle Forms Workbench requires database connectivity.

However, standard Oracle connectivity in Rust typically presents a dilemma:
- **Native OCI bindings** (`rust-oracle` / OCI C API) require installing the proprietary **Oracle Instant Client** libraries (`libclntsh.dylib` on macOS, `libclntsh.so` on Linux, `oci.dll` on Windows), adding ~80–200 MB of platform-specific binary dependencies and environment variable configuration (`LD_LIBRARY_PATH`, `PATH`).
- **Pure-Rust wire protocols** (direct TCP socket to `:1521` speaking Oracle Two-Task Common / TTC and TNS) do not currently have a mature, production-grade off-the-shelf Rust crate on crates.io that supports modern Oracle 19c/21c/23ai verifier authentication (O5LOGON/12C/SHA-256 verifiers), Native Network Encryption (NNE), TLS, and LOB/DATE/NUMBER encoding.
- **Testbed CLI bridge** (`podman exec sqlplus`) works immediately with our existing 23ai Free container testbed, but is not a native library driver, requires container runtime (`podman` or `docker`), and incurs per-statement process spawn overhead.

---

## 2. Evaluation of Options

| Dimension | Option A: Testbed Sqlplus Driver | Option B: OCI via `oracle` Crate (`oracle-oci`) | Option C: Pure-Rust Thin Driver (`oracle-thin`) |
|---|---|---|---|
| **Mechanism** | Shells out to `podman exec -i <container> sqlplus -s /nolog` over standard input | Wraps Oracle Call Interface (OCI) via `rust-oracle` | Implements TNS packet wire format and TTC protocol directly over `tokio::net::TcpStream` |
| **Client Requirement** | None on host; requires Podman/Docker running `oracle23ai-lite` | Requires **Oracle Instant Client** installed on host (~80 MB) | **Zero client dependencies**; pure native Rust binary |
| **Platforms** | Any host with Podman (macOS, Linux, Windows WSL2) | Platforms supported by Instant Client (x86_64, arm64 macOS, Windows) | Any platform compiled by `rustc` |
| **Performance** | ~300–600 ms per query (process execution & container bridge overhead) | < 1 ms per query (in-process shared library connection pool) | < 1 ms per query (in-process async non-blocking TCP socket) |
| **Bind Parameters** | Emits SQL*Plus variable bindings (`VARIABLE` / `EXEC`) | Native OCI positional & named bind variables | Native TTC bind parameter protocol |
| **Implementation Cost** | **Already built** (needs security & bind cleanup in Phase A.1) | **Small** (~2–3 days to wrap behind cargo feature flag) | **Large** (dedicated parallel project track, 4–6 weeks) |
| **Reference Implementations** | N/A | `rust-oracle` (MIT/Apache-2.0) | `python-oracledb` (Thin mode, UPL/Apache-2.0), `go-ora` (MIT), `node-oracledb` (Thin mode, UPL/Apache-2.0) |

---

## 3. Decision

We adopt a **three-tier staged connectivity strategy**:

1. **Tier 1 (Current / Baseline)**: **Option A (Testbed Sqlplus Driver)**
   - Keep Option A for immediate out-of-the-box operation when using the containerized testbed.
   - **Crucial renaming and relabeling**: Rename from `OracleDriver` to `TestbedSqlplusDriver` in module `testbed_sqlplus.rs`.
   - Clearly label in the UI and documentation: *"Uses the local Podman testbed's sqlplus. Requires container `oracle23ai-lite`. Not a native driver."*
   - Clean up credentials and security: remove hardcoded passwords, execute via `/nolog` with `CONNECT` passed securely over stdin, and support bound parameters.

2. **Tier 2 (Enterprise & Performance Option)**: **Option B (OCI Driver via Feature Flag)**
   - Introduce an optional, non-default Cargo feature flag in `oracle-forms-db`:
     ```toml
     [features]
     default = ["sqlite"]
     oracle-oci = ["dep:oracle"]
     sqlite = ["dep:rusqlite"]
     ```
   - When compiled with `--features oracle-oci`, users who have Oracle Instant Client installed can use the ultra-fast, in-process OCI driver with zero middleware, zero WebLogic, and zero JVM.
   - When compiled without this flag (the default), `oracle-forms-workbench` compiles cleanly on any machine without Instant Client installed.

3. **Tier 3 (Pure-Native Strategic Track)**: **Option C (Pure-Rust Thin Driver `oracle-thin`)**
   - Establish a **separate, parallel track** (Track T) in its own crate `crates/oracle-thin/`.
   - Reference implementation: `python-oracledb` (Thin mode, licensed under UPL-1.0 and Apache-2.0) and `go-ora` (pure Go Oracle driver, MIT).
   - Core components to implement:
     1. TNS Connect/Accept packet exchange over TCP/TLS.
     2. TTC (Two-Task Common) protocol: Protocol negotiation, DataType representation (Oracle NUMBER 38-digit decimal, DATE 7-byte century/year/month/day/hour/min/sec, VARCHAR2 UTF-8).
     3. Authentication: O5LOGON protocol supporting 11G (SHA-1), 12C (PBKDF2/SHA-512), and modern 23ai authentication verifiers.
     4. Query & Execute: TTC statement execute packet with bound parameters and streaming result set fetch.
   - **Promotion Gate**: Track T becomes the default driver only after passing 100% of the conformance test suite against Oracle Database 23ai Free without regressions.

---

## 4. Invariants & Rules

1. **Workspace Purity**: The core Rust workspace (`oracle-forms-core`, `oracle-forms-xml`, `oracle-forms-binary`, `oracle-forms-plsql`, `oracle-forms-runtime`) must **never** depend on Oracle software or Instant Client libraries.
2. **Feature Gating**: Any database driver that requires Oracle client software (`oracle-oci`) MUST sit behind a non-default cargo feature flag and be explicitly named as such.
3. **No Hardcoded Credentials**: No database driver may embed default or fallback usernames or passwords. All credentials must be passed dynamically via the API request or environment variables (`OFW_DB_USER`, `OFW_DB_PASSWORD`).
4. **Parameter Safety**: User-supplied values must NEVER be formatted as raw text into SQL strings. All drivers must enforce bound parameters via `BoundSql`.

---

## 5. Consequences

- **Positive**: 
  - Resolves the architectural contradiction between "zero client software" and "OCI crate dependencies".
  - Developers with Instant Client get instant high-performance native connectivity.
  - Developers without Instant Client can either use SQLite (embedded), Mock, or the Testbed Sqlplus container driver.
  - Clear roadmap for developing a true pure-Rust thin driver without stalling workbench product milestones.
- **Negative**:
  - Developing Option C is a substantial engineering effort requiring deep protocol reverse-engineering; staging it as an independent parallel track prevents it from blocking Phase B/C/D.
