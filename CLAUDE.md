# CLAUDE.md

**Read [AGENTS.md](AGENTS.md) in full before doing anything else in this repository, and follow it.** It is the project's instruction file for all coding agents, and it overrides default behavior.

**Then read every file in [.agent/rules/](.agent/rules/).** Those rules are mandatory and always on. This applies on every task, including short ones and follow-ups. In particular, [.agent/rules/document-every-step.md](.agent/rules/document-every-step.md): document every change, error, and configuration adjustment as it happens, script anything done by hand, and report only what was verified.

**Then read [architecture.md](architecture.md)** (where Fusion Middleware and the Rust app run on macOS and Windows, how they fit together, the repository map, and how to start and check the stack), and run `python3 start_testbed.py --status` (read-only; from the WSL shell on Windows) to see what is up before touching containers.

Setup, run, and troubleshooting how-to lives once in the setup skill's references (`plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/references/index.md`; start with `clean-clone-checklist.md`), never duplicated elsewhere.

Do not start work until you have read all of these. If this file and AGENTS.md ever disagree, AGENTS.md and the rules in `.agent/rules/` win.
