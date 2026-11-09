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
- **Phase C (Completed)**: PL/SQL front-end & static code intelligence (`crates/oracle-forms-plsql`), recursive-descent lexer with XML unescaping, AST models, statement-boundary error synchronization, symbol resolution across blocks/items/PUs/libraries, call graph, cross references, executability reporting, and 100% corpus verification across all XML and PLD samples.
- **Phase D (Completed)**: Resumable PL/SQL tree-walking interpreter inside `FormSession` (`crates/oracle-forms-runtime`), Oracle value semantics (3-valued logic, `NULL` equality, empty string is `NULL`, numeric/string conversions), Tier-1 built-ins (`GO_ITEM`, `GO_BLOCK`, `EXECUTE_QUERY`, `FIRST/NEXT/PREV/LAST_RECORD`, `SHOW_ALERT`, `MESSAGE`, `SET_ITEM_PROPERTY`, `NAME_IN`, `COPY`, `DEFAULT_VALUE`, `ERASE`), interactive async suspension during `SHOW_ALERT` with continuation on resume, step budget and recursion guardrails, and verified conformance scenario `conf_03_query_cascade`.
- **Phase E (Completed)**: Transactions & Full CRUD Commit Event Pipeline (`crates/oracle-forms-runtime`, `crates/oracle-forms-db`), first-edit row locking (`SELECT ... FOR UPDATE NOWAIT`) with optimistic concurrency re-check against `initial_values`, complete commit pipeline (`KEY-COMMIT` -> validation -> `PRE-COMMIT` -> per-dirty-record `PRE/ON/POST-DELETE`, `PRE/ON/POST-INSERT`, `PRE/ON/POST-UPDATE` -> `POST-FORMS-COMMIT` -> DB `COMMIT` or savepoint rollback), `:SEQUENCE.<NAME>.NEXTVAL` primary key generation in `PRE-INSERT`, `COMMIT` / `COMMIT_FORM` builtins, and verified conformance suite `conf_08_commit_pipeline` (`test_conf_08_commit_pipeline_full_crud_event_sequence`, `test_conf_08_first_edit_locking_and_concurrency_conflict`, `test_conf_08_pre_commit_failure_triggers_savepoint_rollback`).
- **Database Testbed**: Podman container `oracle23ai-lite` is running with seeded schema tables (`ORDERS`, `CUSTOMERS`, `DEPARTMENTS`, `EMPLOYEES`, `CLAIMS`, `ITEMS`, `STOCK`, etc.).

### 3. Immediate Mission: Milestone F (Visual Layout Editor & XML Round-Trip) or Runtime UI Engine
Please inspect `docs/plans/06-IMPLEMENTATION-ROADMAP-AND-MILESTONES.md` and help me execute the next milestone:

- **Milestone F: Visual Layout Editor, Form Diff Tool & XML Round-Trip**:
  1. 2D drag-and-drop layout editor for items, buttons, and frames on canvas.
  2. Property inspector for editing coordinates, prompts, and block/item properties.
  3. Forms2XML export serializer for round-tripping edits back to valid XML.
  4. Automated compilation gate in Podman testbed (`frmcmp_batch` validating `.fmb` / `.fmx`).
  5. Visual side-by-side diff tool comparing original module with edited module.

- **Parallel Track: Live Runtime UI Engine (`/api/forms/session`)**:
  1. Wire `FormSession` into `oracle-forms-server` REST & WebSocket endpoints.
  2. Live interactive keyboard navigation (Tab, Shift+Tab, Up/Down, Enter), real-time field validation, and master-detail synchronization on the web canvas.
  3. Interactive alert dialogs and commit buttons on the web UI.

Let's review where we are, confirm git status, and proceed with the next milestone!
```

---

## Reference Architecture Documents
- [06-IMPLEMENTATION-ROADMAP-AND-MILESTONES.md](file:///Users/richardfremmerlid/Projects/oracle-forms-fmb-research/docs/plans/06-IMPLEMENTATION-ROADMAP-AND-MILESTONES.md)
- [08-PLSQL-AND-EVENT-EXECUTION.md](file:///Users/richardfremmerlid/Projects/oracle-forms-fmb-research/docs/plans/08-PLSQL-AND-EVENT-EXECUTION.md)
- [03-RUNTIME-STATE-MACHINE-AND-TRIGGERS.md](file:///Users/richardfremmerlid/Projects/oracle-forms-fmb-research/docs/plans/03-RUNTIME-STATE-MACHINE-AND-TRIGGERS.md)
- [04-DATABASE-INTEGRATION-AND-CRUD.md](file:///Users/richardfremmerlid/Projects/oracle-forms-fmb-research/docs/plans/04-DATABASE-INTEGRATION-AND-CRUD.md)
- [AGENTS.md](file:///Users/richardfremmerlid/Projects/oracle-forms-fmb-research/AGENTS.md)
