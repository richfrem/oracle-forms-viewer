# Feasibility Assessment & Executive Recommendations: Oracle Forms Workbench

> **Target Audience**: Technical Leadership & Engineering Review  
> **Location**: `temp/plan/FEASIBILITY-ASSESSMENT-AND-EXEC-SUMMARY.md`  
> **Scope**: Pragmatic, high-ROI capabilities to **Analyze, Modify, Preview, Query, and Modernize** legacy Oracle Forms and Reports in pure Rust.

---

## 1. Executive Summary: The 80/20 Workbench Strategy

Legacy Oracle Forms and Reports deployments require an extensive runtime footprint:
- Multi-gigabyte container images containing WebLogic Infrastructure, Node Manager, and native C binaries.
- Complex system dependencies (Motif/X11 runtime libraries, resource files, and legacy font configs).
- Outdated desktop launch mechanisms (Forms Standalone Launcher / FSAL).

However, attempting a 100% drop-in replacement for the entire Oracle application server ecosystem (including obscure database packages, OLE/COM automation, and complex locking edge cases) is a multi-year project with diminishing returns.

### The Pragmatic Solution: An 80/20 Developer & Analyst Workbench
Rather than building a production application server replacement, we deliver **Oracle Forms Workbench**:
A high-performance, standalone, pure-native Rust toolchain that addresses the core requirements of maintenance and modernization teams:
1. **Instant Inspection & Code Intelligence**: Ingest `.fmb`, `.xml`, `.pld`, and `.rdf` artifacts in milliseconds. Extract tables, bind variables, triggers, call graphs, and dependency maps into searchable, visual interfaces.
2. **Visual 2D Layout & Canvas Preview**: True coordinate rendering (canvases, windows, stacked layouts, frames, shapes, prompts) with both classic and modern visual styling.
3. **Direct Database Queries & Record Browsing**: Connect directly to Oracle Database 23ai Free or SQLite. Generate automated `SELECT` statements from block metadata and browse records without WebLogic or Java.
4. **Forms-Side Event Dispatch & Bounded PL/SQL Interpreter**: Parse and interpret client-side triggers (`WHEN-*`, `KEY-*`, `PRE/POST-QUERY`) and Tier-1 built-ins (`GO_ITEM`, `EXECUTE_QUERY`, `SHOW_ALERT`) while delegating database SQL out to the database driver.
5. **Safe Round-Trip Editing**: Edit properties and layout in a visual web UI, serialize back to Forms2XML, and validate the export via automated testbed compilation (`frmcmp_batch`).

---

## 2. Feasibility Breakdown by Functional Area

```text
┌──────────────────────────────────────────────┬─────────────┬──────────────┬───────────────────────────────┐
│ Functional Capability                        │ Feasibility │ Complexity   │ ROI / Value Impact            │
├──────────────────────────────────────────────┼─────────────┼──────────────┼───────────────────────────────┤
│ 1. Deep Analysis & Cross-Referencing         │ 95% (High)  │ Low          │ ⭐⭐⭐⭐⭐ Instant, Zero Setup  │
│ 2. Visual 2D Layout & Canvas Preview         │ 90% (High)  │ Low-Med      │ ⭐⭐⭐⭐⭐ Ground-Truth Visual  │
│ 3. Form & Query Modification (XML Round-Trip)│ 85% (High)  │ Medium       │ ⭐⭐⭐⭐ Safe Maintenance     │
│ 4. Live DB Queries & Record Browsing         │ 85% (High)  │ Medium       │ ⭐⭐⭐⭐⭐ No Middleware Needed │
│ 5. LOVs & Record Group Search                │ 85% (High)  │ Medium       │ ⭐⭐⭐⭐ High Daily Utility   │
│ 6. Forms PL/SQL Front-End & Analysis         │ 80% (High)  │ Medium       │ ⭐⭐⭐⭐⭐ Code Intelligence   │
│ 7. Forms-Side PL/SQL & Tier-1 Built-ins      │ 70% (Med)   │ Med-High     │ ⭐⭐⭐⭐ Event Parity         │
│ 8. Reports Engine (Research Spike First)     │ 60% (Med)   │ Medium       │ ⭐⭐⭐ Feasibility Spike Gate │
│ 9. Full Database PL/SQL Engine               │ Out of Scope│ Extremely Hi │ ❌ Out of Scope (Use DB)      │
└──────────────────────────────────────────────┴─────────────┴──────────────┴───────────────────────────────┘
```

---

## 3. Detailed Feasibility Analysis

### 3.1 Deep Analysis & Cross-Referencing (Feasibility: 95%)
- Ingests binary `.fmb`, `.mmb`, `.olb`, `.pld`, and Forms2XML exports via streaming parser.
- Automatically indexes tables, views, bind variables, triggers, program units, and library dependencies.
- Delivers immediate standalone value for code auditing and dependency discovery.

### 3.2 Visual 2D Layout & Canvas Preview (Feasibility: 90%)
- Renders content canvases, stacked canvases, and tabbed canvases.
- Accurately scales Forms coordinate systems (Real inches/cm, Points, Pixels, Characters) to CSS/SVG pixels.
- Proven in the current `oracle-forms-server` implementation.

### 3.3 Safe Round-Trip Editing via Forms2XML (Feasibility: 85%)
- Direct byte mutation of binary `.fmb` files is error-prone due to ROS 60050 internal cyclic structures.
- The safe, proven path:
  1. Parse `.fmb` or `.xml`.
  2. Edit properties in the Web UI (coordinates, prompts, WHERE clauses, triggers).
  3. Export to canonical XML conforming to `forms-14c.xsd`.
  4. Verify compilation headlessly in the local testbed container via `frmcmp_batch`.

### 3.4 Live Database Queries & Multi-Record Browsing (Feasibility: 85%)
- Connects to Oracle Database 23ai Free or SQLite.
- Inspects block metadata (`QueryDataSourceName`, database items, `WhereClause`, `OrderByClause`) and generates parameterized SQL.
- Enables live record browsing (`Record 1 of N`) directly on the visual canvas. Delivered in Phase A.

### 3.5 Forms-Side PL/SQL Front-End & Interpreter (Feasibility: 70–80%)
- Writing a full database PL/SQL engine is explicitly out of scope; database SQL and stored packages are delegated directly to the database.
- Forms client-side PL/SQL (triggers, program units, attached libraries) operates within a bounded vocabulary of statements and Forms built-ins (`GO_ITEM`, `EXECUTE_QUERY`, `SET_ITEM_PROPERTY`, `SHOW_ALERT`).
- A recursive-descent parser (Phase C) and tree-walking interpreter with async suspension for user dialogs (Phase D) provides interactive functional parity.

### 3.6 Reports Engine (Feasibility: 60%)
- Ingestion of `.rdf` binary streams carries uncertainty regarding internal data model structures.
- Phase G commences with a dedicated research spike to determine whether binary `.rdf` decoding is viable, or whether Reports XML exports should be the baseline input.

---

## 4. Scope Boundaries: What We Explicitly Avoid

1. **Full Database PL/SQL Virtual Machine**: Emulating packages, autonomous transactions, dynamic SQL (`EXECUTE IMMEDIATE`), and PL/SQL collections in pure Rust is avoided. Pure database logic is sent to the target database engine.
2. **Direct Binary FMB Synthesis**: Compiling new binary `.fmb`/`.fmx` files from scratch in Rust is avoided; we emit validated XML and compile via the testbed's official compiler.
3. **Proprietary OS Bridges**: Legacy desktop dependencies (`HOST`, `OLE2`, `DDE`, `D2KWUTIL`) are unsupported by design; mock return values can be configured via per-project mock definitions (`ofw_mocks.yaml`).

---

## 5. Architectural Alignment & Next Steps

All detailed engineering specifications are documented in the [`temp/plan/`](./README.md) directory:
- [00. Plan Review & Revision Instructions](00-PLAN-REVIEW-AND-REVISION-INSTRUCTIONS.md)
- [02. Core Architecture & Multi-Crate Workspace](02-CORE-ARCHITECTURE-AND-CRATES.md)
- [03. Runtime State Machine & Triggers](03-RUNTIME-STATE-MACHINE-AND-TRIGGERS.md)
- [04. Database Integration & Dynamic CRUD](04-DATABASE-INTEGRATION-AND-CRUD.md)
- [05. Reports Engine Research Spike](05-REPORTS-ENGINE-SPECIFICATION.md)
- [06. Implementation Roadmap & Milestones](06-IMPLEMENTATION-ROADMAP-AND-MILESTONES.md)
- [07. Oracle Database Connectivity ADR](07-ORACLE-CONNECTIVITY-ADR.md)
- [08. PL/SQL Front-End & Event Execution Specification](08-PLSQL-AND-EVENT-EXECUTION.md)
- [09. Conformance Testing Kit](09-CONFORMANCE-TESTING.md)
- [Target Architecture](TARGET-ARCHITECTURE.md)
- [Implementation Plan](IMPLEMENTATION-PLAN.md)
