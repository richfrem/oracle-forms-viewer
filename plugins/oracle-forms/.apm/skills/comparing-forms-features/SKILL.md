---
name: comparing-forms-features
description: "Produces side-by-side feature comparison reports between Oracle Fusion Middleware 14c Forms Server and native Rust Oracle Forms Workbench. Use when auditing module feature parity, checking built-in and trigger compatibility, or evaluating migration feasibility."
---

# Comparing Oracle Forms Features

Produces automated side-by-side feature comparison, compatibility scoring, and latency benchmarks between Oracle Fusion Middleware 14c Forms Server and the native pure-Rust Oracle Forms Workbench (OFW).

## Contents

- [Overview](#overview)
- [Quick Start](#quick-start)
- [Workflow](#workflow)
- [Side-by-Side Architectural Matrix](#side-by-side-architectural-matrix)
- [Verification & Evals](#verification--evals)
- [References](#references)

---

## Overview

Audits an Oracle Forms module (`.xml` or `.fmb`) against the runtime feature catalogs of both engines:
1. **Oracle Fusion Middleware 14c**: The enterprise baseline (Motif/Java applet via FSAL, WebLogic 14.1.2, OCI client).
2. **Native Rust Workbench**: The zero-JVM modern stack (`oracle-forms-core`, `oracle-forms-runtime`, `oracle-forms-db` via pure-Rust `oracle-thin` wire protocol, vanilla HTML5 web client).

Calculates a 0–100% parity score based on data blocks, widgets, canvases, event triggers, and PL/SQL built-in subprograms.

---

## Quick Start

Run the comparison CLI from workspace root:

```bash
# Print side-by-side summary table:
python3 plugins/oracle-forms/.apm/skills/comparing-forms-features/scripts/compare_features.py samples/ofw_demo_employees_fmb.xml

# Generate full Markdown comparison report:
python3 plugins/oracle-forms/.apm/skills/comparing-forms-features/scripts/compare_features.py samples/ofw_sales_fmb.xml --format markdown -o report.md

# Output structured JSON audit:
python3 plugins/oracle-forms/.apm/skills/comparing-forms-features/scripts/compare_features.py samples/orders_fmb.xml --format json

# Run with live database wire latency benchmark:
python3 plugins/oracle-forms/.apm/skills/comparing-forms-features/scripts/compare_features.py samples/ofw_demo_employees_fmb.xml --benchmark
```

---

## Workflow

1. **Parse Module AST**: Calls `oracle-forms-cli inspect --json` to extract blocks, items, canvases, windows, alerts, triggers, and program units.
2. **Audit PL/SQL Subprograms**: Scans all trigger and program unit bodies for standard Oracle Forms built-ins (`EXECUTE_QUERY`, `SET_ITEM_PROPERTY`, `SHOW_ALERT`, etc.).
3. **Classify Compatibility**: Categorizes each detected built-in as `SUPPORTED`, `PARTIAL`, or `UNSUPPORTED`.
4. **Compute Parity Score**: Weighted metric factoring blocks (20%), items (25%), canvases (15%), triggers (25%), and built-ins (15%).
5. **Output Comparison**: Emits side-by-side summary, widget distributions, and implementation recommendations.
6. **Optional Benchmark**: Executes multi-run query against Oracle 23ai measuring async wire latency.

---

## Side-by-Side Architectural Matrix

| Feature Dimension | Oracle Fusion Middleware 14c | Native Rust Oracle Forms Workbench |
| :--- | :--- | :--- |
| **Server Footprint** | ~5 GB container, WebLogic AdminServer 14.1.2 | ~25 MB standalone native executable |
| **Client Requirement** | Host Java 17+ & Forms Standalone Launcher | Zero-install modern web browser |
| **Database Driver** | Oracle Net / OCI C libraries | Pure-Rust async Tokio wire protocol (`oracle-thin`) |
| **Query Latency** | ~3,000–5,000 ms (applet startup) | 1–5 ms direct wire queries |
| **Trigger Execution** | Server-side C/Forms engine | AST tree-walking interpreter with package state |
| **Alerts & Modals** | Modal dialog window in Java applet | Resumable 3-button HTML modal continuations |

---

## Verification & Evals

Trigger evals are registered in [`evals/evals.json`](evals/evals.json).

Verify the skill locally:
```bash
python3 plugins/oracle-forms/.apm/skills/comparing-forms-features/scripts/compare_features.py samples/ofw_demo_employees_fmb.xml
python3 plugins/oracle-forms/.apm/skills/comparing-forms-features/scripts/compare_features.py samples/ofw_sales_fmb.xml
```

---

## References

- Feature Catalog & Built-in Mapping: [feature-catalog.md](references/feature-catalog.md)
- End-to-End Runtime Parity Testing: [testing-forms-parity](../testing-forms-parity/SKILL.md)
- Forms Batch Compilation: [compiling-and-running-oracle-forms](../compiling-and-running-oracle-forms/SKILL.md)
