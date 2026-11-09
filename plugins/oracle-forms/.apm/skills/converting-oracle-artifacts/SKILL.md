---
name: converting-oracle-artifacts
description: "Converts Oracle Forms (.fmb, .mmb, .olb to and from XML), PL/SQL libraries (.pll and .pld), and Reports (.rdf to XML and REX). Use when extracting form ASTs, decompiling binary forms, inspecting reports, or converting XML back to binary forms."
---

# Converting Oracle Artifacts

Converts Oracle Forms (.fmb, .mmb, .olb to and from XML), PL/SQL libraries (.pll and .pld), and Reports (.rdf to XML and REX). Use when extracting form ASTs, decompiling binary forms, inspecting reports, or converting XML back to binary forms.

## Contents

- [Quick start](#quick-start)
- [Workflow](#workflow)
- [Verification](#verification)

## Quick start

Run from this skill's root:

```bash
python3 scripts/converting_oracle_artifacts.py --help
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
