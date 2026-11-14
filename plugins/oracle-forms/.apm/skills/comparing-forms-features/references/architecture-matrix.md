# Side-by-side architectural matrix

Detail behind the `comparing-forms-features` skill.

## Contents

- [Matrix](#matrix)
- [Engines audited](#engines-audited)

## Matrix

| Feature Dimension | Oracle Fusion Middleware 14c | Native Rust Oracle Forms Workbench |
| :--- | :--- | :--- |
| **Server Footprint** | ~5 GB container, WebLogic AdminServer 14.1.2 | ~25 MB standalone native executable |
| **Client Requirement** | Host Java 17+ and Forms Standalone Launcher | Zero-install modern web browser |
| **Database Driver** | Oracle Net / OCI C libraries | Pure-Rust async Tokio wire protocol (`oracle-thin`) |
| **Query Latency** | ~3,000 to 5,000 ms (applet startup) | 1 to 5 ms direct wire queries |
| **Trigger Execution** | Server-side C/Forms engine | AST tree-walking interpreter with package state |
| **Alerts and Modals** | Modal dialog window in the Java applet | Resumable 3-button HTML modal continuations |

## Engines audited

1. **Oracle Fusion Middleware 14c:** the enterprise baseline (Motif/Java applet via FSAL, WebLogic 14.1.2, OCI client).
2. **Native Rust Workbench:** the zero-JVM modern stack (`oracle-forms-core`, `oracle-forms-runtime`, `oracle-forms-db` via the pure-Rust `oracle-thin` wire protocol, a vanilla HTML5 web client).

The parity score (0 to 100%) is a weighted metric: data blocks 20%, items 25%, canvases 15%, triggers 25%, PL/SQL built-ins 15%.
