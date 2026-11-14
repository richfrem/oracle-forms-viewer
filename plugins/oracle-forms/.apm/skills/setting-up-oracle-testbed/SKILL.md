---
name: setting-up-oracle-testbed
description: "Provisions and configures the complete Oracle Forms 14c and Database 23ai Free container testbed on macOS or on Windows 11 (everything inside WSL2). Use when setting up the database, building the Forms 14c container, relinking runtime libraries, provisioning WebLogic domains, running the Rust workbench next to the testbed, or troubleshooting container errors."
---

# Setting Up Oracle Testbed

Sets up and runs the Oracle Forms 14c and Oracle Database Free testbed in Podman containers, plus the Rust workbench, on macOS or Windows 11. On Windows **everything runs inside the Ubuntu WSL2 distro**: Podman and the containers, Python, Java 21, the Rust app, and your coding agent. The reference pages are the single source of truth; this file only routes.

## Contents

- [Constraints](#constraints)
- [Quick start](#quick-start)
- [Workflow](#workflow)
- [Choose your platform](#choose-your-platform)
- [Verification](#verification)

## Constraints

1. **Windows means the Ubuntu WSL shell.** Never PowerShell, never a native Windows build (Visual Studio and C++ were a red herring). Use Ubuntu's `podman`, not the Windows one or `podman-machine-default`.
2. **Credentials only from the git-ignored `.env`.** Never type or commit passwords. `git check-ignore -v .env` must match.
3. **The Oracle Home is a host folder** (5.4 GB) mounted into containers. Updating images or containers never touches it. Where it lives is a setting (`OFW_ORACLE_HOME`). **On Windows install it on Ubuntu's disk from the start; never into the repo folder** (`oracle-reference-stack/oracle_home_14c`, on the slow Windows drive), and never copy it with Windows Explorer.
4. **Never remove or replace the owner's containers or images**, or rebuild an image, without saying what will happen first. `start_testbed.py` needs the owner's sudo password on Windows; ask the owner to run it.
5. **Do not test with Save & launch in Oracle**; each click rewrites the form's XML. Use `launch_fsal.py`.
6. **Document every step**: a change-log row and the reference that owns the topic (`.agent/rules/document-every-step.md`). Mark anything not run as not verified.

## Quick start

From the repo root, in the Ubuntu shell on Windows:

```bash
python3 plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/scripts/create_env.py   # once: creates the git-ignored .env
set -a; source .env; set +a
python3 start_testbed.py --status        # read-only: what is up
python3 start_testbed.py                 # start everything and confirm each part
```

## Workflow

1. Read [references/clean-clone-checklist.md](references/clean-clone-checklist.md): the ordered steps from a fresh clone, per platform.
2. Do the host setup for your platform (below), then the shared steps it links: Forms 14c install, database, Rust app.
3. Start and confirm with `python3 start_testbed.py` ([references/starting-the-stack.md](references/starting-the-stack.md)).
4. Verify with `scripts/health_check.py` and the testing guide; fix errors with [references/troubleshooting.md](references/troubleshooting.md).
5. Record what you did in [references/windows-wsl-change-log.md](references/windows-wsl-change-log.md) and the owning reference.

## Choose your platform

| Host | Runbook |
|---|---|
| macOS (Apple Silicon) | [references/podman-setup-guide.md](references/podman-setup-guide.md): Podman machine with Rosetta 2 |
| Windows 11 | [references/windows-podman-setup.md](references/windows-podman-setup.md): WSL2 + Ubuntu, Podman in Ubuntu, the 16 fixes a fresh install needs. Status: [references/windows-podman-status-log.md](references/windows-podman-status-log.md) |

All references, linked directly (grouped list: [references/index.md](references/index.md)):

- Setup: [installing-forms-14c.md](references/installing-forms-14c.md), [database-23ai.md](references/database-23ai.md), [rust-workbench-setup.md](references/rust-workbench-setup.md), [dependencies.md](references/dependencies.md), [oracle-home-location.md](references/oracle-home-location.md), [oracle-reference-stack-map.md](references/oracle-reference-stack-map.md), [legacy-12c.md](references/legacy-12c.md)
- Running and testing: [starting-the-stack.md](references/starting-the-stack.md), [forms-and-reports-testing-guide.md](references/forms-and-reports-testing-guide.md), [forms-builder-gui.md](references/forms-builder-gui.md), [reports-conversion.md](references/reports-conversion.md), [troubleshooting.md](references/troubleshooting.md)
- Reference and history: [native-rust-workbench-architecture.md](references/native-rust-workbench-architecture.md), [podman-architecture.mmd](references/podman-architecture.mmd), [windows-wsl-change-log.md](references/windows-wsl-change-log.md)

## Verification

```bash
python3 plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/scripts/health_check.py   # containers, servlet, database
python3 start_testbed.py --status
```

Verified in WSL (2026-11-14): `health_check.py`, `compile_form.py`, `convert_fmb.py --check`, `ofw_sales.fmb`, the `frmbld` GUI (WSLg), a cold stop and start. **Not verified:** `apply_forms14c_config.sh` and `provision_oracle_home.sh` on a clean Oracle Home, and the whole checklist end to end on a clean machine. Eval cases: `evals/evals.json`.
