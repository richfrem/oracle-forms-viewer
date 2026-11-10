# oracle-forms Plugin

Generated via Agent Scaffolder.

## Purpose
Autonomous tooling, headless compilation, XML round-trip, and testbed container orchestration for Oracle Forms and Reports (12c/14c).

## Plugin Components
List the self-contained skills, sub-agents, and hooks provided by this plugin here. Do NOT list them in `.claude-plugin/plugin.json` as APM and Claude Code auto-discover primitives under `.apm/`.

### Skills
- **`setting-up-oracle-testbed`**: Container orchestration, health checking, and developer credential management for Oracle Database 23ai and WebLogic Forms 14c.
- **`converting-oracle-artifacts`**: Bidirectional conversion between Oracle binary forms (`.fmb`), XML representations, PL/SQL libraries (`.pll`/`.pld`), and Reports (`.rdf`).
- **`compiling-and-running-oracle-forms`**: Headless batch compilation (`frmcmp_batch`) and live desktop execution via Forms Standalone Launcher (FSAL).
- **`testing-forms-parity`**: End-to-end parity tester comparing form compilation, servlet responses, and query execution between Fusion Middleware 14c and the native pure-Rust Oracle Forms Workbench.

### Sub-Agents

### Hooks

## Directory Structure

```text
oracle-forms/
├── .apm/
│   └── skills/
├── .claude-plugin/
│   └── plugin.json
├── apm.yml
└── README.md
```
