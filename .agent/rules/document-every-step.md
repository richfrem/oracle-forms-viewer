---
trigger: always_on
description: Where and how to document in this repo. How-to lives once, in the setup skill's references; history goes in the change log; document every change, error, and configuration adjustment as it happens; script hand work; report only what was verified.
---

# Document every step, as it happens, in the right place

The point of this repository's setup work is a written, reproducible record that other people can follow on a clean clone, on macOS or on Windows (Ubuntu WSL2). The repo owner has asked for this many times. It is not a final cleanup task.

## Where things are documented (one source of truth)

**How to do things** (set up, install, run, test, troubleshoot) lives **once**, in the setup skill's references: `plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/references/`, indexed by that skill's `SKILL.md`. Never keep a second copy of a how-to in `docs/` or at the repo root; duplicates drift. The ordered list from a fresh clone is `clean-clone-checklist.md`.

| Topic | Where |
|---|---|
| Ordered steps from a fresh clone, both platforms | `references/clean-clone-checklist.md` |
| macOS host (Podman machine, Rosetta, XQuartz) | `references/podman-setup-guide.md` |
| Windows host (WSL2, Ubuntu, Podman in Ubuntu, agent in WSL, fixes) | `references/windows-podman-setup.md` (steps), `references/windows-podman-status-log.md` (status log, prerequisites, troubleshooting log) |
| Installing Forms 14c, the domain, the configuration script, starting the server | `references/installing-forms-14c.md` |
| The database container and seed schema | `references/database-23ai.md` |
| Installing and running the Rust app | `references/rust-workbench-setup.md`; what it is: `references/native-rust-workbench-architecture.md` and `.mmd` |
| Where the Oracle Home lives, moving it, verifying a copy | `references/oracle-home-location.md` |
| Every file in `oracle-reference-stack/`, `compose.yaml` | `references/oracle-reference-stack-map.md` (the folder's `README.md` only points to it) |
| Starting, restarting, and confirming the stack (`start_testbed.py`) | `references/starting-the-stack.md` |
| Testing the servers, FSAL, compiling, Reports | `references/forms-and-reports-testing-guide.md`, `references/reports-conversion.md` |
| Forms Builder GUI (`frmbld`) and live forms with FSAL | `references/forms-builder-gui.md` |
| Symptom, cause, fix | `references/troubleshooting.md` (Windows-only fixes: `windows-podman-setup.md` section 10) |
| The 12c legacy setup | `references/legacy-12c.md` |
| Conversions, validation, compiling, running (scripts, results, limits) | the `SKILL.md` and `references/` of `converting-oracle-artifacts` and `compiling-and-running-oracle-forms` |
| **History: every change, error, fix, open item** | `references/windows-wsl-change-log.md` (one table row each; this is where "what we did" goes) |
| Open work for the next session | `start-next.md` (repo root) |
| Agent rules and orientation | `AGENTS.md`, `CLAUDE.md`, `.agent/rules/`; where things run and the repo map: `architecture.md` |
| Running the Rust app day to day, CLI | `HOW-TO-RUN.md` (repo root) |
| Dependencies: Rust toolchain and crates (root) / Oracle stack, installers, downloads (skill) | `DEPENDENCIES.md` (root) / `references/dependencies.md` |
| Design, roadmap, status of features, UI backlog | `docs/plans/` |

## How to write it

1. **Write what to do, not what happened.** A reference page says the correct way: the commands, why, what you should see, and what is not verified. The story of how we got there (the dead ends, the debugging) goes in the change log, not in the how-to.
2. **Cover both platforms.** For anything that differs between macOS and Windows, say both, or say which platform the page is for and link the other. Windows means the Ubuntu WSL shell, never PowerShell or a native build.
3. **No machine-specific values.** No usernames, home paths, passwords, or container IP addresses in committed files; use `<you>`, `$VARIABLES` from the git-ignored `.env`, and host names such as `oracle-db-23ai`.
4. **Link, do not repeat.** Link to the page that owns a topic. Use relative links, and check them after moving a file: `grep -rn "<old name>"` over the repo, excluding `oracle_home_14c/`, `target/`, `temp/`, and `.git/`.
5. **Keep the index current.** A new page in `references/` gets a row in the "References" table of the setup skill's `SKILL.md`. A new or moved file gets its links fixed in the same step.
6. **Status on every claim.** Say what was run and what was checked, and mark everything else "not verified". When something gets verified, remove the stale "not yet verified" line.

## Rules

1. **Document immediately.** After every change, every error and its fix, and every configuration adjustment, update the documentation before starting the next step. Do not batch it for later, and do not note it as "pending".
2. **Two places for every step.** Add the history row to the change log, and update the how-to reference that owns the topic so a clean clone can repeat the step. The change log alone is not enough.
3. **Record dead ends and pivots too**, in the change log: what was tried, what failed, why it was dropped (for example the native Windows build with Visual C++).
4. **Script hand work.** A configuration step done by hand is not documented until it is a checked-in script or template that reproduces it. Prose such as "by hand" or "not yet committed as a template" does not count.
5. **Be honest about status.** Say "done" only for what was run or checked. Mark everything else "not verified" or "not done". Never describe documentation as complete when parts are missing.
6. **Moving or removing a document:** check what it uniquely contains and capture that in its new home first, with `cmp` or `diff` where two copies exist; then `git mv` or `git rm`; then fix every link; then record it in the change log. Do not commit; the owner says when work is done.

## Working with the repo owner

- Answer yes/no questions with yes or no first, then explain.
- When the owner has already said what to do, do it. Do not offer options or ask for confirmation.
- One step at a time when the owner asks for it. Do not run ahead, and keep messages short.
- Before any destructive or image-level action (removing or replacing containers or images, rebuilding the image, deleting files), say what will happen first. Never remove or replace the owner's containers or images on your own initiative.
- When the owner asks a question, answer that question. Do not substitute a different answer or a list of offers.
- Never commit, push, or open a pull request until the owner says the work is done.
- Run all Oracle tooling, builds, and tests inside WSL2 on Windows, not natively on Windows.

## Check before finishing a task

- Is every change, error, and configuration adjustment from this task in the change log **and** in the reference that owns the topic?
- Could someone on a clean clone repeat the step on macOS and on Windows from the references alone?
- Does each hand-done step have a script or template?
- Do the status lines in the docs match what was actually run?
- Are all links to moved or removed files fixed, and is the `SKILL.md` references index current?
