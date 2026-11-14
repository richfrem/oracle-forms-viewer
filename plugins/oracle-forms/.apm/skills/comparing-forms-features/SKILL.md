---
name: comparing-forms-features
description: "Produces side-by-side feature comparison reports between Oracle Fusion Middleware 14c Forms Server and native Rust Oracle Forms Workbench. Use when auditing module feature parity, checking built-in and trigger compatibility, or evaluating migration feasibility."
---

# Comparing Oracle Forms Features

Produces an automated side-by-side feature comparison, compatibility score, and latency benchmark between Oracle Fusion Middleware 14c and the native pure-Rust Oracle Forms Workbench (OFW) for a form module (`.xml` or `.fmb`).

## Contents

- [Constraints](#constraints)
- [Quick start](#quick-start)
- [Workflow](#workflow)
- [Verification](#verification)
- [References](#references)

## Constraints

1. Reads the module with `oracle-forms-cli inspect --json`, so the Rust workspace must build (`cargo check`).
2. `--benchmark` needs the database container up (`python3 start_testbed.py --status`) and `.env` credentials; on Windows run from the Ubuntu WSL shell.
3. The score is a heuristic over declared features and built-ins, not a behavioral test. Runtime parity is the `testing-forms-parity` skill; compiling and running is `compiling-and-running-oracle-forms`.

## Quick start

From the repo root:

```bash
python3 plugins/oracle-forms/.apm/skills/comparing-forms-features/scripts/compare_features.py samples/ofw_demo_employees_fmb.xml
python3 plugins/oracle-forms/.apm/skills/comparing-forms-features/scripts/compare_features.py samples/ofw_sales_fmb.xml --format markdown -o report.md
python3 plugins/oracle-forms/.apm/skills/comparing-forms-features/scripts/compare_features.py samples/orders_fmb.xml --format json
python3 plugins/oracle-forms/.apm/skills/comparing-forms-features/scripts/compare_features.py samples/ofw_demo_employees_fmb.xml --benchmark
```

## Workflow

1. Parse the module AST (blocks, items, canvases, windows, alerts, triggers, program units).
2. Scan trigger and program unit bodies for Oracle Forms built-ins (`EXECUTE_QUERY`, `SET_ITEM_PROPERTY`, `SHOW_ALERT`, and others).
3. Classify each built-in `SUPPORTED`, `PARTIAL`, or `UNSUPPORTED`.
4. Compute the weighted parity score ([references/architecture-matrix.md](references/architecture-matrix.md) has the weights).
5. Emit the side-by-side summary, widget distributions, and implementation recommendations; optionally benchmark query latency against Oracle 23ai.

## Verification

Eval cases are in [evals/evals.json](evals/evals.json). Run `compare_features.py` on the two demo forms (`samples/ofw_demo_employees_fmb.xml`, `samples/ofw_sales_fmb.xml`). **Not re-run in the 2026-11-14 Windows sessions.**

## References

- [architecture-matrix.md](references/architecture-matrix.md): engines compared and the score weights
- [testing-forms-reference.md](references/testing-forms-reference.md): Forms, 14c FSAL vs native Rust
- [testing-reports-reference.md](references/testing-reports-reference.md): Reports, 14c server vs native engine
- [feature-catalog.md](references/feature-catalog.md): feature catalog and built-in mapping
