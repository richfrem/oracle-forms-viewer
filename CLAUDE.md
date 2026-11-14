# CLAUDE.md

**Read [AGENTS.md](AGENTS.md) in full before doing anything else in this repository, and follow it.** It is the project's instruction file for all coding agents, and it overrides default behavior.

**Then read every file in [.agent/rules/](.agent/rules/).** Those rules are mandatory and always on. This applies on every task, including short ones and follow-ups. In particular, [.agent/rules/document-every-step.md](.agent/rules/document-every-step.md): document every change, error, and configuration adjustment as it happens, script anything done by hand, and report only what was verified.

Do not start work until you have read both. If this file and AGENTS.md ever disagree, AGENTS.md and the rules in `.agent/rules/` win.
