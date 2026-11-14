---
name: testing-forms-parity
description: "Tests and validates runtime and query parity between legacy Oracle Fusion Middleware 14c Forms Server (frmcmp_batch + FSAL) and the native pure-Rust Oracle Forms Workbench (oracle-forms-server + oracle-thin). Use when verifying form compilation, database execution, and runtime parity across both engines."
---

# Testing Oracle Forms Parity

Tests and validates runtime and query parity between legacy Oracle Fusion Middleware 14c Forms Server (`frmcmp_batch` + FSAL) and the native pure-Rust Oracle Forms Workbench (`oracle-forms-server` + `oracle-thin`).

## Contents

- [Overview](#overview)
- [Quick Start](#quick-start)
- [Workflow](#workflow)
- [Comparison Matrix](#comparison-matrix)
- [Testing Both Engines](#testing-both-engines)
- [Verification & Evals](#verification--evals)

---

## Overview

When developing or modernizing Oracle Forms modules, it is critical to verify that:
1. The form compiles cleanly in official Oracle Fusion Middleware (`frmcmp_batch`) against the real Oracle Database 23ai schema.
2. The form loads in the Oracle WebLogic Forms Listener Servlet (`/forms/frmservlet?config=standaloneapp`).
3. The native pure-Rust Oracle Forms Workbench engine (`oracle-forms-core`, `oracle-forms-xml`, `oracle-forms-db`) can parse the module, execute block queries over pure-Rust `oracle-thin` wire protocol, and deliver equivalent data payloads with sub-millisecond to low-millisecond latencies.

---

## Quick Start

Run the automated parity verification script from workspace root:

```bash
# Test parity on the Employee master-detail demo form:
python3 plugins/oracle-forms/.apm/skills/testing-forms-parity/scripts/test_parity.py samples/ofw_demo_employees_fmb.xml --block EMP

# Test parity on the Sales Order Entry demo form:
python3 plugins/oracle-forms/.apm/skills/testing-forms-parity/scripts/test_parity.py samples/ofw_sales_fmb.xml --block CUSTOMERS
```

---

## Workflow

```text
  [Input .xml / .fmb]
          │
          ├──> [1. Seed Schema] ──────────> Oracle Database 23ai (:1521 / FREEPDB1)
          │
          ├──> [2. XML2Forms] ────────────> .fmb binary module
          │
          ├──> [3. frmcmp_batch] ─────────> .fmx executable
          │                                      │
          │                                      ▼
          │                            [4. WebLogic 14c Servlet] (:7001)
          │                                      │
          │                                      ▼
          │                            [A. Desktop FSAL Viewer]
          │
          └──> [5. Native Rust CLI/Server] ──────> [B. Web UI Viewer] (:7070)
                      (oracle-thin wire protocol)
```

1. **Verify Containers**: Confirms `oracle23ai-lite` and `forms-server-14c` are active.
2. **Schema Seeding**: Automatically seeds corresponding SQL tables and records (e.g., `ofw_demo_employees.sql` or `ofw_demo_sales.sql`) via `sqlplus` in `oracle23ai-lite`.
3. **Containerized Conversion**: Converts XML to `.fmb` using Oracle's `XML2Forms` utility (`oracle.forms.util.xmltools.XML2Forms`).
4. **Containerized Compilation**: Compiles `.fmb` into `.fmx` using `frmcmp_batch` connected to `FREEPDB1`.
5. **Staging**: Copies generated `.fmx` into `/oracle_home_14c/forms/` so the WebLogic Forms servlet can resolve it.
6. **Servlet Health Probe**: Probes `http://localhost:7001/forms/frmservlet?config=standaloneapp&form=<form>.fmx` for HTTP 200 OK.
7. **Native Rust Engine Query**: Queries the same database block using `oracle-forms-cli query --backend oracle-thin` to verify row extraction, column type decoding, and execution timing.
8. **Parity Summary & Launch URLs**: Emits execution statistics and exact launch commands.

---

## Comparison Matrix

| Capability | Oracle Fusion Middleware 14c | Native Rust Oracle Forms Workbench |
| :--- | :--- | :--- |
| **Runtime Architecture** | Java EE / WebLogic AdminServer 14.1.2 | Standalone Native Rust binary (`oracle-forms-server`) |
| **Client Requirement** | JRE + Forms Standalone Launcher (`frmsal.jar`) | Zero-install web browser (`http://127.0.0.1:7070`) |
| **Startup / Query Latency** | ~3–5 seconds (JVM + Applet init) | 2–10 ms direct async TCP queries |
| **Client Protocol** | Oracle Forms socket protocol | HTTP/REST + JSON canvas stream |
| **Database Connectivity** | Oracle Net / OCI client libraries | Pure-Rust `oracle-thin` (async Tokio TCP wire protocol) |
| **Dependency Footprint** | ~5 GB container / Oracle Home | Single ~25 MB native compiled binary |

---

## Testing Both Engines

### Engine A: Fusion Middleware 14c (Desktop FSAL)

Run the desktop Forms Standalone Launcher (requires host Java runtime):
```bash
# Demo Employees Form:
java -jar oracle-reference-stack/oracle_home_14c/forms/java/frmsal.jar \
  -url "http://localhost:7001/forms/frmservlet?config=standaloneapp&form=ofw_demo_employees.fmx"

# Sales Order Entry Form:
java -jar oracle-reference-stack/oracle_home_14c/forms/java/frmsal.jar \
  -url "http://localhost:7001/forms/frmservlet?config=standaloneapp&form=ofw_sales.fmx"
```
* **Logon Credentials**: `$DB_USER` / `$DB_PASSWORD` @ `10.88.0.93:1521/FREEPDB1` (or `FREEPDB1` via container TNS).

### Engine B: Native Rust Workbench (Browser)

1. Start the native server:
   ```bash
   cargo run -p oracle-forms-server
   ```
2. Open in browser: `http://127.0.0.1:7070`
3. Select `ofw_demo_employees_fmb.xml` or `ofw_sales_fmb.xml` from the module selector.
4. Click **▶ Run Form** to start an interactive live session with real database queries, block navigation, and commit processing.

---

## Verification & Evals

Evaluation cases are maintained in `evals/evals.json`.

Run evaluation tests:
```bash
python3 plugins/oracle-forms/.apm/skills/testing-forms-parity/scripts/test_parity.py samples/ofw_demo_employees_fmb.xml --block EMP
python3 plugins/oracle-forms/.apm/skills/testing-forms-parity/scripts/test_parity.py samples/ofw_sales_fmb.xml --block CUSTOMERS
```
Expected output:
- Compilation: PASS (`.fmx` generated)
- WebLogic Servlet: PASS (`HTTP 200 OK`)
- Native Rust Query: PASS (`Records Fetched: 10`, Execution Time: < 25 ms)
