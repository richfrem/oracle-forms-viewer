# Implementation Plan: Oracle Forms Workbench

> **Document Status**: Revised Implementation Plan (Post-Review Alignment)  
> **Location**: `temp/plan/IMPLEMENTATION-PLAN.md`  
> **Execution Order**: Phase A ✅ $\to$ Phase A.1 $\to$ Phase B $\to$ Phase C $\to$ Phase D $\to$ Phase E $\to$ Phase F $\to$ Phase G  
> **Parallel Tracks**: Track T (Pure-Rust Thin Driver), Track K (Conformance Kit)

---

## 1. Unified Roadmap Overview

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ Phase A: Read-Only Block Query [COMPLETED & MERGED TO MAIN (PR #3)]         │
│ Direct connection to testbed Oracle 23ai & SQLite; block SELECT generation; │
│ UI Query DB button and record navigation controls.                          │
├─────────────────────────────────────────────────────────────────────────────┤
│ Phase A.1: Phase A Fix-Forward & Driver Trait Hardening                     │
│ Relabel testbed sqlplus driver; BoundSql trait; remove hardcoded passwords; │
│ select ROWID; parse binds in WHERE clauses; secure server defaults.        │
├─────────────────────────────────────────────────────────────────────────────┤
│ Phase B: Runtime Spine (Read-Only) & Event Dispatcher                       │
│ Crate oracle-forms-runtime: FormSession, multi-record buffers (NEW/INSERT/  │
│ QUERY/CHANGED), navigation state machine, :SYSTEM.* variables, trigger      │
│ resolution (item/block/form hierarchy), stub event logging, master-detail.  │
├─────────────────────────────────────────────────────────────────────────────┤
│ Phase C: PL/SQL Front-End & Code Intelligence                               │
│ Crate oracle-forms-plsql: Resilient recursive-descent parser, AST model,    │
│ symbol table (Form + PLL + MMB + OLB), call graph, event map, cross-ref,   │
│ and executability coverage report. No execution in this phase.              │
├─────────────────────────────────────────────────────────────────────────────┤
│ Phase D: Resumable PL/SQL Interpreter & Tier-1 Built-ins                    │
│ Tree-walking interpreter in FormSession; Oracle 38-digit decimal, 3-valued  │
│ NULL, and DATE semantics; SQL delegation; Tier-1 built-ins; resumable       │
│ async execution for interactive alerts (SHOW_ALERT); safety budgets.        │
├─────────────────────────────────────────────────────────────────────────────┤
│ Phase E: Transactions & Full CRUD Commit Event Pipeline                     │
│ Lock mode (SELECT ... FOR UPDATE NOWAIT); DML generation; commit pipeline   │
│ triggers (PRE/ON/POST-INSERT/UPDATE/DELETE); sequence assignment; rollback. │
├─────────────────────────────────────────────────────────────────────────────┤
│ Phase F: Visual Layout Editor, Form Diff Tool & XML Round-Trip              │
│ 2D canvas editor; property edits; round-trip XML serialization validated by │
│ compilation in local testbed (frmcmp_batch).                                │
├─────────────────────────────────────────────────────────────────────────────┤
│ Phase G: Reports Engine (Research Spike First)                              │
│ Feasibility spike on ROS 60050 binary .rdf decoding vs Reports XML; matrix  │
│ repeating frames; printable HTML5/PDF generation.                           │
├─────────────────────────────────────────────────────────────────────────────┤
│ Track T (Parallel): Pure-Rust Oracle Thin Driver (oracle-thin)              │
│ TNS packet wire format, TTC protocol, O5LOGON/verifier authentication.      │
├─────────────────────────────────────────────────────────────────────────────┤
│ Track K (Ongoing): Conformance Testing Kit                                  │
│ Automated golden trace validation against genuine Oracle Forms 14c testbed. │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Phase Deep-Dives

### Phase A: Read-Only Block Query ✅
- **Status**: Completed and merged into `main` (`d76ff4a`, PR #3).
- **Serves**: Analyst / Modernizer.
- **Delivered**: `crates/oracle-forms-db/`, CLI `query` command, server endpoints (`/api/db/test`, `/api/forms/query-block`), and visual canvas record navigation.
- **Architecture Note**: The Oracle query path connects to the local Podman testbed container (`oracle23ai-lite`) using `sqlplus` wrapping JSON objects.

---

### Phase A.1: Phase A Fix-Forward & Driver Trait Hardening
- **Serves**: All personas (Security, Safety & Clean Driver Abstraction).
- **Depends on**: Phase A.
- **Out of Scope**: Transaction commit pipelines or PL/SQL evaluation.
- **Key Deliverables**:
  1. **Relabel Testbed Driver**:
     - Rename `OracleDriver` $\to$ `TestbedSqlplusDriver` (`testbed_sqlplus.rs`).
     - UI and docs label: *"Oracle (testbed sqlplus) - Requires container oracle23ai-lite. Not a native driver."*
     - Container name configurable via `OFW_TESTBED_DB_CONTAINER` (default: `oracle23ai-lite`).
  2. **Security & Credentials**:
     - Remove all hardcoded usernames and passwords. If credentials are not supplied, return `DbError::ConfigError("Username and password required")`.
     - Credentials passed dynamically via request or environment variables (`OFW_DB_USER`, `OFW_DB_PASSWORD`).
     - Eliminate command-line passwords: pass `CONNECT user/pass@service` over standard input with `sqlplus -s /nolog`.
     - `test_connection` executes `SELECT 1 FROM DUAL` to verify actual login, not just TCP listener ping.
  3. **Driver Trait Bound Parameters**:
     - Introduce `BoundSql`:
       ```rust
       pub enum SqlValue {
           Null,
           Text(String),
           Number(String),
           Date(String),
       }
       pub struct BoundSql {
           pub sql: String,
           pub binds: Vec<(String, SqlValue)>,
       }
       ```
     - Update `DatabaseDriver::query` to take `&BoundSql`. SQLite binds parameters natively; `TestbedSqlplusDriver` formats bound script parameters.
  4. **SQL Generator Hardening**:
     - For Oracle dialect, select `ROWID` as the first column (kept hidden from UI) for Phase E locking.
     - Never fall back to `SELECT *`. Return a typed error if a block has no queryable items.
     - Scan `WhereClause` for `:BLOCK.ITEM`, `:GLOBAL.X`, `:PARAMETER.X`, rewrite them to dialect binds, and return unresolved references as a typed error.
     - Add Query-By-Example (QBE) criteria input: `Vec<(item, operator, SqlValue)>`.
- **Verification Gates**:
  - `grep -rn "$DB_PASSWORD" crates/` returns zero matches.
  - Driver trait compiles across SQLite, Mock, and TestbedSqlplus drivers with `BoundSql`.
  - Generator unit tests cover `ROWID` selection, bind extraction, string literal preservation (`':not_a_bind'`), and no `SELECT *`.

---

### Phase B: Runtime Spine (Read-Only) & Event Dispatcher
- **Serves**: Analyst / Modernizer, Developer.
- **Depends on**: Phase A.1.
- **Out of Scope**: Full PL/SQL execution (triggers are resolved and dispatched to a stub logger, not interpreted yet).
- **Key Deliverables**:
  1. **New Crate `oracle-forms-runtime`**:
     - `FormSession`: Represents an active running form instance, holding block buffers, cursor focus, mode, and session settings.
     - Multi-Record Buffer: Records stored per block with status tracking (`NEW`, `INSERT`, `QUERY`, `CHANGED`) and item validation statuses (`NEW`, `CHANGED`, `VALID`).
     - Navigation State Machine: Tracking `Form` $\to$ `Block` $\to$ `Record` $\to$ `Item` cursor transitions.
     - System Variables: Full implementation of `:SYSTEM.CURSOR_ITEM`, `CURSOR_BLOCK`, `CURSOR_RECORD`, `RECORD_STATUS`, `BLOCK_STATUS`, `FORM_STATUS`, `MODE`, `LAST_QUERY`.
  2. **Trigger Resolution & Event Dispatcher**:
     - Trigger resolution: Item $\to$ Block $\to$ Form level.
     - Execution Hierarchy support: `Override`, `Before`, `After`.
     - Event Dispatcher: Emits trace events (`PRE-BLOCK`, `POST-CHANGE`, `WHEN-VALIDATE-ITEM`, `POST-QUERY`).
     - In Phase B, the trigger action executes via a **Stub Executor**: logs the firing order and system variable snapshot to trace logs without running PL/SQL.
  3. **Master-Detail Coordination**:
     - Evaluates `RelationInfo` declarative relations (e.g. `ORDERS` $\to$ `ORDER_ITEMS`).
     - Automatically coordinates detail block queries when master record changes.
- **Verification Gates**:
  - Conformance scenario `conf_01_startup` and `conf_02_nav_valid` verify trigger firing order and focus state.
  - Master-detail test verifying detail block queries update when scrolling through master records.

---

### Phase C: PL/SQL Front-End & Code Intelligence
- **Serves**: Reviewer / Maintainer, Analyst / Modernizer.
- **Depends on**: `oracle-forms-core` (can be developed in parallel with Phase B).
- **Out of Scope**: PL/SQL bytecode generation or virtual machine execution.
- **Key Deliverables**:
  1. **New Crate `oracle-forms-plsql`**:
     - Lexer & Tokenizer: Forms-specific PL/SQL tokenization supporting bind variables (`:BLOCK.ITEM`, `:GLOBAL.X`, `:SYSTEM.X`), literals, and keywords.
     - Recursive-Descent Parser with Error Recovery: Synchronization on statement boundaries (`END;`, `;`) so syntax quirks in one trigger do not discard the module.
     - AST Representation: Statements (`IF`, `CASE`, `LOOP`, `ASSIGN`, `CALL`, `RAISE`), expressions, declarations (`%TYPE`, `%ROWTYPE`, record, table), and exception handlers.
     - Embedded SQL: Captured as opaque SQL spans with extracted bind parameters and `INTO` target variables.
  2. **Symbol Table & Scope Resolver**:
     - Resolves scopes across: Form Program Units, Attached Libraries (`.pll`/`.pld` in attachment order), Menu code (`.mmb`), and built-ins catalog.
  3. **Analysis Reports & Exports**:
     - **Call Graph**: Complete call tree from triggers to procedures to library packages.
     - **Event Map**: Chronological listing of triggers firing per widget event.
     - **Item Cross-Reference**: Index of all triggers reading or writing specific fields.
     - **Executability Coverage Report**: Every statement classified as `Supported`, `Delegated-to-DB`, or `Unsupported (reason)` with line numbers. Output as JSON and HTML.
- **Verification Gates**:
  - Parses 100% of PL/SQL extracted from `orders_fmb.xml`, `claims_fmb.xml`, and `samples/utils.pld` with zero parser crashes.
  - Call graph accurately identifies cross-unit calls to `utils.pld`.

---

### Phase D: Resumable PL/SQL Interpreter & Tier-1 Built-ins
- **Serves**: Analyst / Modernizer, Developer.
- **Depends on**: Phase B, Phase C, Phase A.1.
- **Out of Scope**: Legacy OS host calls (`HOST`, `OLE2`), reports execution (`RUN_REPORT_OBJECT`).
- **Key Deliverables**:
  1. **AST Tree-Walking Interpreter**:
     - Evaluates statements inside active `FormSession`.
     - Oracle Value Semantics: Three-valued `NULL` logic, `''` is `NULL`, 38-digit decimal numbers, `DATE` with time component and session NLS formatting.
     - Exception Handling: Support for `EXCEPTION WHEN ... THEN ...`, `RAISE`, and `RAISE FORM_TRIGGER_FAILURE`.
  2. **SQL Delegation**:
     - `SELECT ... INTO`, DML, and cursor iterations delegated to `DatabaseDriver` with bound parameter resolution.
     - Map database exceptions: `NO_DATA_FOUND`, `TOO_MANY_ROWS`, `DUP_VAL_ON_INDEX`.
  3. **Tier-1 Built-ins Catalog**:
     - Navigation: `GO_ITEM`, `GO_BLOCK`, `NEXT_ITEM`, `PREVIOUS_ITEM`, `NEXT_RECORD`, `PREVIOUS_RECORD`, `FIRST_RECORD`, `LAST_RECORD`.
     - Query & Records: `EXECUTE_QUERY`, `ENTER_QUERY`, `CLEAR_BLOCK`, `CLEAR_FORM`, `CREATE_RECORD`, `DELETE_RECORD`, `DO_KEY`.
     - Messages & Alerts: `MESSAGE`, `SHOW_ALERT`, `SET_ALERT_PROPERTY`.
     - Properties: `GET_ITEM_PROPERTY`, `SET_ITEM_PROPERTY`, `GET_BLOCK_PROPERTY`, `SET_BLOCK_PROPERTY`.
     - Values: `NAME_IN`, `COPY`, `DEFAULT_VALUE`, `ERASE`.
     - LOVs: `SHOW_LOV`, `LIST_VALUES`, `FIND_GROUP`, `POPULATE_GROUP`.
  4. **Resumable Execution Model**:
     - Async continuation model: When interactive built-ins (`SHOW_ALERT`) are called, execution suspends, requests user response from UI, and resumes upon button click.
  5. **Safety Guardrails**:
     - Step budget per trigger (100,000 steps), call stack depth limit (64), and read-only database guard.
- **Verification Gates**:
  - Conformance scenario `conf_03_query_cascade` and `conf_05_key_override` match native Forms 14c golden traces.
  - Interactive test: `SHOW_ALERT` halts execution, waits for user selection, and takes appropriate `IF` branch.

---

### Phase E: Transactions & Full CRUD Commit Event Pipeline
- **Serves**: Modernizer, Developer.
- **Depends on**: Phase D.
- **Out of Scope**: Distributed 2-phase commit (2PC) across multiple databases.
- **Key Deliverables**:
  1. **Forms Locking Semantics**:
     - Implements `SELECT ... FOR UPDATE NOWAIT` upon first edit of a queried record.
     - Checks that queried columns match current database state before modifying; raises `FRM-40501: Unable to reserve record for update or delete` on conflict.
     - Supports `Lock Mode` (Immediate / Delayed / Automatic).
  2. **Commit Event Pipeline**:
     - Executes full transactional cascade:
       `KEY-COMMIT` $\to$ Validation $\to$ `PRE-COMMIT` $\to$ For each dirty record:
       - If `INSERT`: `PRE-INSERT` $\to$ `ON-INSERT` (or default INSERT) $\to$ `POST-INSERT`
       - If `CHANGED`: `PRE-UPDATE` $\to$ `ON-UPDATE` (or default UPDATE) $\to$ `POST-UPDATE`
       - If `MARKED_DELETE`: `PRE-DELETE` $\to$ `ON-DELETE` (or default DELETE) $\to$ `POST-DELETE`
       $\to$ `POST-FORMS-COMMIT` $\to$ Database `COMMIT`.
  3. **Sequences & Key Assignment**:
     - Resolves primary keys assigned in `PRE-INSERT` triggers via `:SEQUENCE.X.NEXTVAL` or DB triggers.
     - Handles `RETURNING ROWID INTO :bind` for Oracle updates.
- **Verification Gates**:
  - Conformance scenario `conf_08_commit_pipeline` matches native Forms 14c golden trace.
  - End-to-end CRUD: Insert, update, and delete rows in `test_items` via the web UI; verify database records accurately reflect mutations.

---

### Phase F: Visual Layout Editor, Form Diff Tool & XML Round-Trip
- **Serves**: Developer (Light Edits).
- **Depends on**: Phase C (PL/SQL AST) and `oracle-forms-core`.
- **Key Deliverables**:
  1. **Visual Canvas 2D Drag-and-Drop Editor**:
     - Move and resize buttons, text items, frames, prompts, and graphics directly on the canvas.
     - Property Sheet: Edit labels, coordinates, data lengths, WHERE clauses, and item types.
  2. **PL/SQL Trigger Editor**:
     - In-browser code editor with syntax highlighting, autocomplete for block items and built-ins, and live Phase C diagnostic checks.
  3. **Validated XML Round-Trip Export**:
     - Serializes modified `ModuleIndex` back to Forms2XML `.xml`.
     - Validated via automated testbed compilation: `frmcmp_batch` compiles exported XML to `.fmx` with zero compiler errors.
  4. **Visual Diff Tool**:
     - Side-by-side graphical diff showing layout shifts and text diffs between form revisions.
- **Verification Gates**:
  - Modify `test_db.fmb` in the web editor, export to XML, compile in Forms 14c testbed, and run the resulting `.fmx` via FSAL.

---

### Phase G: Reports Engine (Research Spike First)
- **Serves**: Analyst / Modernizer.
- **Depends on**: Phase B.
- **Key Deliverables**:
  1. **Research Spike**:
     - Investigate ROS 60050 binary stream structures in `.rdf` files versus Reports XML exports.
     - Determine whether binary `.rdf` decoding is achievable within budget or if Reports XML is required as the baseline input.
  2. **Reports Data Model Evaluator**:
     - Queries, groups, summary columns, formula columns.
  3. **Layout & Print Engine**:
     - Matrix layout generator with repeating frames.
     - Printable HTML5 and PDF export via `printpdf`.
- **Verification Gates**:
  - Research spike decision document completed before writing rendering code.

---

### Track T (Parallel): Pure-Rust Oracle Thin Driver (`oracle-thin`)
- **Serves**: All personas (Zero-client, pure-native wire connectivity).
- **Independent Parallel Track**: Does not block Phase B, C, D, or E.
- **Deliverables**:
  - New crate `crates/oracle-thin/`.
  - TNS packet framing, TTC protocol negotiation, O5LOGON authentication (12C/PBKDF2 verifiers), and data type serialization over async TCP.
- **Promotion Gate**: Passes 100% of `oracle-forms-db` integration tests against Oracle 23ai Free before replacing the testbed adapter.

---

### Track K (Ongoing): Conformance Testing Kit
- **Serves**: Quality Assurance & Ground-Truth Parity.
- **Deliverables**:
  - `samples/conformance/`: Test forms `conf_01_startup` through `conf_08_commit_pipeline`.
  - Autonomous trace logger `ofw_trace` logging trigger firing order, system variables, and record statuses.
  - Golden trace capture from native Oracle Forms 14c testbed and headless scenario runners.
