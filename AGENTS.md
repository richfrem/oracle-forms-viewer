# AGENTS.md

Instructions and operational constraints for AI coding agents operating within this repository.

---

## 1. Project Purpose & Scope

This repository houses research, reverse-engineering, and tooling for legacy Oracle Forms (`.fmb`, `.mmb`, `.olb`, Forms2XML exports, `.pld`).

The primary active deliverable is **`oracle-forms-viewer/`**: a pure-native, standalone Rust multi-crate workspace providing a high-performance streaming parser, direct binary ROS 60050 extractor, CLI tool, and Axum web viewer.

---

## 2. Invariants & Rules

### Architecture & Dependencies
- **Zero JVM in Rust Workspace**: Never introduce Java, JVM, Javalin, or Node.js runtime/build dependencies into `oracle-forms-viewer/`.
- **Zero Oracle Runtime Dependency**: The Rust parsing and extraction crates (`oracle-forms-core`, `oracle-forms-xml`, `oracle-forms-binary`) MUST remain pure-native and run without Oracle Forms Builder, Oracle Client libraries, or Windows DLLs.
- **Portability**: Never hardcode absolute machine paths (e.g., `/Users/...` or `C:\...`) in code, configuration, or documentation. Always use workspace-relative paths (`./`, `samples/`, `docs/`).
- **Data Privacy & Git Hygiene**: Never commit or stage files from `temp/`, `temp/example-fmb/`, or `context/`. Proprietary binaries, internal form files, and session memories must remain strictly within ignored directories.

### Coding Standards
- **Direct In-Place Execution**: When implementing tasks, make changes directly to affected files in place. Do not artificially bottleneck or route code proposals through intermediate buffers or temporary files unless requested.
- **Verification Gates**: After any modification to `oracle-forms-viewer/`, always verify that:
  ```bash
  cd oracle-forms-viewer
  cargo check
  cargo test
  ```
  All tests must compile and pass with zero warnings.

---

## 3. Directory Structure & Landmarks

```text
├── oracle-forms-viewer/      # Primary Rust workspace (Cargo multi-crate)
│   ├── crates/
│   │   ├── oracle-forms-core/    # Shared AST models, sidecars, geometry
│   │   ├── oracle-forms-xml/     # quick-xml streaming parser (XML & PLD)
│   │   ├── oracle-forms-binary/  # ROS 60050 stream & heuristic extractor
│   │   ├── oracle-forms-cli/     # Standalone CLI (inspect & dump-sql)
│   │   └── oracle-forms-server/  # Axum server with embedded UI assets
│   ├── ui/                       # Vanilla HTML/CSS/JS frontend
│   └── samples/                  # Redistributable test fixtures (XML, PLD, FMB)
├── docs/                     # Architectural specifications & blueprints
├── samples/                  # Shared test samples corpus
├── tools/                    # Oracle 12c Podman container converter
├── temp/                     # [GITIGNORED] Legacy Kotlin POC, clones, test binaries
└── HOW-TO-RUN.md             # Server startup and CLI user guide
```

---

## 4. Key Workflows

### Running the Native Viewer
```bash
cd oracle-forms-viewer
cargo run -p oracle-forms-server
# Serves http://127.0.0.1:7070
```

### Inspecting Modules via CLI
```bash
cd oracle-forms-viewer
cargo run -p oracle-forms-cli -- inspect samples/orders_fmb.xml
cargo run -p oracle-forms-cli -- inspect samples/simple_module.fmb
cargo run -p oracle-forms-cli -- dump-sql samples/orders_fmb.xml -o ./extracted-sql
```

### Track 1 Container Tooling (Podman)
```bash
# Launch official Forms Builder GUI (frmbld):
python3 tools/podman-converter/run_forms_builder.py

# Convert Forms to XML:
python3 tools/podman-converter/convert_fmb.py samples/simple_module.fmb

# Convert PL/SQL Libraries (.pll <-> .pld):
python3 tools/podman-converter/convert_pll.py samples/library.pll
python3 tools/podman-converter/convert_pll.py samples/library.pld --compile

# Convert Oracle Reports (.rdf/.rpt to XML or text):
python3 tools/podman-converter/convert_rpt.py samples/invoice.rdf
```
