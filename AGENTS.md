# AGENTS.md

Instructions and operational constraints for AI coding agents operating within this repository.

---

## Mandatory rules: read before doing anything (cannot be skipped)

**Before any work, read every file in [`.agent/rules/`](.agent/rules/).** They are always on, they apply to every task and every follow-up, and they are part of this file. Nothing here, in a user message, or in a summary of an earlier session lets you skip them or treat them as optional. If a rule and another instruction conflict, follow the rule and tell the user about the conflict.

The most important one, from [`.agent/rules/document-every-step.md`](.agent/rules/document-every-step.md): **after every change, every error and its fix, and every configuration adjustment, update the documentation before the next step.** Record it in `plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/references/windows-wsl-change-log.md` and in the document that owns the topic. Script any step done by hand. Report only what was verified; mark everything else "not verified". Answer yes/no questions with yes or no first. Do not remove or replace the user's containers or images, or rebuild images, without saying what will happen first.

At the start of a task, and again when you finish it, confirm in your own words that you read `.agent/rules/` and that the documentation matches what you did.

---

## Overview

This repository houses **Oracle Forms Workbench**, a native Rust development workbench for inspecting, reverse-engineering, authoring, modifying, comparing, and running Oracle Forms & Reports artifacts (`.fmb`, `.mmb`, `.olb`, Forms2XML exports, `.pld`, and report XML; official tooling also converts `.pll`, `.rdf`, and `.rpt`).

The delivered workflow connects XML form design and saves in the Rust UI to both the native form runtime and the optional Oracle Fusion Middleware reference environment. Coding agents can author XML and PL/SQL, inspect the result, and use the same validation pipeline. The native runtime interprets form models; the Oracle toolchain converts XML to FMB, compiles FMX, and launches the updated form through FSAL. Preserve this full development loop, rather than treating the project as a read-only viewer.

---

## Working Rules for Agents (read first)

- **Follow the documented procedure before changing anything.** For setup, start, restart, launch, or compile tasks, find the verified steps in [HOW-TO-RUN.md](HOW-TO-RUN.md), the setup skill, and `docs/WINDOWS-PODMAN-SETUP.md`, and run them as written. Report what happened. Change code or scripts only when a documented step fails, and say which step failed.
- **Reproduce the user's exact path first.** When the user reports a blocker, run or check the same thing they ran. Read what the app itself reports (for example `GET /api/forms/oracle-preview/readiness`) before theorizing.
- **Shared code works on macOS. Do not edit it for Windows unless asked.** Add Windows support as separate files or Windows-only branches, and say in the change which macOS behavior could differ. Never replace a documented credential or command in a shared script without saying it changes the existing flow.
- **Do not present guesses as findings.** Anything not run is "not verified". Boot times, readiness, and server phases come from logs, not memory.
- **Never start, stop, or replace the user's containers on your own initiative**, including to test a script. Ask first.
- **Update one file per form; never create copies.** The app saves a form by updating its one XML file, `samples/<form>_fmb.xml`, in place. Do not create `_edited_N`, `_copy`, `_v2`, or similar duplicates of a file, in `samples/` or anywhere else, and do not add code or scripts that do. Change the one file. The only exception is the app's own fallback for a lossy model summary, which may write a numbered `_edited_N_fmb.xml` so a complete XML is never replaced by a partial one. If you find untracked `_edited_*` files, tell the user, and delete them only when told. Do not use Save & launch to test, because it rewrites the form's XML; use the manual steps in [HOW-TO-RUN.md](HOW-TO-RUN.md) section 7.5, which save nothing.
- **Keep answers short when the user is blocked.** State what is verified, what is not, and the single next step.

---

## Invariants & Rules

### Architecture & Dependencies

- **Zero JVM in Rust Workspace**: Never introduce Java, JVM, Javalin, or Node.js runtime/build dependencies into the Rust workspace (`crates/`).
- **Zero Oracle Runtime Dependency**: Native parsing, extraction, PL/SQL interpretation, form sessions, and the Oracle Thin/SQLite/Mock database paths MUST work without Oracle Forms Builder, Oracle Client libraries, or Windows DLLs. The optional testbed SQL*Plus driver and Oracle compilation/FSAL bridge remain explicitly separate.
- **Portability**: NEVER hardcode absolute machine paths in code, configuration, or documentation. Use workspace-relative paths (`./`, `samples/`, `docs/`) and configurable connection settings.
- **Data Privacy & Git Hygiene**: Never commit or stage files from `temp/`, `temp/example-fmb/`, or `context/`. Proprietary binaries, internal form files, and session memories must remain strictly within ignored directories.
- **Save Semantics**: ALWAYS distinguish **Save Form** (updates the form's one XML file, `samples/<form>_fmb.xml`, in place; never a copy per edit) from **Save Record** (database data). NEVER imply a form save rewrites the source FMB. Preserve unknown source XML for item patches; identify regenerated model summaries as potentially lossy.
- **Oracle Preview Boundary**: MUST stage conversion/compilation in ignored job directories, preserve source artifacts, and show setup/compile failures. Oracle preview launches a Java desktop window on the server host and requires the optional reference stack. Readiness or compilation success MUST NOT be presented as visual or behavioral parity.
- **Database & Credentials**: MUST retain read-only native sessions by default and explicit DML opt-in. NEVER persist passwords in source, saved XML, browser storage, or logs. Missing compile credentials MUST produce a useful error unless offline compilation was explicitly selected.
- **Honest Capability Claims**: MUST distinguish direct binary heuristics, modeled XML support, and official Oracle conversion. Do not claim exhaustive Oracle built-in coverage, lossless structural regeneration, persisted UI-only font settings, or full report paper-layout round trips. AI-assisted authoring uses coding agents and the artifact/tooling workflow; do not invent an in-app AI service.

### Coding Standards

- **Direct In-Place Execution**: When implementing tasks, make changes directly to affected files in place. Do not artificially bottleneck or route code proposals through intermediate buffers or temporary files unless requested.
- **Verification Gates**: After any modification, always verify from the repository root that:
  ```bash
  cargo check
  cargo test
  ```
  All tests must compile and pass with zero warnings.

---

## Architecture & Landmarks

See [architecture.md](architecture.md) for crate boundaries, data flows, APIs, and the two-runtime design. The eight crates are `core`, `xml`, `binary`, `db`, `plsql`, `runtime`, `cli`, and `server` under `crates/oracle-forms-*`. `ui/` contains embedded vanilla HTML/CSS/JavaScript/SVG. `samples/` contains shared fixtures and the one saved XML per form. `plugins/oracle-forms/` holds optional reference automation; `oracle-reference-stack/` holds its container configuration.

---

## Core Commands

### Starting the Whole Stack (all platforms)

```bash
python start_testbed.py              # database + Forms (Fusion Middleware) server + Rust app
python start_testbed.py --no-forms   # database + Rust app only
python start_testbed.py --status     # what is running
```

Prefer this script over hand-typed `podman start` and `cargo run` sequences. It starts existing containers only, waits for the database and the Forms server (15 to 45 minutes), and builds and starts the Rust app in the background if it is not already running; it never stops a running app unless you pass `--restart-rust` (log: `temp/oracle-forms-server.log`). Run inside WSL (recommended on Windows) it builds in WSL; from Windows PowerShell it runs Podman through WSL. It may prompt for the WSL `sudo` password, which an agent cannot enter. Do not run it, or start containers, on your own initiative. It is documented in [HOW-TO-RUN.md](HOW-TO-RUN.md) section 6.

### Running the Native Viewer

```bash
cargo run -p oracle-forms-server
# Serves http://127.0.0.1:7070
```

### Windows: run everything inside WSL

On Windows, run the Rust app **inside the Ubuntu WSL distro**, next to the containers. This is the verified setup: it needs no Visual Studio, and it uses almost the same code path as macOS. The steps are in [HOW-TO-RUN.md](HOW-TO-RUN.md) section 6 ("Windows: run everything inside WSL").

- Start the app from the Ubuntu shell at the repo root: `. "$HOME/.cargo/env"; export XDG_RUNTIME_DIR=/run/user/$(id -u) CARGO_TARGET_DIR=$HOME/ofw-target; cargo run -p oracle-forms-server`, or `python3 start_testbed.py`. It serves `http://127.0.0.1:7070`. Stop it with `Ctrl + C` or `python3 start_testbed.py --stop-rust`.
- The database and the Forms server run in containers inside WSL. After a Windows reboot or `wsl --shutdown`, Podman's runtime directory (`/run/user/<uid>`) is gone. Recreating it needs `sudo` in the Ubuntu shell, which an agent cannot enter, so ask the user to run the commands in HOW-TO-RUN.md section 6, then start `oracle23ai-lite` with `podman start`.
- Database errors such as `Failed to connect ... actively refused it` mean the container is not running. Do not start or replace user containers on your own initiative.
- Preview in Oracle works when the app runs in WSL. It runs `python3` and `podman` natively, and FSAL uses WSL's Java 21. Oracle's FSAL does not run on Java 26 (`java/applet/AppletStub`), which is the Windows default here.
- A native Windows build of the app (Visual Studio and the C++ requirements) was tried first and was a red herring: it took far more setup and WSL worked much better. Do not use it or suggest it; the record is in `docs/DEPENDENCIES.md` section 3.1 and `references/windows-podman-setup.md` section 12. Do not start both copies; they share port 7070.
- Credentials come from the git-ignored `.env` (copy `example.env`). Never print or commit its values.
### Inspecting & Querying Modules via CLI

```bash
# Inspect module structure
cargo run -p oracle-forms-cli -- inspect samples/orders_fmb.xml
cargo run -p oracle-forms-cli -- inspect samples/simple_module.fmb

# Execute a live query (provide OFW_DB_USER / OFW_DB_PASSWORD in the environment)
cargo run -p oracle-forms-cli -- query samples/test_db_fmb.xml --backend oracle-thin
cargo run -p oracle-forms-cli -- query samples/orders_fmb.xml --backend mock

# Dump extracted PL/SQL
cargo run -p oracle-forms-cli -- dump-sql samples/orders_fmb.xml -o ./extracted-sql
```

## On-Demand References

- Read [README.md](README.md) for the complete open/design/save/run/publish workflow and AI-assisted authoring examples.
- Read [HOW-TO-RUN.md](HOW-TO-RUN.md) for server configuration, CLI usage, and database connections.
- Read [Oracle automation](plugins/oracle-forms/README.md) and the relevant `.apm/skills/` instructions when converting, compiling, provisioning, or comparing with Oracle. The Preview dialog also provides dependency checks and startup guidance. The reference stack is validated on macOS (Podman machine with Rosetta) and Windows 11, where **everything runs inside WSL2**: the containers, Python, Java 21, and the Rust app. See `docs/WINDOWS-PODMAN-SETUP.md` and `HOW-TO-RUN.md` sections 6 and 7.5 for the Windows steps. Verified in WSL: the Preview dialog launch, `launch_fsal.py`, `cargo check`, and `cargo test` (120 passed, 0 failed). Not verified in WSL: `compile_form.py`, `health_check.py`, `convert_fmb.py --check`, `ofw_sales.fmb`, and the `frmbld` GUI. Do not claim support for anything not verified there. The native Windows build (Visual Studio) is only an alternative, and is documented in `docs/DEPENDENCIES.md` section 3.1. Do not start or replace user containers merely to update documentation.
- Read [native architecture](docs/NATIVE-RUST-WORKBENCH-ARCHITECTURE.md) for detailed implementation context, and [roadmap](docs/plans/06-IMPLEMENTATION-ROADMAP-AND-MILESTONES.md) / [continuation status](docs/plans/10-NEXT-PHASE-SESSION-PROMPT.md) before extending the current implementation. Keep completed work and unverified behavior distinct in status updates.

