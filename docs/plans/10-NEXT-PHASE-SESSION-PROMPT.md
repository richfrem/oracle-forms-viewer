# 10: Next Phase Session Starter Prompt

> **Purpose**: Copy and paste the prompt below into a new/clean AI coding session to immediately resume work on the next phase of the Oracle Forms Workbench roadmap.

---

## Quick Context Summary

- **Repository**: `oracle-forms-fmb-research` (GitHub: `richfrem/oracle-forms-workbench`)
- **Active Branch**: `main` (fully synced, clean working tree)
- **Active Stack**:
  - Native multi-crate Rust workspace at root (`crates/oracle-forms-*`)
  - Web viewer on Axum at `http://127.0.0.1:7070`
  - Container testbed: `oracle23ai-lite` (Podman / 1521) with seeded tables (`ORDERS`, `CUSTOMERS`, `EMPLOYEES`, `DEPARTMENTS`, `CLAIMS`)
  - Forms 14c container: `forms-server-14c` (WebLogic AdminServer on 7001)

---

## 📋 Copy-Paste Prompt for New Session

Copy everything inside the box below and paste it as your initial prompt in the new session:

```markdown
You are pairing with me on **Oracle Forms Workbench** (`oracle-forms-fmb-research`), a native Rust toolchain and web viewer for legacy Oracle Forms & Reports.

### 1. Invariants to Always Follow
- **Zero JVM / Node in Rust Workspace**: The Rust crates (`crates/`) must remain 100% pure native Rust.
- **Direct Workspace Execution**: Modify repository files directly in place. Do not bottleneck changes through temporary files.
- **Portability**: Never use absolute machine paths. Use workspace-relative paths (`./`, `samples/`, `docs/`).
- **Verification Gate**: After every change, verify with `cargo check && cargo test`. All tests must pass with zero warnings.

### 2. Current Project State
- **Phase A (Completed)**: Read-only block query engine (`crates/oracle-forms-db`), SQL generator, live container execution against Oracle 23ai (`oracle23ai-lite`), CLI `query` command, and REST API (`/api/forms/query-block`).
- **UI Viewer (Completed)**: Multi-block auto-flow layout, native SVG Tab Canvas notebook rendering, interactive tab switching, dynamic bounds expansion, and record navigation at `http://127.0.0.1:7070`.
- **Database Testbed**: Podman container `oracle23ai-lite` is running with seeded schema tables (`ORDERS`, `CUSTOMERS`, `DEPARTMENTS`, `EMPLOYEES`, `CLAIMS`, `STOCK`, etc.).

### 3. Immediate Mission: Milestone A.1 & B Selection
Please inspect `docs/plans/06-IMPLEMENTATION-ROADMAP-AND-MILESTONES.md` and help me execute the next milestone:

- **Milestone A.1 (Driver Trait Hardening & Security Fix-Forward)**:
  1. Rename `OracleDriver` -> `TestbedSqlplusDriver` in `crates/oracle-forms-db/src/testbed_sqlplus.rs`.
  2. Relabel UI backend selector to `Oracle (testbed sqlplus)`, `SQLite`, `Mock`.
  3. Remove hardcoded credentials (`Welcome123#`); support `OFW_DB_USER` / `OFW_DB_PASSWORD` env vars and `/nolog` stdin authentication.
  4. Introduce `BoundSql` to `DatabaseDriver::query` to prepare for parameter binding.
  5. Select `ROWID` and rewrite binds in `WhereClause`.

- **Milestone B (Runtime Spine & Navigation State Machine)**:
  1. Scaffold `crates/oracle-forms-runtime/`.
  2. Implement `FormSession` and `BlockBuffer` managing records with `FormsRecordStatus` (`NEW`, `INSERT`, `QUERY`, `CHANGED`).
  3. Implement navigation state machine (Form -> Block -> Record -> Item).
  4. Implement system variables (`:SYSTEM.CURSOR_ITEM`, `CURSOR_BLOCK`, `CURSOR_RECORD`, `MODE`).
  5. Implement master-detail coordination for `ORDERS` -> `ORDER_LINES`.

Let's review where we are, confirm git status, and start executing Milestone A.1!
```

---

## Reference Architecture Documents
- [06-IMPLEMENTATION-ROADMAP-AND-MILESTONES.md](file:///Users/richardfremmerlid/Projects/oracle-forms-fmb-research/docs/plans/06-IMPLEMENTATION-ROADMAP-AND-MILESTONES.md)
- [02-CORE-ARCHITECTURE-AND-CRATES.md](file:///Users/richardfremmerlid/Projects/oracle-forms-fmb-research/docs/plans/02-CORE-ARCHITECTURE-AND-CRATES.md)
- [03-RUNTIME-STATE-MACHINE-AND-TRIGGERS.md](file:///Users/richardfremmerlid/Projects/oracle-forms-fmb-research/docs/plans/03-RUNTIME-STATE-MACHINE-AND-TRIGGERS.md)
- [04-DATABASE-INTEGRATION-AND-CRUD.md](file:///Users/richardfremmerlid/Projects/oracle-forms-fmb-research/docs/plans/04-DATABASE-INTEGRATION-AND-CRUD.md)
- [AGENTS.md](file:///Users/richardfremmerlid/Projects/oracle-forms-fmb-research/AGENTS.md)
