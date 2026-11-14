# 00: Plan Review & Revision Instructions (for the implementing agent)

> **Reviewer**: Claude (review only — no code was changed)
> **Date**: 2026-11-09
> **Reviewed**: every file in `temp/plan/`, plus the Phase A code merged to `main` in PR #3 (`d76ff4a`)
> **Audience**: the agent revising the plan and building Phase B onward
> **How to use this file**: work through Parts 1–8 in order. Each part ends with a checklist. Do not start Phase B code until the Part 1 and Part 2 checklists are done.

---

## 0. Summary of the review

The plan's direction is right: build an 80/20 workbench rather than a drop-in production replacement, go read-only first, use SQLite for CI, use bound parameters, and edit through XML round-trips that are checked by compiling in the testbed. Five problems need fixing before Phase B:

1. **Phase A's Oracle path is not native.** It shells out to `podman exec ... sqlplus` and has default credentials hardcoded. The plan docs and the PR summary describe it as a "pure-Rust, zero-client" driver. That is not accurate.
2. **The plan has no real story for PL/SQL execution.** In real forms the behavior lives in triggers, program units, attached `.pll` libraries, `.mmb` menu code and `.olb`-inherited triggers. A "lightweight AST scanner" plus "push it to the database" does not work: Forms PL/SQL runs on the Forms side, not in the database, and it calls Forms built-ins the database does not have.
3. **The phases are in the wrong order.** CRUD (Phase B) comes before trigger dispatch (Phase C). But default DML in Forms runs inside the trigger pipeline (`PRE-INSERT`, `ON-INSERT`, `ON-LOCK`, `POST-UPDATE`, and so on). Building B first means rebuilding it later.
4. **The Forms event model is missing**: trigger scope and inheritance, Execution Hierarchy, `KEY-*` and `ON-*` triggers that replace default behavior, `FORM_TRIGGER_FAILURE`, `:SYSTEM.*`, timers, and multi-form.
5. **Accuracy problems**:
   - performance numbers that were never measured
   - a `dep:oracle` feature flag that needs Oracle Instant Client, which breaks the stated rule
   - "certified" XML export
   - an `.ofm` packager with no use case
   - two phase numbering schemes that disagree

The repo has been renamed **`oracle-forms-workbench`**. The plan must describe a multi-component toolkit for several user personas (Part 3), not "a viewer".

---

## Part 1 — Phase A fix-forward (blocking: do this before Phase B)

The current state on `main`:

- `crates/oracle-forms-db/src/oracle.rs`
  - `test_connection` only opens a TCP socket to `host:1521`. It proves the listener is up, not that the login works.
  - `query` runs `podman exec -i oracle23ai-lite sqlplus -s user/pass@SERVICE`. It pastes the SQL into a script wrapped in `SELECT json_object(*) FROM (<sql>)` and parses what sqlplus prints.
  - `oracle.rs:39` falls back to a hard-coded password and username when none is configured. The fallback was removed from the Rust sources. The Python skill scripts read credentials from `.env` (see `example.env`), and the Rust server reads `OFW_DB_USER` and `OFW_DB_PASSWORD` from the environment, so load `.env` in the shell first (`set -a; source .env; set +a`).
- `crates/oracle-forms-db/src/driver.rs`: `async fn query(&self, sql: &str)` has **no bind parameter argument**, so nothing can be bound.
- `crates/oracle-forms-db/src/generator.rs`:
  - falls back to `SELECT *` when no column is found
  - does not select `ROWID`
  - pastes `WhereClause` as raw text with any `:BLOCK.ITEM` references left unresolved
  - has no query-by-example (QBE) criteria

### Required changes

1. **Rename and relabel the sqlplus path.**
   - Rename `OracleDriver` to `TestbedSqlplusDriver`, in a module such as `testbed_sqlplus.rs`.
   - Doc comment and UI label: "Uses the local Podman testbed's sqlplus. Requires container `oracle23ai-lite`. Not a native driver."
   - Make the container name configurable (env `OFW_TESTBED_DB_CONTAINER`, default `oracle23ai-lite`).
   - Change the UI backend selector to: `Oracle (testbed sqlplus)`, `SQLite`, `Mock`.
2. **Remove the default credentials from the code.** If no username or password is supplied, return `DbError::ConfigError("username/password required")`. Credentials may come from the request or from env vars (`OFW_DB_USER`, `OFW_DB_PASSWORD`). The Rust server does not read `.env` itself. Local development exports them from the git-ignored `.env` (copy `example.env`) before running the server or CLI. Never log them; check the `tracing` calls.
3. **Don't put the password on a command line.** `podman exec ... sqlplus -s user/pass@svc` makes the password visible in `ps`. Send `CONNECT user/pass@svc` as the first line of stdin instead and run `sqlplus -s /nolog`.
4. **Make `test_connection` actually log in.** For the testbed driver, run `SELECT 1 FROM DUAL` and only then report success. Report the listener-only check separately as "listener reachable".
5. **Add bind parameters to the driver trait now**, before anything else depends on the trait:
   ```rust
   pub enum SqlValue { Null, Text(String), Number(String /* decimal text, see Part 5 */), Date(String /* ISO-8601 */), }
   pub struct BoundSql { pub sql: String, pub binds: Vec<(String, SqlValue)> }  // named binds, positional order derived per dialect
   async fn query(&self, stmt: &BoundSql) -> Result<QueryResult, DbError>;
   ```
   - SQLite binds them natively.
   - The testbed sqlplus driver may emit `VARIABLE`/`EXEC :b := ...` for binds, or refuse statements that have binds. Either way, document which it does.
   - User-supplied values must never be pasted into SQL text. This rule applies to every driver, the testbed one included.
6. **Fix the generator:**
   - Oracle dialect: always select `ROWID` as the first column (keep it hidden from the UI), so Phase E can lock and update by `ROWID`.
   - Never fall back to `SELECT *`. If a block has no database items, return a typed error.
   - Scan `WhereClause` and `OrderByClause` for `:NAME`, `:BLOCK.ITEM`, `:GLOBAL.X` and `:PARAMETER.X` references. Rewrite them to dialect binds and return the list of references, so the runtime can resolve them later. Until a runtime exists, an unresolved bind is a typed error, not a silent pass-through.
   - Leave string literals inside the clause alone (the scanner must skip `'...'` and `q'[...]'`).
   - Add QBE input: `Vec<(item, operator, SqlValue)>`, rendered as bound predicates. Strings containing `%` or `_` use `LIKE`.
   - Respect `BlockDml.query_allowed` and `ItemDml.query_allowed`.
7. **Make the server safe to expose:**
   - Keep the default bind address `127.0.0.1`.
   - If the server is bound to a non-loopback address while DB backends are enabled, print a startup warning.
   - Do not store DB credentials in browser `localStorage`.
8. **Correct the record.** Edit the PR #3 description, or add a follow-up note to the CHANGELOG/README: the Oracle path currently goes through the testbed's sqlplus and is not native.

### Part 1 checklist
- [ ] No string literal password or username anywhere under `crates/` (a grep for the known dev passwords and usernames under `crates/` returns nothing, using the values from `.env`)
- [ ] The driver trait takes `BoundSql`; all three drivers compile against it
- [ ] Generator tests cover: ROWID present (Oracle), no `SELECT *`, `:BLOCK.ITEM` in WhereClause becomes a bind, a literal `':not_a_bind'` is left alone, QBE `LIKE` with bound value
- [ ] UI and docs label the Oracle backend as "testbed sqlplus"
- [ ] `cargo check && cargo test` pass with zero warnings

---

## Part 2 — Oracle connectivity decision (write it as an ADR in the plan)

The plan currently asserts "direct wire protocol without Instant Client" and also lists `oracle-wire = ["dep:oracle"]`. These contradict each other: the `oracle` crate (rust-oracle) loads OCI, which means Instant Client.

Write `temp/plan/07-ORACLE-CONNECTIVITY-ADR.md` with these options and this recommendation:

| Option | What it is | Cost | Native? |
|---|---|---|---|
| **A. Testbed sqlplus adapter** (exists now) | `podman exec sqlplus` | Already built | No: needs the container |
| **B. OCI driver via `oracle` crate**, behind the cargo feature `oracle-oci` (off by default) | Uses Instant Client (~80 MB download, not 15 GB of middleware) | Small | No: needs Instant Client, but no WebLogic, no Forms, no JVM |
| **C. Pure-Rust thin driver**: port of the open-source thin-mode protocol (reference implementations: python-oracledb thin mode, go-ora, node-oracledb thin) | TNS packets, TTC protocol, O5LOGON/verifier auth, NUMBER/DATE/TIMESTAMP/LOB encoding, Native Network Encryption or TLS | Large, a project of its own | Yes |

**Recommendation:** ship A now (relabelled per Part 1). Add B next as the "real" Oracle path for users who can install Instant Client. Run C as a **separate parallel track** with its own crate (`oracle-thin` or similar). C only becomes the default after it passes the same driver test suite against 23ai Free.

Update the invariants text accordingly:
- "Parser, model, PL/SQL and runtime crates must never depend on Oracle software."
- "Database drivers that need Oracle client software must sit behind a non-default cargo feature and be named as such."

Check the licence of whichever reference implementation is used for C and record it in the ADR. python-oracledb is UPL/Apache-2.0; confirm the current terms.

### Part 2 checklist
- [ ] ADR written with options, decision, consequences
- [ ] `02-CORE-ARCHITECTURE-AND-CRATES.md` feature-flag section corrected (`oracle-oci` is not a default feature; no claim of "pure wire" until Option C exists)
- [ ] `TARGET-ARCHITECTURE.md` §1 invariant 2 rewritten as above

---

## Part 3 — Reframe the plan around the product and its personas

Rename the plan's product to **Oracle Forms Workbench** (repo `oracle-forms-workbench`). Drop "OF-Next" / "NextGen" / "the 15 GB dinosaur" language from formal docs; keep the pain-point doc factual.

Trademark: describing the product as "for Oracle Forms" is fine. Do not use Oracle logos or styling, and do not imply affiliation. Add one disclaimer line to the README.

Add a **Personas & Entry Points** section to `README.md` (plan) and `TARGET-ARCHITECTURE.md`. Every phase must say which persona it serves:

| Persona | Need | Component | Needs Oracle software? |
|---|---|---|---|
| **Reviewer / maintainer** | Read code, triggers, SQL and dependencies of a form without installing anything | Workbench: browser + PL/SQL analysis | No |
| **Analyst / modernizer** | See how a legacy form looks and behaves, trace business logic, convert to XML, export inventories | Workbench: preview + functional runtime + analysis reports | No (DB optional) |
| **Developer (light edits)** | Change prompts, layout, SQL clauses and triggers without Forms Builder | Workbench editor + XML round-trip, then testbed compile to verify | Only to compile `.fmx` |
| **Developer (real stack)** | Run the actual Forms 14c / DB 23ai stack locally in Podman/Docker | `testbed/` + APM skills | Yes (containers) |
| **Agent / automation user** | Have an AI agent set up, convert, compile, launch, inspect | `plugins/oracle-forms` APM skills + CLI | Depends on skill |

Note the repo itself uses the real stack as the **conformance oracle** for the Rust workbench (Part 6).

### Directory rename (decide once, do it together with the doc rewrite)

Approved layout (implemented):
```
oracle-forms-workbench/
├── Cargo.toml              # Rust workspace root
├── crates/                 # Pure-native Rust workspace (core, xml, binary, db, cli, server)
├── ui/                     # Vanilla HTML/CSS/JS frontend
├── samples/                # Merged test corpus (samples + test fixtures)
├── oracle-reference-stack/ # was tools/podman-converter/ (12c & 14c container testbed + 23ai DB)
├── plugins/                # APM skills (unchanged)
├── docs/  HOW-TO-RUN.md  AGENTS.md  architecture.md  README.md
```
Done in **one dedicated PR**: `git mv`, plus fixing paths in `Cargo.toml`, `AGENTS.md`, `README.md`, `architecture.md`, `HOW-TO-RUN.md`, `docs/*`, every skill script/reference. No feature work mixed in.

Files that still contain the old repo name `oracle-forms-fmb-research`:
- `HOW-TO-RUN.md`
- `docs/DEPENDENCIES.md`
- `docs/PODMAN-CONVERTER-SETUP-AND-HISTORY.md`
- `docs/FORMS-AND-REPORTS-14C-TESTING-GUIDE.md`
- `plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/references/podman-setup-guide.md`
- `plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/references/forms-and-reports-testing-guide.md`

Also update the `repository` field in the workspace `Cargo.toml` and in any plugin manifest.

### Part 3 checklist
- [ ] Persona table present in plan README and TARGET-ARCHITECTURE
- [ ] Each phase in IMPLEMENTATION-PLAN lists "Serves: <personas>"
- [ ] Directory-rename decision recorded (ask the user, don't assume)

---

## Part 4 — New phase order (replace IMPLEMENTATION-PLAN §1 and doc 06)

Use **one** numbering scheme. Delete the "Phase 1–4 / Milestone 1–4" scheme in doc 06, or rewrite 06 to match. Proposed order:

| Phase | Name | Why here | Serves |
|---|---|---|---|
| **A** ✅ | Read-only block query (done) | — | Analyst |
| **A.1** | Phase A fix-forward (Part 1) | Unblocks a safe driver trait | All DB users |
| **B** | **Runtime spine (read-only)**: `FormSession`, block record buffers (query records only), navigation (form → block → record → item), `:SYSTEM.*` variables, **trigger resolution and event dispatcher with a stub executor**, built-in registry, read-only master-detail coordination from `RelationInfo` | Everything later plugs into this. Triggers are resolved and the would-fire list is logged before any PL/SQL runs | Analyst, Modernizer |
| **C** | **PL/SQL front end and code intelligence** (new crate `oracle-forms-plsql`): lexer, parser, AST, symbol table across form + attached PLL + MMB + OLB-inherited code, analysis reports. **No execution yet.** Can run in parallel with B | Valuable on its own (the code-review persona). Required before an interpreter | Reviewer, Modernizer |
| **D** | **PL/SQL interpreter and Tier-1 built-ins**: run `WHEN-*`, `PRE-QUERY`/`POST-QUERY`, `KEY-*`, program units, PLL code; send SQL to the DB; alerts, messages, LOVs | This is the functional-preview milestone | Modernizer, Analyst |
| **E** | **Transactions / CRUD through the commit event pipeline**: insert/update/delete/lock generated per block, run inside `PRE-*`/`ON-*`/`POST-*` triggers; sequences; locking; `FRM-40501` behavior | Must come after D because `PRE-INSERT` typically assigns keys | Modernizer, Developer |
| **F** | **Editing, diff, XML round-trip** (old Phase E) | The editor needs a solid model; the round-trip is verified by compiling in the testbed | Developer |
| **G** | **Reports**: research spike first, then decide | `.rdf` decoding is unproven; don't commit to a delivery date | Analyst |
| **Track T** (parallel) | Pure-Rust Oracle thin driver (Part 2, Option C) | Independent of the runtime | All |
| **Track K** (ongoing) | **Conformance kit** (Part 6) | Starts in B; every later phase adds cases | — |

Move the Dual-Theme toggle out of the Reports phase. It is UI polish and can land any time after B.

### Part 4 checklist
- [ ] IMPLEMENTATION-PLAN.md rewritten with the order above, a deep-dive per phase, and verification gates
- [ ] Doc 06 either deleted or rewritten to match exactly
- [ ] Each phase states "Depends on: …" and "Out of scope: …"

---

## Part 5 — The PL/SQL & execution track (new doc `08-PLSQL-AND-EVENT-EXECUTION.md`)

This is the biggest gap. Replace `03` §5 ("Lightweight Trigger AST Interpreter") and feasibility §4.1 ("Writing a full PL/SQL VM is a trap") with this. The real trap is building a database PL/SQL engine. Forms-side PL/SQL with Forms built-ins is a bounded language and API, and the tool has to execute it to give a functional preview.

### 5.1 Where executable logic lives (the plan must cover all of these)

| Source | Model type today | Notes |
|---|---|---|
| Form / block / item triggers | `TriggerInfo` (`level`, `block_name`, `item_name`, `inherited`) | Body via `text_ref` |
| Program units (procedures, functions, packages) | `ProgramUnitInfo` | Package spec + body pairing |
| Attached libraries `.pll` / `.pld` | `AttachedLibraryInfo` + `utils.pld` parsing | Resolution order matters; library names shadow |
| Menu modules `.mmb` | `MenuInfo` / `MenuItemInfo` | Menu item commands are PL/SQL that call into the active form; menu startup code; `ENABLE_ITEM`-style menu built-ins |
| Object libraries `.olb` / subclassing | `InheritanceRef`, `PropertyClassInfo`, `ObjectLibraryEntry` | Inherited triggers and properties must be **resolved into the effective module** before analysis or execution (extend the existing `EffectiveItemProperties` approach to triggers) |
| Record groups / LOVs | `RecordGroupInfo` / `LovInfo` | Query text has binds |
| Block properties | `BlockDml.where_clause` etc. | Contain binds, and `SET_BLOCK_PROPERTY(DEFAULT_WHERE)` changes them at runtime |
| Item properties | `ItemDml.initial_value`, `copy_value_from_item`, calculation/summary items | `:GLOBAL.x`, `$$DATE$$`, `:SEQUENCE.X.NEXTVAL` initial values |

### 5.2 Event model specification (must be written before Phase B code)

Document, with a test case for each:
- **Trigger resolution**: item → block → form. The *Execution Hierarchy* property (Override / Before / After) decides whether a parent-level trigger also fires. Smart-trigger names and the full trigger catalogue are grouped by family: `PRE-`/`POST-`/`WHEN-`/`ON-`/`KEY-`, navigational, transactional, query-time, validation, message-handling (`ON-ERROR`, `ON-MESSAGE`), mouse/timer.
- **`KEY-*` triggers replace the default key function.** `DO_KEY('NEXT_ITEM')` runs the default unless a `KEY-NEXT-ITEM` trigger exists.
- **`ON-*` triggers replace default processing** (`ON-INSERT`, `ON-UPDATE`, `ON-DELETE`, `ON-LOCK`, `ON-SELECT`, `ON-FETCH`, `ON-COUNT`, `ON-LOGON`, `ON-COMMIT`).
- **Failure semantics**: `RAISE FORM_TRIGGER_FAILURE` aborts the triggering operation in a defined way, per trigger type (navigation aborted, validation fails, commit rolled back to savepoint). Unhandled exceptions behave differently in `WHEN-*` and `PRE-*` triggers.
- **Validation unit** (Form / Block / Record / Item) and item/record validation status (NEW / CHANGED / VALID).
- **Navigation and "restricted built-ins"**: navigation built-ins are illegal inside some triggers (for example `GO_ITEM` in `WHEN-VALIDATE-ITEM`). The runtime should raise the same error class.
- **System variables**: `:SYSTEM.CURSOR_ITEM`, `TRIGGER_ITEM`, `CURSOR_RECORD`, `CURSOR_BLOCK`, `RECORD_STATUS`, `BLOCK_STATUS`, `FORM_STATUS`, `MODE` (NORMAL / ENTER-QUERY / QUERY), `MESSAGE_LEVEL`, `SUPPRESS_WORKING`, `LAST_QUERY`, …
- **Globals and parameters**: `:GLOBAL.*` (created on assignment, string-typed, shared across forms in a session), `:PARAMETER.*`, parameter lists.
- **Multi-form**: `CALL_FORM`, `OPEN_FORM`, `NEW_FORM`, `EXIT_FORM`. Phase D supports a stack of sessions; full multi-form goes in Tier 2.
- **Timers**: `CREATE_TIMER` → `WHEN-TIMER-EXPIRED`. The server drives them and pushes events to the UI. This is the one place a WebSocket or server-sent events (SSE) are actually justified.
- **Query mode / QBE**: `ENTER_QUERY` puts the block in query mode; item values become criteria; `#` operators and `:SYSTEM.LAST_QUERY`.

### 5.3 Crate `oracle-forms-plsql` (Phase C)

- **Parser**: hand-written recursive-descent with **error recovery** (one bad trigger must not lose the module). Target PL/SQL *as used in Forms* (the Forms client engine lags database PL/SQL; record which language level is assumed).
  - Grammar coverage, in order: blocks, declarations (incl. `%TYPE`, `%ROWTYPE`, records, PL/SQL tables / associative arrays), `IF`/`CASE`/`LOOP`/`WHILE`/`FOR`/cursor `FOR`, explicit cursors, `EXCEPTION` handlers, `RAISE`, assignments, calls, packages.
  - Bind references `:a.b`, `:GLOBAL.x`, `:SYSTEM.x`, `:PARAMETER.x`, and bare `:item`.
- **Embedded SQL** (`SELECT … INTO`, DML, cursors): keep it as an **opaque SQL span** plus extracted binds, `INTO` targets and referenced tables. Do not try to parse full Oracle SQL in v1. Table/column extraction may use a light tokenizer. Revisit `sqlparser-rs` later.
- **Symbol table**: one resolved scope graph per running module. It covers form program units, attached libraries (in attach order), menu code, package specs/bodies, and built-ins (from the registry). Unresolved identifiers become diagnostics.
- **Analysis outputs** (ship these in the UI and CLI; they serve the Reviewer and Modernizer personas immediately):
  - call graph (trigger → program unit → library → DB package)
  - **event map**: for each item/block, what fires and in what order
  - item read/write cross-reference (`:BLOCK.ITEM` written by which triggers)
  - SQL inventory with tables touched (read vs write) and DB packages called
  - built-in usage histogram
  - **executability coverage report**: each statement classified as *supported / delegated-to-DB / unsupported (reason)*, rolled up per trigger, per form, and per application. Output: JSON plus an HTML page. This is how users see the limits up front.

### 5.4 Interpreter (Phase D)

- Tree-walking interpreter over the AST, executing inside a `FormSession`.
- **Value semantics** must match Oracle, each with tests:
  - three-valued `NULL` logic
  - `''` is `NULL` for VARCHAR2
  - `NUMBER` as a decimal type with 38-digit precision. `rust_decimal` gives only 28–29 digits, so either use a big-decimal crate or document the limit and test where it bites.
  - `DATE` has a time part; implicit conversions use `NLS_DATE_FORMAT` and `NLS_NUMERIC_CHARACTERS` from the session settings
  - `VARCHAR2` length checks raise `VALUE_ERROR`
  - `BOOLEAN` is PL/SQL-only
- **SQL delegation**: `SELECT … INTO`, DML and cursor fetches go to the DB driver with resolved binds. Map `NO_DATA_FOUND`, `TOO_MANY_ROWS`, `DUP_VAL_ON_INDEX`, `SQL%ROWCOUNT`/`%FOUND`/`%NOTFOUND`. Calls to stored DB packages go through as an anonymous block with binds (`BEGIN pkg.proc(:b1, :b2); END;`) and support OUT parameters.
- **Built-ins** are a registry of Rust functions with metadata (restricted? navigational? tier):
  - **Tier 1 (Phase D)**:
    - navigation: `GO_*`, `NEXT_*`/`PREVIOUS_*`/`FIRST_*`/`LAST_RECORD`
    - query and records: `EXECUTE_QUERY`, `ENTER_QUERY`, `CLEAR_*`, `CREATE_RECORD`, `DELETE_RECORD`, `DO_KEY`
    - messages and alerts: `MESSAGE`, `SHOW_ALERT`, `SET_ALERT_PROPERTY`
    - properties: `SET`/`GET_ITEM_PROPERTY`, `SET`/`GET_BLOCK_PROPERTY`, `SET_ITEM_INSTANCE_PROPERTY`
    - values: `NAME_IN`, `COPY`, `DEFAULT_VALUE`, `ERASE`
    - LOVs: `SHOW_LOV`, `LIST_VALUES`
    - record groups: `FIND_*`, `ID_NULL`
    - the `POPULATE_GROUP` family, `SYNCHRONIZE`, `PAUSE`
  - **Tier 2**:
    - forms and timers: `CALL_FORM`/`OPEN_FORM`/`NEW_FORM`, timers
    - record groups and lists: `ADD_GROUP_*`, `GET_GROUP_*`, list item built-ins
    - windows and canvases: `SET_WINDOW_PROPERTY`, `SET_CANVAS_PROPERTY`, `SHOW_VIEW`/`HIDE_VIEW`
    - menus: menu built-ins
  - **Tier 3 (unsupported by design)**: `HOST`, `OLE2.*`, `TEXT_IO`/`CLIENT_TEXT_IO`, `WEBUTIL_*`, `RUN_PRODUCT`/`RUN_REPORT_OBJECT`, `D2KWUTIL`, Java importer, `DDE`. Each raises a clear `Unsupported(builtin)` diagnostic and shows in the coverage report. Provide a **mock hook**: a per-project YAML/JSON file can stub a return value, so a preview can continue past them.
- **UI interaction from PL/SQL**: `SHOW_ALERT` returns the button the user pressed. The interpreter therefore has to **suspend** mid-trigger and resume when the browser answers. Design the interpreter as resumable from day one: either continuation/state-machine style, or run each session's interpreter on its own task and await a channel. Do not bolt this on later.
- **Determinism and safety**: step budget per trigger (catch infinite loops), recursion limit, and a per-session "read-only database" switch that blocks DML/commit, so a preview against a real database cannot change data unless the user turns that on.

### Part 5 checklist
- [ ] `08-PLSQL-AND-EVENT-EXECUTION.md` written with §5.1–5.4
- [ ] `03` §5 and feasibility §4.1 replaced with a pointer to doc 08
- [ ] Feasibility matrix row 7 re-scoped: "Forms-side PL/SQL interpreter + Tier-1 built-ins: Medium-High complexity, High ROI" and "Full DB PL/SQL engine: out of scope (delegate to DB)"

---

## Part 6 — Conformance kit: use the real stack as the source of truth (new doc `09-CONFORMANCE-TESTING.md`)

The repo already runs real Forms 14c + DB 23ai in Podman. That makes it possible to **prove** behavioral parity instead of claiming it.

1. Build a set of small **instrumented test forms** under `samples/conformance/`. Every trigger begins with `INSERT INTO ofw_trace(seq, form, trigger_name, block, item, record, sysvars...) VALUES (...)` via an autonomous-transaction DB procedure. Each form exercises one area: startup cascade, navigation and validation, query + `POST-QUERY`, master-detail, commit pipeline, `FORM_TRIGGER_FAILURE` in each trigger family, `KEY-*` override, Execution Hierarchy Before/After, LOV return mapping, alerts, globals.
2. **Scripted scenarios**: a list of user actions (focus item, type value, press key, click button, answer alert).
3. **Golden traces**: run each scenario on real Forms 14c (FSAL or a scripted client, recorded once and committed as JSON) and on the Rust runtime (headless, through the session API). Diff the trigger sequences, item values and DB state.
4. Run the Rust side in CI against SQLite where possible, and against the 23ai container in an opt-in job.
5. Phase gates (B, D, E) become "all conformance scenarios in area X match the golden trace".

Also check whether Forms' own trace facility (Forms Trace / `record=` options) is usable in 14c as a second source. The DB trace table is the dependable baseline.

### Part 6 checklist
- [ ] Doc 09 written with the trace table DDL, scenario format and diff rules
- [ ] At least 3 conformance forms specified for Phase B (startup cascade, navigation, read-only query)

---

## Part 7 — Accuracy and cleanup edits across existing plan files

| File | Edit |
|---|---|
| `README.md` (plan) | Rename the product; add the persona table; replace the doc map with the new doc list (00, 07, 08, 09); remove "15+ GB dinosaur" phrasing |
| `TARGET-ARCHITECTURE.md` | §1 invariant 2 per Part 2. **Remove invariant 4's numbers** ("cold boot < 20 ms", "< 25 MB") unless measured; replace with "measured and tracked in CI benchmarks". Remove "certified" from invariant 5 ("validated by round-trip compile in the testbed"). Add the `oracle-forms-plsql` crate. Mark the WebSocket engine as "only if needed (timers, server push); start with REST + session id". Drop PostgreSQL from Phase A–D scope (keep it as future). Mark Tauri as future |
| `02-CORE-ARCHITECTURE-AND-CRATES.md` | Add `oracle-forms-plsql` (depends on core only). Runtime depends on plsql + db. **Remove `oracle-forms-compiler` and the `.ofm` packager** (no use case; validation lives in core/plsql, and compiling is the testbed's job). Fix the feature flags per Part 2 |
| `03-RUNTIME-STATE-MACHINE-AND-TRIGGERS.md` | Fix the state diagram: Forms record status is `NEW`/`INSERT`/`QUERY`/`CHANGED`. `MARKED_DELETE` is not a Forms `RECORD_STATUS` value; keep it as an internal pending-delete flag and say so. Add the validation status of each item and record. Replace §5 with a pointer to doc 08. Reuse the existing core types (`RelationInfo`, `BlockDml`, `ItemDml`, `EffectiveItemProperties`) instead of inventing parallel ones |
| `04-DATABASE-INTEGRATION-AND-CRUD.md` | Locking: Forms' default is `SELECT … FOR UPDATE NOWAIT` on first change to a record, re-checking that the queried values still match. That is not "UPDATE … WHERE price = :original". Document `Lock Mode` (Immediate/Delayed/Automatic) and `Key Mode`. Note that `RETURNING ROWID INTO` needs an OUT bind. Bind mapping per dialect, done once in the generator |
| `05-REPORTS-ENGINE-SPECIFICATION.md` | Retitle "Research spike". Step 1: can we decode `.rdf` at all, or only Reports XML? Gate the rest on that result. Remove "< 50 ms" |
| `06-IMPLEMENTATION-ROADMAP-AND-MILESTONES.md` | Delete, or rewrite to match Part 4 exactly |
| `FEASIBILITY-ASSESSMENT-AND-EXEC-SUMMARY.md` | Re-score row 7 per Part 5; remove "Your intuition is 100% spot on" and other unsupported confidence claims; keep the 80/20 framing |
| `IMPLEMENTATION-PLAN.md` | Rewrite per Part 4; Phase A marked done, with an honest note on the testbed adapter |

General rules for all plan docs:
- Every capability claim either names its test/gate or is marked *(target)*.
- No absolute machine paths (`/Users/...`). Use repo-relative paths.
- One numbering scheme.

---

## Part 8 — Repo docs to update after the plan is approved

Do this in the rename PR or right after it, not mixed with feature code:

- **`README.md`** (root):
  - new title "Oracle Forms Workbench"
  - one-paragraph positioning
  - the persona table with an entry point for each (link to quick start / testbed / skills)
  - the honest DB capability statement (testbed sqlplus now; OCI and native later)
  - trademark disclaimer
- **`architecture.md`**:
  - crate count and list (now includes `oracle-forms-db`; plan adds `oracle-forms-plsql`, `oracle-forms-runtime`)
  - replace the "static Rust viewer models" wording
  - show the testbed's role as the conformance oracle
- **`AGENTS.md`**:
  - purpose → workbench
  - directory map
  - extend the "Zero Oracle Runtime Dependency" rule to cover `oracle-forms-db` (except feature-gated drivers named as such) and the new crates
  - add "no default or fallback credentials in code; secrets only from request or env"
  - add the `query` CLI workflow
  - keep the verification gate
- **`HOW-TO-RUN.md`** and the six files listed in Part 3: old repo name and paths.

---

## Constraints for the implementing agent

- Never commit `temp/`, `context/` or proprietary forms. This review and the plan stay in `temp/plan/` (gitignored) unless the user decides to promote them to `docs/`.
- After any change under the Rust workspace: `cargo check && cargo test`, zero warnings.
- Ask the user before: the directory rename, deleting doc 06, and choosing Option C (thin driver) as a committed track.
- Report results honestly. If something only works against the local testbed, say so in the PR description.
