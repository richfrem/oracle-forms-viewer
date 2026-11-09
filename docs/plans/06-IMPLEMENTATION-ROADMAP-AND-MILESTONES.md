# 06: Unified Implementation Roadmap & Phased Milestones

> **Document Status**: Revised Roadmap Specification  
> **Unified Numbering Scheme**: Phases A through G, Track T, Track K

---

## 1. Unified Phased Delivery Sequence

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ Phase A: Read-Only Block Query [DONE - PR #3]                               │
│ Direct testbed database query; dynamic SELECT synthesis; UI record nav.     │
├─────────────────────────────────────────────────────────────────────────────┤
│ Phase A.1: Phase A Fix-Forward & Driver Trait Hardening [DONE]              │
│ Relabel testbed driver; BoundSql trait; remove hardcoded passwords.         │
├─────────────────────────────────────────────────────────────────────────────┤
│ Phase B: Runtime Spine (Read-Only) & Event Dispatcher [DONE]                │
│ FormSession; multi-record buffers; navigation; :SYSTEM.*; master-detail.    │
├─────────────────────────────────────────────────────────────────────────────┤
│ Phase C: PL/SQL Front-End & Code Intelligence [DONE]                        │
│ oracle-forms-plsql: AST parser; symbol table; call graph; coverage report.  │
├─────────────────────────────────────────────────────────────────────────────┤
│ Phase D: Resumable PL/SQL Interpreter & Tier-1 Built-ins [DONE]             │
│ Tree-walking interpreter; Oracle value semantics; async alert suspension.   │
├─────────────────────────────────────────────────────────────────────────────┤
│ Phase E: Transactions & Full CRUD Commit Event Pipeline [DONE]              │
│ SELECT ... FOR UPDATE NOWAIT; DML generation; commit triggers; sequences.   │
├─────────────────────────────────────────────────────────────────────────────┤
│ Phase F: Visual Layout Editor, Form Diff Tool & XML Round-Trip              │
│ 2D canvas editor; property edits; testbed-validated XML compilation.        │
├─────────────────────────────────────────────────────────────────────────────┤
│ Phase G: Reports Engine (Research Spike First)                              │
│ Feasibility investigation on .rdf ROS 60050 stream vs Reports XML exports.  │
├─────────────────────────────────────────────────────────────────────────────┤
│ Track T (Parallel): Pure-Rust Oracle Thin Driver (oracle-thin)              │
│ Zero-client wire protocol over TCP :1521 (independent parallel track).      │
├─────────────────────────────────────────────────────────────────────────────┤
│ Track K (Ongoing): Conformance Testing Kit                                  │
│ Autonomous trace logger ofw_trace and golden trace parity testing.          │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Milestone Deep-Dives

### Milestone A: Read-Only Block Query ✅
- **Delivered**: `oracle-forms-db` crate, CLI `query` command, server REST endpoints (`/api/db/test`, `/api/forms/query-block`), and visual canvas record navigation.

---

### Milestone A.1: Phase A Fix-Forward & Driver Trait Hardening ✅
- **Objective**: Fix technical debt in Phase A before building Phase B runtime dependencies.
- **Tasks**:
  1. Rename `OracleDriver` $\to$ `TestbedSqlplusDriver` (`testbed_sqlplus.rs`).
  2. Relabel UI backend selector: `Oracle (testbed sqlplus)`, `SQLite`, `Mock`.
  3. Support `OFW_TESTBED_DB_CONTAINER` environment variable (default `oracle23ai-lite`).
  4. Remove all hardcoded passwords and usernames. Pass credentials via request or env (`OFW_DB_USER`, `OFW_DB_PASSWORD`).
  5. Connect via `/nolog` with `CONNECT` sent over stdin (prevents password visibility in `ps`).
  6. Verify actual login via `SELECT 1 FROM DUAL` in `test_connection`.
  7. Add `BoundSql` to `DatabaseDriver::query` trait method.
  8. Update `SqlGenerator` to select `ROWID`, eliminate `SELECT *`, and rewrite binds in `WhereClause`.
- **Verification Gate**:
  - `grep -rn "Welcome123" crates/` returns zero matches.
  - Driver trait unit tests compile and pass with `BoundSql`.

---

### Milestone B: Runtime Spine (Read-Only) & Event Dispatcher ✅
- **Objective**: Establish the in-memory `FormSession`, multi-record buffer, and navigation state machine.
- **Tasks**:
  1. Scaffold `crates/oracle-forms-runtime/`.
  2. Implement `FormSession` and `BlockBuffer` managing records with `FormsRecordStatus` (`NEW`, `INSERT`, `QUERY`, `CHANGED`) and validation states.
  3. Implement navigation state machine (Form $\to$ Block $\to$ Record $\to$ Item).
  4. Implement system variables: `:SYSTEM.CURSOR_ITEM`, `CURSOR_BLOCK`, `CURSOR_RECORD`, `RECORD_STATUS`, `MODE`, `LAST_QUERY`.
  5. Implement trigger resolution hierarchy (`Override`, `Before`, `After`) with stub execution logging.
  6. Implement master-detail coordination from `RelationInfo`.
- **Verification Gate**:
  - Conformance scenarios `conf_01_startup` and `conf_02_nav_valid` verify trigger firing order and cursor focus.

---

### Milestone C: PL/SQL Front-End & Code Intelligence ✅
- **Objective**: Build resilient PL/SQL parser and static code analysis reports.
- **Tasks**:
  1. Scaffold `crates/oracle-forms-plsql/`.
  2. Hand-written recursive-descent parser with error synchronization on statement boundaries.
  3. AST models for statements, expressions, and declarations (`%TYPE`, `%ROWTYPE`).
  4. Symbol table resolving across Form Program Units, attached `.pll` libraries, and `.mmb` menus.
  5. Generate call graph, event map, item cross-reference, and executability coverage report.
- **Verification Gate**:
  - Successfully parses 100% of PL/SQL extracted from sample corpus (`orders_fmb.xml`, `claims_fmb.xml`, `customers_fmb.xml`, `test_db_fmb.xml`, `utils.pld`).
  - Unit and integration tests pass with zero warnings.

---

### Milestone D: Resumable PL/SQL Interpreter & Tier-1 Built-ins ✅
- **Objective**: Execute client-side triggers and built-ins with interactive async suspension.
- **Tasks**:
  1. Implement tree-walking AST interpreter inside `FormSession`.
  2. Oracle value semantics: 3-valued `NULL`, `''` is `NULL`, 38-digit decimal arithmetic, `DATE` with time component.
  3. SQL statement delegation to `DatabaseDriver` with bound variables.
  4. Tier-1 built-ins: `GO_ITEM`, `EXECUTE_QUERY`, `FIRST/NEXT/PREV/LAST_RECORD`, `SHOW_ALERT`, `MESSAGE`, `SET_ITEM_PROPERTY`, `NAME_IN`, `COPY`.
  5. Resumable interpreter: execution suspends during `SHOW_ALERT` and resumes when user clicks dialog button.
  6. Step budget and recursion guardrails.
- **Verification Gate**:
  - Conformance scenario `conf_03_query_cascade` matches native Forms 14c golden trace.
  - All unit and integration tests compile and pass with zero warnings.

---

### Milestone E: Transactions & Full CRUD Commit Event Pipeline ✅
- **Objective**: Implement transactional saves with genuine Oracle Forms locking semantics.
- **Tasks**:
  1. Implement `SELECT ... FOR UPDATE NOWAIT` locking upon first edit of a record.
  2. Implement optimistic concurrency re-check comparing database row against `initial_values`.
  3. Execute commit event pipeline (`KEY-COMMIT` $\to$ `PRE-COMMIT` $\to$ `PRE/ON/POST-INSERT/UPDATE/DELETE` $\to$ `POST-FORMS-COMMIT`).
  4. Support primary key assignment in `PRE-INSERT` triggers via `:SEQUENCE.X.NEXTVAL`.
- **Verification Gate**:
  - Conformance scenario `conf_08_commit_pipeline` (`test_conf_08_commit_pipeline_full_crud_event_sequence`, `test_conf_08_first_edit_locking_and_concurrency_conflict`, `test_conf_08_pre_commit_failure_triggers_savepoint_rollback`) verified.
  - All unit and integration tests compile and pass with zero warnings.

---

### Milestone F: Visual Layout Editor, Form Diff Tool & XML Round-Trip
- **Objective**: In-browser 2D visual editor with verified compilation round-trips.
- **Tasks**:
  1. 2D drag-and-drop layout editor for items, buttons, and frames.
  2. Property inspector for editing coordinates, prompts, and WHERE clauses.
  3. Forms2XML export serializer.
  4. Automated compilation gate in Podman testbed (`frmcmp_batch`).
  5. Visual side-by-side diff tool.
- **Verification Gate**:
  - Modifying a form in the editor, exporting to XML, and compiling in testbed produces a valid `.fmx`.

---

### Milestone G: Reports Engine (Research Spike First)
- **Objective**: Investigate `.rdf` binary stream decoding feasibility and build layout renderer.
- **Tasks**:
  1. Step 1: Research spike evaluating `.rdf` ROS 60050 decoding vs Reports XML exports.
  2. Step 2: Data model evaluator (queries, groups, summaries).
  3. Step 3: Repeating frame matrix layout calculator and HTML5/PDF renderer.
