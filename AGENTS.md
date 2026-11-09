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
* **Image**: Built from `tools/podman-converter/Containerfile` (`linux/amd64` using OpenJDK 8 + Motif + X11).
* **Setup**: Unpack `V983392-01_1of2.zip` and `_2of2.zip` into `tools/podman-converter/installer/` and run silent install via `tools/podman-converter/compose.yaml`.
* **GUI Forwarding**:
  * **macOS**: Install XQuartz (`brew install --cask xquartz`), enable "Allow connections from network clients", restart XQuartz, and run `xhost + 127.0.0.1 host.containers.internal`.
  * **Windows 11 (WSL2)**: WSLg automatically handles X11 display forwarding via `DISPLAY=:0`.

### Step 3: Oracle Forms 14c Enterprise (WebLogic + Forms + Reports)
* **Image**: Built from `tools/podman-converter/Containerfile.14c` (`linux/amd64` using OpenJDK 17 + Motif + X11).
* **Installers**: Staged in `tools/podman-converter/installer/`:
  1. `fmw_14.1.2.0.0_infrastructure.jar` (FMW Infrastructure 14.1.2)
  2. `fmw_14.1.2.0.0_fr_linux64.bin` (Forms and Reports 14.1.2)
* **Execution**: Run Stage 1 (`infra-install-14c`) followed by Stage 2 (`forms-install-14c`) via `compose.yaml`.

### Step 4: Running Live Forms on macOS / Windows (FSAL)
Forms 14c supports **Forms Standalone Launcher (FSAL)**, allowing the real Oracle Forms Java client to run as a native desktop window on macOS/Windows without browser plugins. In dev mode, use AdminServer port **7001**:
```bash
java -jar tools/podman-converter/oracle_home_14c/forms/java/frmsal.jar \
  -url "http://localhost:7001/forms/frmservlet?config=standaloneapp&form=simple_module.fmx"
```
* When prompted by the native Forms **Logon** dialog:
  * **Username**: `forms_user`
  * **Password**: `Welcome123#`
  * **Database**: `10.88.0.93:1521/FREEPDB1`

