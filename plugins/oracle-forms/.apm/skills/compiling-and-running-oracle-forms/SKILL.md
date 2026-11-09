---
name: compiling-and-running-oracle-forms
description: "Compiles Oracle Forms (.fmb -> .fmx) headlessly via frmcmp_batch and executes forms live on macOS/Windows desktops via Forms Standalone Launcher (FSAL). Use when compiling forms, testing database SQL*Net binding, or running forms live."
---

# Compiling And Running Oracle Forms

Compiles Oracle Forms (.fmb -> .fmx) headlessly via frmcmp_batch and executes forms live on macOS/Windows desktops via Forms Standalone Launcher (FSAL). Use when compiling forms, testing database SQL*Net binding, or running forms live.

## Contents

- [Quick start](#quick-start)
- [Workflow](#workflow)
- [Verification](#verification)

## Quick start

Run from this skill's root:

```bash
python3 scripts/compiling_and_running_oracle_forms.py --help
```

## Workflow

1. Check the script's declared inputs and dependencies.
2. Run the supplied script with the requested inputs. Use its diagnostics to fix errors.
3. Verify the output before reporting success.

## Verification

Create at least three representative task scenarios and compare against a baseline.
Keep routing outcomes in `evals/evals.json` and task-success cases separately.
Report executed checks and any missing model coverage.

<!-- Author: implement the canonical script before publishing; --help alone is not a
task-success check. Put critical constraints early, name required dependencies and
link task-specific references directly. -->
