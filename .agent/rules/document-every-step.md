---
trigger: always_on
description: Document every change, every error, and every configuration adjustment as it happens; script anything done by hand; report only what was verified.
---

# Document every step, as it happens

The point of this repository's setup work is a written, reproducible record that other people can follow. The repo owner has asked for this many times. It is not a final cleanup task.

## Rules

1. **Document immediately.** After every change, every error and its fix, and every configuration adjustment, update the documentation before starting the next step. Do not batch it for later, and do not note it as "pending".
2. **Where to record it.** Add an entry to `plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/references/windows-wsl-change-log.md` (changes, errors with fixes, configuration, open items). Then update the document that owns the topic: the relevant skill `SKILL.md` and `references/`, `HOW-TO-RUN.md`, `docs/WINDOWS-PODMAN-SETUP.md`, `docs/DEPENDENCIES.md`, `docs/PODMAN-CONVERTER-SETUP-AND-HISTORY.md`.
3. **Record dead ends and pivots too.** If an approach is abandoned (for example the native Windows build with Visual C++), record what was tried, what failed, and why it was dropped.
4. **Script hand work.** A configuration step done by hand is not documented until it is a checked-in script or template that reproduces it. Prose such as "by hand" or "not yet committed as a template" does not count.
5. **Be honest about status.** Say "done" only for what was run or checked. Mark everything else "not verified" or "not done". Remove or correct stale "not yet verified" lines when something is verified. Never describe documentation as complete when parts are missing.

## Working with the repo owner

- Answer yes/no questions with yes or no first, then explain.
- When the owner has already said what to do, do it. Do not offer options or ask for confirmation.
- Before any destructive or image-level action (removing or replacing containers or images, rebuilding the image, deleting files), say what will happen first. Never remove or replace the owner's containers or images on your own initiative.
- When the owner asks a question, answer that question. Do not substitute a different answer or a list of offers.
- Run all Oracle tooling, builds, and tests inside WSL2 on Windows, not natively on Windows.

## Check before finishing a task

- Is every change, error, and configuration adjustment from this task in the change log?
- Does each hand-done step have a script or template?
- Do the status lines in the docs match what was actually run?
