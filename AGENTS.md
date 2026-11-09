# AGENTS.md

Instructions and operational constraints for AI coding agents operating within this repository.

---

## 1. Project Purpose & Scope

This repository houses **Oracle Forms Workbench** (`oracle-forms-workbench`), a comprehensive research, reverse-engineering, and native developer toolchain for legacy Oracle Forms & Reports (`.fmb`, `.mmb`, `.olb`, Forms2XML exports, `.pld`, `.rdf`).

The primary active deliverable is the native Rust multi-crate workspace at the repository root, providing a high-performance streaming parser, direct binary ROS 60050 extractor, direct database query engine, CLI tool, and Axum web viewer.

---

## 2. Invariants & Rules

### Architecture & Dependencies
- **Zero JVM in Rust Workspace**: Never introduce Java, JVM, Javalin, or Node.js runtime/build dependencies into the Rust workspace (`crates/`).
- **Zero Oracle Runtime Dependency**: The Rust parsing, extraction, and database crates (`oracle-forms-core`, `oracle-forms-xml`, `oracle-forms-binary`, `oracle-forms-db`) MUST remain pure-native and run without Oracle Forms Builder, Oracle Client libraries, or Windows DLLs.
- **Portability**: Never hardcode absolute machine paths (e.g., `/Users/...` or `C:\...`) in code, configuration, or documentation. Always use workspace-relative paths (`./`, `samples/`, `docs/`).
- **Data Privacy & Git Hygiene**: Never commit or stage files from `temp/`, `temp/example-fmb/`, or `context/`. Proprietary binaries, internal form files, and session memories must remain strictly within ignored directories.

### Coding Standards
- **Direct In-Place Execution**: When implementing tasks, make changes directly to affected files in place. Do not artificially bottleneck or route code proposals through intermediate buffers or temporary files unless requested.
- **Verification Gates**: After any modification, always verify from the repository root that:
  ```bash
  cargo check
  cargo test
  ```
  All tests must compile and pass with zero warnings.

---

## 3. Directory Structure & Landmarks

```text
├── Cargo.toml                # Rust workspace root
├── crates/                   # Primary Rust multi-crate workspace
│   ├── oracle-forms-core/    # Shared AST models, sidecars, geometry
│   ├── oracle-forms-xml/     # quick-xml streaming parser (XML & PLD)
│   ├── oracle-forms-binary/  # ROS 60050 stream & heuristic extractor
│   ├── oracle-forms-db/      # Direct DB query engine (Oracle 23ai, SQLite, Mock)
│   ├── oracle-forms-cli/     # Standalone CLI (inspect, dump-sql, query)
│   └── oracle-forms-server/  # Axum server with embedded UI assets
├── ui/                       # Vanilla HTML/CSS/JS frontend
├── samples/                  # Shared test samples corpus (XML, PLD, FMB)
├── oracle-reference-stack/   # Oracle 12c & 14c Podman container reference stack + 23ai DB
├── docs/                     # Architectural specifications & blueprints
├── temp/                     # [GITIGNORED] Legacy Kotlin POC, clones, test binaries
└── HOW-TO-RUN.md             # Server startup and CLI user guide
```

---

## 4. Key Workflows

### Running the Native Viewer
```bash
cargo run -p oracle-forms-server
# Serves http://127.0.0.1:7070
```

### Inspecting & Querying Modules via CLI
```bash
# Inspect module structure
cargo run -p oracle-forms-cli -- inspect samples/orders_fmb.xml
cargo run -p oracle-forms-cli -- inspect samples/simple_module.fmb

# Execute live database query (Phase A Query Engine)
cargo run -p oracle-forms-cli -- query samples/test_db_fmb.xml --backend oracle
cargo run -p oracle-forms-cli -- query samples/orders_fmb.xml --backend mock

# Dump extracted PL/SQL
cargo run -p oracle-forms-cli -- dump-sql samples/orders_fmb.xml -o ./extracted-sql
```

### Track 1 Container Tooling & APM Skills
The containerized tooling is packaged into APM skills under `plugins/oracle-forms/.apm/skills/`:

```bash
# Health-check the 23ai DB and Forms 14c container testbed:
python3 plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/scripts/health_check.py

# Headless batch compile forms (.fmb -> .fmx):
python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/compile_form.py samples/test_db.fmb

# Launch forms live on desktop via FSAL:
python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/launch_fsal.py test_db.fmx

# Convert Forms to XML:
python3 plugins/oracle-forms/.apm/skills/converting-oracle-artifacts/scripts/convert_fmb.py samples/simple_module.fmb

# Convert PL/SQL Libraries (.pll <-> .pld):
python3 plugins/oracle-forms/.apm/skills/converting-oracle-artifacts/scripts/convert_pll.py samples/library.pll

# Convert Oracle Reports (.rdf/.rpt to XML or text):
python3 plugins/oracle-forms/.apm/skills/converting-oracle-artifacts/scripts/convert_rpt.py samples/invoice.rdf
```

---

## 5. Cross-Platform Container Setup Guide (macOS & Windows)

For AI agents and developers setting up the containerized Oracle testbed on a fresh environment:

### Step 1: Oracle Database 23ai Free (Native ARM64 / x86_64)
Oracle publishes public multi-arch container images. No Oracle account or manual download needed:
```bash
podman run -d \
  --name oracle23ai-lite \
  -p 1521:1521 \
  -e ORACLE_PWD=Welcome1 \
  container-registry.oracle.com/database/free:latest-lite
```
* **Host**: `localhost:1521`
* **Pluggable DB**: `FREEPDB1`
* **Default Admin**: `sys` or `system` / `Welcome1`
* **Test Developer User**: `forms_user` / `Welcome123#` (configured via `temp/test_db.sh`)

### Step 2: Oracle Forms 12c Standalone (Converter & Builder)
* **Image**: Built from `oracle-reference-stack/Containerfile` (`linux/amd64` using OpenJDK 8 + Motif + X11).
* **Setup**: Unpack `V983392-01_1of2.zip` and `_2of2.zip` into `oracle-reference-stack/installer/` and run silent install via `oracle-reference-stack/compose.yaml`.
* **GUI Forwarding**:
  * **macOS**: Install XQuartz (`brew install --cask xquartz`), enable "Allow connections from network clients", restart XQuartz, and run `xhost + 127.0.0.1 host.containers.internal`.
  * **Windows 11 (WSL2)**: WSLg automatically handles X11 display forwarding via `DISPLAY=:0`.

### Step 3: Oracle Forms 14c Enterprise (WebLogic + Forms + Reports)
* **Image**: Built from `oracle-reference-stack/Containerfile.14c` (`linux/amd64` using OpenJDK 17 + Motif + X11).
* **Installers**: Staged in `oracle-reference-stack/installer/`:
  1. `fmw_14.1.2.0.0_infrastructure.jar` (FMW Infrastructure 14.1.2)
  2. `fmw_14.1.2.0.0_fr_linux64.bin` (Forms and Reports 14.1.2)
* **Execution**: Run Stage 1 (`infra-install-14c`) followed by Stage 2 (`forms-install-14c`) via `compose.yaml`.

### Step 4: Running Live Forms on macOS / Windows (FSAL)
Forms 14c supports **Forms Standalone Launcher (FSAL)**, allowing the real Oracle Forms Java client to run as a native desktop window on macOS/Windows without browser plugins. In dev mode, use AdminServer port **7001**:
```bash
java -jar oracle-reference-stack/oracle_home_14c/forms/java/frmsal.jar \
  -url "http://localhost:7001/forms/frmservlet?config=standaloneapp&form=simple_module.fmx"
```
* When prompted by the native Forms **Logon** dialog:
  * **Username**: `forms_user`
  * **Password**: `Welcome123#`
  * **Database**: `10.88.0.93:1521/FREEPDB1`

