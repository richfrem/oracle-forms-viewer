---
name: testing-forms-parity
description: "Tests and validates runtime and query parity between legacy Oracle Fusion Middleware 14c Forms Server (frmcmp_batch + FSAL) and the native pure-Rust Oracle Forms Workbench (oracle-forms-server + oracle-thin). Use when verifying form compilation, database execution, and runtime parity across both engines."
---

# Testing Oracle Forms Parity

Tests and validates runtime and query parity between Oracle Fusion Middleware 14c (`frmcmp_batch` + FSAL) and the native pure-Rust Oracle Forms Workbench (`oracle-forms-server` + `oracle-thin`).

## Contents

- [Constraints](#constraints)
- [Quick start](#quick-start)
- [Workflow](#workflow)
- [Verification and evals](#verification-and-evals)

## Constraints

1. Needs the testbed running: `oracle23ai-lite` and `forms-server-14c` up (`python3 start_testbed.py --status`), `.env` present, and a host Java 17 or 21 for FSAL. On Windows run from the Ubuntu WSL shell.
2. A pass shows the same data and a clean compile on both engines; it is not visual or behavioral parity.
3. `test_parity.py` still stages the `.fmx` under the repo's `oracle-reference-stack/oracle_home_14c/forms/`; it does not yet follow `OFW_ORACLE_HOME`.
4. Credentials come from `.env`; use the host connect string `localhost:1521/FREEPDB1`, never a container IP address.

## Quick start

From the repo root:

```bash
python3 plugins/oracle-forms/.apm/skills/testing-forms-parity/scripts/test_parity.py samples/ofw_demo_employees_fmb.xml --block EMP
python3 plugins/oracle-forms/.apm/skills/testing-forms-parity/scripts/test_parity.py samples/ofw_sales_fmb.xml --block CUSTOMERS
```

## Workflow

1. Confirm the containers are active.
2. Seed the form's SQL tables (`samples/ofw_demo_employees.sql` or `ofw_demo_sales.sql`) in `oracle23ai-lite`.
3. Convert the XML to `.fmb` with `XML2Forms`, then compile to `.fmx` with `frmcmp_batch` against `FREEPDB1`.
4. Stage the `.fmx` in `/oracle_home_14c/forms/` and probe `http://localhost:7001/forms/frmservlet?config=standaloneapp&form=<form>.fmx` for HTTP 200.
5. Query the same block with `oracle-forms-cli query --backend oracle-thin` to check rows, column types, and timing.
6. Read the parity summary and the launch commands it prints. How to open the form in each engine and how they differ: [references/testing-both-engines.md](references/testing-both-engines.md).

## Verification and evals

Eval cases are in `evals/evals.json`. Expected for the two demo forms: Compilation PASS (`.fmx` generated), WebLogic servlet PASS (`HTTP 200 OK`), native Rust query PASS (`Records Fetched: 10`, under 25 ms). **Not re-run in the 2026-11-14 Windows sessions.** For feature-level comparison see the `comparing-forms-features` skill; for compiling and running see `compiling-and-running-oracle-forms`.
