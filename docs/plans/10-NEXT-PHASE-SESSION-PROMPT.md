# 10: Next Phase Session Starter Prompt

> **Purpose**: Copy and paste the prompt below into a new/clean AI coding session to immediately resume work on the next phase of the Oracle Forms Workbench roadmap.

---

## Active work status — 2026-11-12

This section supersedes the historical completion claims below. Continue from the repository
state reported by `git status` and `git log`. No exploration interview is needed.

### Launcher setup guidance follow-up

Preview in Oracle now opens with a dependency checklist and expandable first-time setup and
startup instructions. Readiness is checked on the Rust server host via
`GET /api/forms/oracle-preview/readiness`: Python, Java 17+, Podman, running Forms container,
HTTP servlet response, FSAL JAR and launcher script. Missing required checks disable Launch;
Check setup again repeats the read-only probes. The local Oracle Free container is reported
separately because database-free/offline compilation can be used without it.

Instructions explain Oracle installer/licensing prerequisites, configured domain requirements,
Podman machine/container startup, log inspection, health checks, native viewer startup, and
separate compile/runtime logons. Checks do not install anything, start services, authenticate
to the database, or certify compiler success. Java commands run with a four-second timeout.
The newest server on 7075 was rebuilt/restarted to expose this endpoint; existing FSAL desktop
windows run independently, but old preview-job history in the Rust process is not retained.

Validation: 119 Rust tests and 13 frontend tests passed; missing dependencies, available setup,
and unavailable readiness-endpoint handling are covered by tests/ui_oracle_setup.test.cjs.

### Completed implementation

- **Save Form** is a separate designer action. `POST /api/forms/save` writes a new durable
  XML version into `samples/<module>_edited_<version>_fmb.xml`. `create_new` prevents overwrite;
  the existing XML/FMB is preserved. A disk-save failure is reported rather than showing success.
- **Save Record** is labelled separately on the runtime toolbar and commits database records.
  It does not save the form definition. Native runtime commits remain opt-in through Allow Writes.
- **Preview in Oracle** opens a dialog explicitly naming the current module. **Save & launch
  current form in Oracle** synchronizes browser edits, saves XML, converts it with XML2Forms,
  compiles FMX in Forms 14c, deploys that unique executable and starts desktop FSAL.
- The optional reference integration is isolated from native parsing/runtime. It requires
  Python, Podman, the existing Forms 14c container and host Java/FSAL. Generated binary files
  are staged under ignored `temp/oracle-preview/<job>/`; source FMBs are not replaced.
- Preview uses Logon username/password for database-connected compilation against the local
  FREEPDB1 alias, if supplied; otherwise it compiles offline. FSAL always prompts for runtime
  logon. Credentials are not saved to artifacts or launcher arguments. Compilation script
  travels over standard input to the container; diagnostic output redacts the logon string.
- Background preview status/log endpoints report compile errors in the dialog. Up to four
  previews may run at once. Closing the dialog does not kill an Oracle desktop session.
- Fixed reference-runner output naming: XML2Forms strips `_fmb` before producing `.fmb`.
  The runner now checks that the binary actually exists before trying compilation.
- Header redesigned into wrapping module controls and a separate view-navigation row.
  **View & panels** groups panel controls; **More form tools** groups secondary form actions.
  Designer/format toolbars wrap instead of requiring horizontal scrolling. Menus close on
  outside click and Escape. Chrome screenshot verified the resulting desktop header.
- Fixed toolbar color/font selectors to match actual HTML IDs and preset attributes. Chrome
  verified Primary Blue updates Search fill to #2563eb and text to #ffffff in the palette.
- Retry save resends in-memory edits after a persistence/network error without reloading.
- Oracle Run prompts for missing credentials; connection failures remain visible in Logon.
  Server-environment credential use requires explicit server opt-in.
- Wizard create/add/replace actions wired; misplaced new-form branch corrected. Chrome created
  UI_VERIFIED_FORM with its own window/canvas. Run synchronizes the edited model first.
- Live schema discovery (`POST /api/db/describe`) supports Oracle USER_TAB_COLUMNS and SQLite,
  with bound values. Wizard Read schema populates columns from the connected database.
- Diff accepts mixed model/name inputs and multiple uploaded snapshots. Baseline comparisons
  are pairwise; layout overviews use shared bounds. Supplemental parsed-model section changes
  cover added/removed canvases/windows, program units, and previously missed properties.
  Trigger identity includes block and item scope. Source-location metadata is ignored.
- Reports: offline definition inspection; bound parameters/defaults/validation; actual extracted
  source; stale-result suppression; accurately labelled data-model-only export.

### Verification evidence

- `cargo check` and `cargo test`: **119 Rust tests**, zero warnings in the latest completed run.
- `node --test tests/ui_*.test.cjs`: **10 frontend tests** passed.
- `python3 tests/test_reference_pipeline.py`: **2 tests**, including three naming scenarios.
- Saved TEST_DB XML to samples and confirmed an updated button color (#123456) was in the file.
  Separate temporary-directory tests verify file persistence and non-overwriting version creation.
- Local Oracle Thin runtime: TEST_DB startup returned Widget A; its button trigger returned
  Gadget B with no error, read-only. A generated-style form queried four TEST_ITEMS records.
- Oracle schema discovery returned ITEM_ID, ITEM_NAME, AMOUNT, CREATED_AT. Three same-name
  form variants compared successfully through the diff API.
- **Oracle reference pipeline:** TEST_DB XML conversion and database-connected compilation
  succeeded; FSAL established Forms session AdminServer.formsapp.7. The user logged into it.
  This was a TEST_DB verification window, not Employee Directory.
- **OFW_DEMO_EMPLOYEES** preview saved `samples/ofw_demo_employees_edited_1_fmb.xml`,
  converted/compiled `ofw_demo_employees_edited_1.fmx`, launched FSAL and established
  Forms session AdminServer.formsapp.8. Job 60286-2 on port 7075. Runtime logon and visual
  equivalence remain to be confirmed by the user.

### Outstanding work and known limits

1. Confirm Employee Directory FSAL logon and side-by-side visual comparison (compilation and session establishment passed);
   verify current browser edits are reflected when launching through its Preview button.
2. Model-summary exports are **not lossless**: arbitrary structural edits cannot preserve all
   Oracle properties. The UI reports the export mode. Original XML plus item patches is used
   when available. Fonts and other UI-only properties not represented in the Rust model need
   end-to-end persistence work; color/geometry/prompt changes are represented.
3. Native Rust does not write original Oracle FMB binaries. Save Form persists XML; the
   optional Oracle preview toolchain produces FMB/FMX in its staging directory.
4. Verify append/replace behavior comprehensively, uploaded same-name form comparisons and
   small-window/mobile layouts. Desktop header, creation, runtime and color interactions have
   been checked; exhaustive browser/accessibility testing remains.
5. Reference preview assumes the existing local container, FREEPDB1 alias, and port 7001.
   Additional testbed configurations, automatic menu/library dependency compilation and
   compile/deploy cancellation require more work.
6. Full Forms/Reports runtime compatibility, lossless report XML round trips, and Track K
   golden-trace automation remain unfinished. Parsed-model diff cannot inspect unparsed data.
7. Add broader automated coverage around preview job lifecycle and authenticated compilation.

### Running and validation

The newest server with disk Save Form and Oracle preview routes is on **http://127.0.0.1:7075**.
Older processes on 7070–7074 may still be running; their UIs may read current assets but their
API routes can be older. Use 7075 for this implementation until restarting the standard server.
Keep browser-only edits or export them before refreshing/restarting. Native module/session
state is in memory; Save Form XML files survive restarts.

From the repository root:

```bash
cargo check
cargo test
node --check ui/app.js
node --test tests/ui_*.test.cjs
python3 tests/test_reference_pipeline.py
```

Node is an optional frontend test tool, not a workspace runtime dependency. The Oracle bridge
is optional reference tooling. Never stage `temp/` or `context/`. Saved preview XML versions in
samples are generated artifacts; review them before any commit. Use git history and the associated pull request for publication status.

---

## Quick Context Summary

- **Repository**: `oracle-forms-fmb-research` (GitHub: `richfrem/oracle-forms-workbench`)
- **Working tree**: active, uncommitted improvements; inspect `git status` before continuing
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
- **Phase F (Completed)**: Visual Layout Editor, Form Diff Tool & XML Round-Trip (`crates/oracle-forms-core`, `crates/oracle-forms-xml`, `crates/oracle-forms-cli`, `crates/oracle-forms-server`, `ui/`), Forms2XML export serializer (`FormsXmlWriter`), structural & visual diff engine (`FormDiffer`, `ModuleDiff`), CLI `diff` and `export-xml` subcommands, server REST endpoints (`/api/forms/export-xml`, `/api/forms/diff`, `/api/forms/module/:name/item`), interactive 2D canvas drag-and-drop item layout editing with coordinate badges, editable Property Inspector, and in-browser Form Diff Tool tab.
- **Phase G (Completed)**: Reports Engine (`crates/oracle-forms-core`, `crates/oracle-forms-xml`, `crates/oracle-forms-runtime`, `crates/oracle-forms-cli`, `crates/oracle-forms-server`, `ui/`), ROS 60050 research spike completed, pure-native Reports XML streaming parser (`ReportsXmlParser`), Data Model Evaluator (`ReportDataEvaluator`) with master-detail data links, formulas (`CF_`), group and report-level summaries (`CS_` sum, count, avg, min, max, percent of total), HTML5 Paged Media layout renderer (`ReportRenderer`) with CSS `@page` printing and format masks (`$#,##0.00`), CLI `report inspect` and `report run`, server endpoints (`/api/reports`, `/api/reports/run`, `/api/reports/:name/html`), and in-browser "📊 Reports Engine" tab with live iframe preview and data model inspector.
- **Track T (Completed)**: Pure-Rust Oracle Thin Driver (`crates/oracle-forms-db/src/thin.rs`), native zero-client wire protocol over TCP :1521 using Oracle's official `oracledb = "26.0.0-beta.5"` crate without JVM, C-libraries, Instant Client, or SQL*Plus container execution. Supports connection pooling, named/positional binds, `OracleTimestamp`, `ROWID`, live container queries in 8 ms, and CLI/server/UI `--backend oracle-thin` integration.
- **Database Testbed**: Podman container `oracle23ai-lite` is running with seeded schema tables (`ORDERS`, `CUSTOMERS`, `DEPARTMENTS`, `EMPLOYEES`, `CLAIMS`, `ITEMS`, `STOCK`, etc.).

### 3. Immediate Mission: Live Interactive Runtime UI Engine (`/api/forms/session`) or Track K Conformance Kit
Please inspect `docs/plans/06-IMPLEMENTATION-ROADMAP-AND-MILESTONES.md` and help me execute the next milestone:

- **Mission A: Live Interactive Runtime UI Engine (`/api/forms/session`)**:
  1. Wire `FormSession` into `oracle-forms-server` REST & WebSocket session endpoints.
  2. Live interactive keyboard navigation (`Tab`, `Shift+Tab`, `Up/Down`, `Enter`), real-time field validation, and master-detail synchronization on the web canvas.
  3. Interactive alert dialogs, query mode (`Enter Query` / `Execute Query`), and commit buttons on the web UI.

- **Mission B: Track K (Conformance Testing Kit & Trace Logger)**:
  1. Autonomous trace logger `ofw_trace` capturing Golden event traces from live Forms 14c container execution.
  2. Parity assertion suite comparing native Rust execution against Oracle Forms engine traces.

Let's review where we are, confirm git status, and proceed with the next milestone!
```

---

## Reference Architecture Documents
- [06-IMPLEMENTATION-ROADMAP-AND-MILESTONES.md](06-IMPLEMENTATION-ROADMAP-AND-MILESTONES.md)
- [08-PLSQL-AND-EVENT-EXECUTION.md](08-PLSQL-AND-EVENT-EXECUTION.md)
- [03-RUNTIME-STATE-MACHINE-AND-TRIGGERS.md](03-RUNTIME-STATE-MACHINE-AND-TRIGGERS.md)
- [04-DATABASE-INTEGRATION-AND-CRUD.md](04-DATABASE-INTEGRATION-AND-CRUD.md)
- [AGENTS.md](../../AGENTS.md)
