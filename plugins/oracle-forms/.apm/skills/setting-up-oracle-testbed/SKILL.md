---
name: setting-up-oracle-testbed
description: "Provisions and configures the complete Oracle Forms 14c and Database 23ai Free container testbed. Use when setting up the database, building the Forms 14c container, relinking runtime libraries, provisioning WebLogic domains, or troubleshooting container errors."
---

# Setting Up Oracle Testbed

Provisions and configures the complete Oracle Forms 14c and Database 23ai Free container testbed. Use when setting up the database, building the Forms 14c container, relinking runtime libraries, provisioning WebLogic domains, or troubleshooting container errors.

## Contents

- [Quick start](#quick-start)
- [Workflow](#workflow)
- [Verification](#verification)

## Quick start

Run from this skill's root:

```bash
python3 scripts/setting_up_oracle_testbed.py --help
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
