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
│ Phase F: Visual Layout Editor, Form Diff Tool & XML Round-Trip [DONE]       │
│ 2D canvas editor; property edits; testbed-validated XML compilation.        │
├─────────────────────────────────────────────────────────────────────────────┤
│ Phase G: Reports Engine (Research Spike & Renderer) [DONE]                  │
│ Native Reports XML parser, data model evaluator, HTML5/PDF paged media.     │
├─────────────────────────────────────────────────────────────────────────────┤
│ Track T (Parallel): Pure-Rust Oracle Thin Driver (oracle-thin) [DONE]          │
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
  - `grep -rn "$DB_PASSWORD" crates/` returns zero matches.
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

### Milestone F: Visual Layout Editor, Form Diff Tool & XML Round-Trip ✅
- **Objective**: In-browser 2D visual editor with verified compilation round-trips.
- **Tasks**:
  1. 2D drag-and-drop layout editor for items, buttons, and frames with coordinate snap-to-grid.
  2. Property inspector for editing coordinates, prompts, and WHERE clauses.
  3. Forms2XML export serializer (`FormsXmlWriter`).
  4. Form structural & visual diff engine (`FormDiffer`, `ModuleDiff`) and CLI `diff` tool.
  5. In-browser side-by-side diff tool view.
- **Verification Gate**:
  - Round-trip fidelity test (`test_xml_round_trip_and_diff_fidelity`) passes with 100% attribute preservation.
  - All unit and integration tests compile and pass with zero warnings.

---

### Milestone G: Reports Engine (Research Spike & Renderer) ✅
- **Objective**: Investigate `.rdf` binary stream decoding feasibility and build layout renderer.
- **Tasks & Deliverables**:
  1. **Research Spike Completed**: Evaluated ROS 60050 chunk structures in `.rdf`/`.tdf` templates; confirmed heuristic text/PL-SQL extraction feasibility while adopting pure-native Reports XML (`reports.dtd`) for complete, lossless layout and query model parsing.
  2. **Reports AST & XML Parser**: Built `oracle-forms-core::report` AST models and `oracle-forms-xml::ReportsXmlParser` streaming quick-xml parser.
  3. **Data Model Evaluator**: Built `ReportDataEvaluator` executing queries via `DatabaseDriver` (Oracle 23ai, SQLite, Mock), resolving master-detail data links, calculating formulas (`CF_`), and accumulating summaries (`CS_` sum, count, average, min, max, median, percent of total).
  4. **HTML5 Paged Media Renderer**: Built `ReportRenderer` producing semantic, print-ready HTML5 with CSS Paged Media (`@page { size: letter portrait; margin: 0.5in; }`), format masks (`$#,##0.00`), and master-detail card layouts.
  5. **CLI & Server Tooling**: Added `oracle-forms-cli report inspect` & `report run` subcommands, server REST endpoints (`/api/reports`, `/api/reports/run`, `/api/reports/:name/html`), and web viewer "📊 Reports Engine" tab with live iframe preview and data model inspector.
- **Verification Gate**:
  - Sample report suites (`orders_summary_report.xml`, `department_salaries_report.xml`) pass all integration tests (`test_orders_summary_report_parsing_and_evaluation`, `test_department_salaries_master_detail_report`).
  - Verified live execution against Oracle Database 23ai container (`oracle23ai-lite`).
  - All unit and integration tests compile and pass with zero clippy warnings.

---

### Track T: Pure-Rust Oracle Thin Driver (`oracle-thin`) ✅
- **Objective**: Implement native, zero-client Oracle Database wire protocol over TCP :1521 without JVM, C-libraries, Instant Client, or SQL*Plus container execution.
- **Tasks & Deliverables**:
  1. **Adopt Official `oracledb` Pure-Rust Driver**: Integrated Oracle's official `oracledb = "26.0.0-beta.5"` implementing Sans-I/O TNS/TTC wire protocol over Tokio TCP with full support for Oracle 19c/21c/23ai verifiers (O5LOGON, PBKDF2/SHA-512), parameter bindings, and streaming cursors.
  2. **`OracleThinDriver` (`crates/oracle-forms-db/src/thin.rs`)**: Built thread-safe pooled driver implementing `DatabaseDriver` trait (`test_connection`, `query`, `backend`).
  3. **Data Type Conversion & Extraction**: Dynamic column mapping for Oracle `NUMBER`, `CHAR`/`VARCHAR2`, `DATE`, `TIMESTAMP` (`OracleTimestamp`), and `ROWID` with automatic JSON conversion.
  4. **Dynamic Parameter Binding**: Supports both named (`:b1`) and positional (`:1`) binds with `SqlValue` serialization into `&dyn ToDbValue`.
  5. **CLI & Server Integration**: Added `oracle-thin` backend support to CLI (`oracle-forms-cli query --backend oracle-thin`, `report run --backend oracle-thin`), server routes, and web UI dropdowns.
- **Verification Gate**:
  - Live query against running `oracle23ai-lite` container executes in 8 ms.
  - Integration tests `test_oracle_thin_live_container_query` and `test_oracle_thin_parameterized_query` pass.
  - Zero clippy warnings and 100% test pass rate across workspace.


### UI follow-up: report inspection, parameters, and save feedback

Implemented against the Phase F/G workbench intentions:

- `GET /api/reports/{name}/definition` exposes the parsed report without database execution.
- The Reports UI provides declared parameters and defaults before execution. The evaluator
  resolves case-insensitive overrides through `BoundSql`, validates numeric parameters, and
  treats empty values as NULL. An unresolved undeclared SQL bind produces an error.
- Reports start with explicitly labelled sample data selected. Changing parameters or the
  backend clears the previous output; late responses cannot replace the current selection.
- Source inspection displays extracted formulas and SQL, without invented lifecycle triggers
  or formula bodies. Data links are labelled as declarative metadata.
- Form property and drag saves check HTTP failures and show unsuccessful persistence.
- Report data-model export includes parameters and explicitly excludes paper layout and
  unparsed metadata; it is not a lossless Oracle Reports round trip.

Try `samples/parameterized_items_report.xml` with SQLite or Mock. Its minimum amount defaults
to 25; setting it to 100 returns only the seeded item with amount 150.

Validation: repository-root `cargo check` and `cargo test`; optional frontend regressions
with `node --test tests/ui_reports.test.cjs` (Node is a test tool only, not a workbench dependency).
Visual browser verification and live Oracle parameter parity remain unverified for this follow-up.


### Native form workflow continuation — 2026-11-12

- Oracle Run prompts for credentials; live native Oracle startup/button execution verified.
- New/add/replace wizard actions implemented; misplaced fresh-form code corrected; creation
  and preview verified in Chrome. Run synchronizes edits before session startup.
- Live table/view schema discovery added for Oracle and SQLite, with bound parameters;
  a form built from discovered TEST_ITEMS columns queried Oracle successfully in read-only mode.
- Diff supports a baseline against multiple targets, uploaded snapshots, item geometry overviews,
  and parsed-model section changes (including program units and added/removed layout objects).
- Toolbar color/font handlers now match the actual HTML controls. Failed saves can be retried.
- Current detailed evidence, limitations and unfinished work are maintained in
  [10-NEXT-PHASE-SESSION-PROMPT.md](10-NEXT-PHASE-SESSION-PROMPT.md).

These improvements do not complete Track K, automatic middleware compilation/deployment,
full runtime visual parity, or lossless XML rewriting after arbitrary structural edits.


### Disk save, Oracle preview and responsive chrome — 2026-11-12

Implemented:

- Separate **Save Form** (new XML version under samples) and **Save Record** (database commit).
- **Preview in Oracle** saves the current module, converts XML to FMB, compiles/deploys FMX
  through optional Track 1 tooling and launches interactive FSAL, with job status/log feedback.
  Binary generation is staged under ignored temp directories; originals are retained.
- Correct XML2Forms `_fmb` filename handling and credential-aware compilation.
- Wrapping header/view navigation, View & panels menu, More form tools menu, and visible
  Save Form / Preview in Oracle actions. Desktop layout checked in Chrome.

Verified: durable edited XML, non-overwriting save versions, TEST_DB conversion/compilation,
FSAL session establishment, 119 Rust tests, 10 frontend tests, and two reference-runner tests.
Employee Directory also converted/compiled and established a separate FSAL session; runtime logon and visual equivalence are not yet certified.

Outstanding: lossless arbitrary structural XML changes, UI-only property persistence (including
fonts), full dependency compilation/configuration/cancellation for reference previews, small
viewport/accessibility coverage, full Oracle runtime parity and Track K. See the authoritative
current evidence and next steps in [10-NEXT-PHASE-SESSION-PROMPT.md](10-NEXT-PHASE-SESSION-PROMPT.md).


### Oracle preview onboarding

The launcher includes setup/startup instructions and read-only dependency checks for Python,
Java 17+, Podman, Forms container/servlet and reference files. Missing prerequisites disable
launch; users can recheck after completing setup. Installation and container startup remain
explicit operator actions. Readiness does not validate schema credentials or compilation.
Validation includes three frontend setup-state regressions (13 frontend tests total).


### Logon feedback follow-up

Connect & Run now names the missing credential, avoids a misleading dotted password
placeholder, shows progress, accepts Enter, and reports already-running sessions. The
classic logon dialog is centered with responsive fields. Browser verification with an actual
password started Employee Directory via Oracle Thin (writes enabled by the user; no commit
performed). See the session prompt document for post-PR publication status and remaining work.


### Oracle preview dialog usability follow-up

Launch status, setup checks, diagnostics and manual FMX tools now have distinct sections.
Logs wrap inside a bounded diagnostics panel. Changing forms clears unrelated launch output,
including late asynchronous responses. Desktop dialog checked in Chrome; 17 frontend checks
pass, including stale-preview and stylesheet-integrity regressions. These follow-up changes
are local until separately committed; current status is in the session prompt document.


### Hand-authored Orders XML compatibility

Oracle preview prepares an isolated compatibility copy for known fixture aliases, version
format and missing native-default layout metadata; originals are preserved and changes logged.
Rust parameter/group-column serialization now uses Oracle schema names. Orders XML2Forms
conversion and database-connected FMX compilation were verified on Forms 14c. Unknown business
nodes are retained for Oracle validation; this is not a general schema migration or parity claim.
The app launch established FSAL session AdminServer.formsapp.18 for orders_edited_3.fmx;
user logon and visual/runtime parity remain unverified. See the session prompt for follow-up status.


### Persist resolved designer layout for Oracle preview

Fallback item layout is shared by the SVG renderer and Save Form / Preview persistence.
The saved XML now includes inferred positions/sizes and button labels; source fixture files,
explicit positions and hidden flags are preserved. Coordinate units are accounted for. Orders
compiled/launched with the persisted layout; final native visual confirmation remains a user
check. Validation includes 20 frontend tests and 119 Rust tests. Compilation alone is not parity.


### Orders visual comparison follow-up

Saved preview XML now carries the designer's inferred button caption even after positions are
already explicit; a caption-only change preserves geometry. Inferred block outlines are shown
only in Layout mode. Caption persistence was verified on saved Orders XML; 21 frontend tests
pass. Native typography/theme/DPI, actual source trigger behavior and pixel parity remain open.


### Explicit compiler connection mode

Preview defaults to database-connected compilation and requests dedicated compile credentials;
offline compilation is an explicit option. This fixes OFW_SALES failing under logon=no after
Run cleared the Logon password. The same FMB compiled successfully with logon=yes. The API has
matching validation; an existing server needs restart for the new backend guard. Current tests:
120 Rust, 23 frontend and 7 reference-runner tests. Runtime logon remains separate.


### Review milestone: XML form design across native and Oracle runtimes

The follow-up PR captures the complete implemented workflow: design/modify a form, persist a
versioned XML artifact, load/execute its model in the native Rust runtime, and use the optional
Oracle toolchain to convert/compile and launch it in Forms 14c FSAL. Saved XML versions from
this work are included for review. Live Orders, Employee Directory and Sales evidence is
recorded in the session prompt document. Full runtime semantics, visual parity, dependency
handling and lossless arbitrary structural changes remain bounded by the limitations there.
Publication is without rerunning verification at the user's request; PR review/merge is manual.
