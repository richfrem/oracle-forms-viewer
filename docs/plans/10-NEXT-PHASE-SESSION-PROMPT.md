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
- **Phase A.1 (Completed)**: Hardened `TestbedSqlplusDriver` (`testbed_sqlplus.rs`), `/nolog` stdin authentication (zero passwords in `ps`), `BoundSql` trait parameter mapping, `ROWID` selection, and complete removal of hardcoded passwords in Rust crates.
- **Phase B (Completed)**: Runtime spine & navigation engine (`crates/oracle-forms-runtime`), `FormSession`, `BlockBuffer`, `FormsRecordStatus` (`NEW`, `INSERT`, `QUERY`, `CHANGED`), `ValidationStatus`, `:SYSTEM.*` variables, trigger resolution hierarchy (`Override`, `Before`, `After`), master-detail coordination (`ORDERS` -> `ORDER_LINES`), and verified conformance suites (`conf_01_startup`, `conf_02_nav_valid`).
- **Database Testbed**: Podman container `oracle23ai-lite` is running with seeded schema tables (`ORDERS`, `CUSTOMERS`, `DEPARTMENTS`, `EMPLOYEES`, `CLAIMS`, `STOCK`, etc.).

### 3. Immediate Mission: Milestone C & Runtime UI Integration Selection
Please inspect `docs/plans/06-IMPLEMENTATION-ROADMAP-AND-MILESTONES.md` and help me execute the next milestone:

- **Milestone C (PL/SQL Front-End & Code Intelligence)**:
  1. Scaffold `crates/oracle-forms-plsql/`.
  2. Implement hand-written recursive-descent lexer and parser for PL/SQL expressions, statements, blocks, declarations (`%TYPE`, `%ROWTYPE`).
  3. Resilient statement-boundary error synchronization.
  4. Symbol table resolving across Form Program Units, attached `.pll` libraries, and `.mmb` menus.
  5. Generate call graph, event map, item cross-reference, and executability coverage report.

- **Alternative Track / Integration: Runtime UI Engine (`/api/forms/session`)**:
  1. Wire `FormSession` into `oracle-forms-server`.
  2. Expose session REST / WebSocket endpoints for active form instances.
  3. Enable interactive keyboard navigation (Tab, Shift+Tab, Up/Down, Enter), real-time field validation, and master-detail synchronization on the web canvas.

Let's review where we are, confirm git status, and proceed with the next milestone!
```

---

## Reference Architecture Documents
- [06-IMPLEMENTATION-ROADMAP-AND-MILESTONES.md](file:///Users/richardfremmerlid/Projects/oracle-forms-fmb-research/docs/plans/06-IMPLEMENTATION-ROADMAP-AND-MILESTONES.md)
- [08-PLSQL-AND-EVENT-EXECUTION.md](file:///Users/richardfremmerlid/Projects/oracle-forms-fmb-research/docs/plans/08-PLSQL-AND-EVENT-EXECUTION.md)
- [03-RUNTIME-STATE-MACHINE-AND-TRIGGERS.md](file:///Users/richardfremmerlid/Projects/oracle-forms-fmb-research/docs/plans/03-RUNTIME-STATE-MACHINE-AND-TRIGGERS.md)
- [04-DATABASE-INTEGRATION-AND-CRUD.md](file:///Users/richardfremmerlid/Projects/oracle-forms-fmb-research/docs/plans/04-DATABASE-INTEGRATION-AND-CRUD.md)
- [AGENTS.md](file:///Users/richardfremmerlid/Projects/oracle-forms-fmb-research/AGENTS.md)
