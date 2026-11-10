# System Dependencies & Software Bill of Materials (SBOM)

This document provides a comprehensive inventory of all system, host, container, language runtime, and library dependencies across the two architectural tracks of this repository.

---

## 1. Architectural Separation

The repository maintains an intentional, strict boundary between its two operational tracks:

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ Track 1: Containerized Oracle Toolchain & Testbed (tools/ & plugins/)        │
│ Purpose: Official Oracle 12c & 14c baseline, WebLogic 14c, DB 23ai, FSAL    │
│ Runtime: Podman, OpenJDK 8/17, Oracle 12c/14c, WebLogic, 23ai Free (ARM64)  │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                         (Differential Validation & Live Ground Truth)
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ Track 2: Pure-Native Rust Workspace (oracle-forms-workbench)                │
│ Purpose: Standalone high-performance streaming parser, CLI & Web Viewer     │
│ Runtime: Pure native Rust (Edition 2021), Axum, vanilla HTML5/SVG           │
│ Invariant: 0 JVM, 0 Node.js, 0 npm, 0 Oracle runtime libraries              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Track 1: Container Environment & Tooling Dependencies

### 2.1 Host System Prerequisites

| Dependency | Minimum Version | Purpose | Installation / Source |
| :--- | :--- | :--- | :--- |
| **Python** | 3.8+ | Host runner and APM automation scripts (`run_form.py`, `compile_form.py`, `launch_fsal.py`, `convert_fmb.py`, `health_check.py`). Uses **standard library only** (zero PyPI packages). | System Python or Homebrew |
| **Java Runtime (JRE/JDK)** | 8, 11, 17, or 21+ | Host runtime executing Forms Standalone Launcher client (`frmsal.jar`) for live desktop form rendering. | System Java or `brew install openjdk` |
| **Podman** | 4.0+ | Container virtualization engine. | `brew install podman` or Podman Desktop |
| **Rosetta 2** | macOS 11+ | Binary translation allowing Apple Silicon ARM64 machines to execute `linux/amd64` container images. | Built-in macOS (`softwareupdate --install-rosetta`) |
| **XQuartz** | 2.8+ | Host X11 window server on macOS for rendering the visual Oracle Forms Builder IDE (`frmbld`). | `brew install --cask xquartz` |
| **X11 / Xorg** | Any modern | Native X11 server on Linux for Forms Builder display forwarding. | Native Linux distribution package |

### 2.2 Container Images

#### 1. Oracle Forms 14c Enterprise (`forms-converter-14c:latest`)
Built via [oracle-reference-stack/Containerfile.14c](oracle-reference-stack/Containerfile.14c):

| Component / Package | Version / Source | Architectural Role |
| :--- | :--- | :--- |
| **Base OS** | `oraclelinux:8-slim` (`linux/amd64`) | Official Oracle Linux 8 minimal container base. |
| **Java Runtime** | `java-17-openjdk-headless`<br/>`java-17-openjdk-devel` | Required by WebLogic Server 14.1.2 and Forms 14c. |
| **Motif & X11** | `motif` (`libXm.so.4`), `libXp`, `libX11`, `libXext`, `libXrender`, `libXtst`, `libXt` | Windowing and visual widget rendering libraries. |
| **C Toolchain & Linker** | `gcc`, `binutils`, `glibc-devel`, `libnsl` | Required for Oracle `genshlib` generation of native runtime shared libraries (`libiplsn.so.0`, etc.). |
| **Installed FMW Stack** | Oracle Forms 14.1.2 Enterprise (`oracle_home_14c/`) | WebLogic Server 14.1.2 (`AdminServer` on port 7001), Forms Listener Servlet, and Reports Server. |

#### 2. Oracle Database 23ai Free (`oracle23ai-lite`)
Pulled from Oracle Container Registry:

| Component / Package | Version / Source | Architectural Role |
| :--- | :--- | :--- |
| **Container Image** | `container-registry.oracle.com/database/free:latest-lite` | Multi-arch official Oracle Database 23ai Free. |
| **Native Architecture** | **Native ARM64 (`aarch64`)** on Apple Silicon; `amd64` on x86_64 | Runs bare-metal native without Rosetta 2 emulation overhead. |
| **Database Listener** | Port 1521 (PDB `FREEPDB1`) | Ground-truth relational backend for live Forms blocks, queries, and transactions. |

#### 3. Legacy Oracle Forms 12c Standalone (`forms-converter:latest`)
Built via [oracle-reference-stack/Containerfile](oracle-reference-stack/Containerfile):

| Component / Package | Version / Source | Architectural Role |
| :--- | :--- | :--- |
| **Base OS** | `oraclelinux:8-slim` (`linux/amd64`) | Official Oracle Linux 8 minimal container base. |
| **Java Runtime** | `java-1.8.0-openjdk-headless`<br/>`java-1.8.0-openjdk` | Required by Oracle Forms2XML and Forms Builder Java GUI subsystems. |
| **Motif & X11** | `motif` (`libXm.so.4`), `libXp`, `libX11`, `libXext`, `libXrender`, `libXtst`, `libXt` | Windowing and visual widget rendering libraries for Forms Builder GUI. |
| **C Runtime & Linker** | `glibc-devel`, `libnsl` | Native JNI and network service linkers for Oracle binaries. |
| **System Utilities** | `tar`, `gzip`, `shadow-utils`, `which` | Unpacking, user management (`oracle:oinstall`, UID 1000), and path lookup. |

### 2.3 Oracle Runtime Binaries (`oracle_home/` & `oracle_home_14c/`)

Mounted on-demand into `/oracle_home:Z`:

| Component | Files / Paths | Purpose |
| :--- | :--- | :--- |
| **Forms Builder IDE** | `bin/frmbld`, `bin/frmbld.sh` | Visual Oracle Forms IDE for macOS/Linux via X11. |
| **Batch Compiler** | `bin/frmcmp_batch` | Headless conversion of `.pll` to `.pld` text scripts and compilation of `.pld`/`.pll`. |
| **Forms to XML** | `bin/frmf2xml.sh` | Official Java/native tool converting `.fmb`, `.mmb`, and `.olb` to certified XML. |
| **Reports Converter** | `bin/rwconverter.sh` *(profile-dependent)* | Official batch utility converting Oracle Reports (`.rdf`, `.rpt`, `.rex`). |
| **Core GUI JARs** | `jlib/frmbld.jar`, `jlib/importer.jar`, `jlib/debugger.jar`, `jlib/utj.jar` | Java classes implementing Forms Builder visual panels and debugging. |
| **Oracle Common JARs**| `oracle_common/jlib/ewt3.jar`, `share.jar`, `jewt4.jar`, `ohj.jar`, `help-share.jar`, `oracle_ice.jar` | Oracle Extended Windowing Toolkit (EWT) and help system. |
| **Forms2XML JARs** | `jlib/frmxmltools.jar`, `jlib/frmjdapi.jar` | Oracle JDAPI object model and XML serializer. |

---

## 3. Track 2: Pure-Native Rust Workspace (`oracle-forms-workbench`)

### 3.1 Host Toolchain Requirements

| Tool | Minimum Version | Channel | Invariant Rule |
| :--- | :--- | :--- | :--- |
| **Rust Compiler (`rustc`)** | 1.75.0+ | Stable (Edition 2021) | **Zero JVM, zero Node.js, zero Oracle Client runtime required.** |
| **Cargo** | 1.75.0+ | Stable | Workspace manager. |

### 3.2 Cargo Dependency Bill of Materials

All Rust workspace dependencies are defined in `Cargo.toml` and vendorable via `cargo vendor`:

| Crate Name | Version | Used By | Purpose / Justification | License |
| :--- | :--- | :--- | :--- | :--- |
| **`oracledb`** | `26.0.0-beta.5` | `db` | Official pure-Rust Oracle thin wire driver (TCP :1521, TNS/TTC, zero client). | UPL-1.0 OR Apache-2.0 |
| **`rusqlite`** | `0.32` | `db` | Embedded local SQLite driver for offline query caching and characterization tests. | MIT |
| **`async-trait`** | `0.1` | `db` | Async database abstraction trait (`DatabaseDriver`). | MIT OR Apache-2.0 |
| **`serde`** | `1.0` | `core`, `server`, `db`, `plsql`, `runtime` | High-performance serialization framework (with `derive`). | MIT OR Apache-2.0 |
| **`serde_json`** | `1.0` | `core`, `cli`, `server`, `db`, `plsql`, `runtime` | JSON serialization matching browser camelCase DTO contracts. | MIT OR Apache-2.0 |
| **`quick-xml`** | `0.37` | `xml` | Streaming StAX-like XML event reader; zero DOM overhead. | MIT |
| **`regex`** | `1.11` | `core`, `xml`, `binary`, `plsql`, `db` | Pattern matching for bind variables, declarations, and SQL tokens. | MIT OR Apache-2.0 |
| **`sha2`** | `0.10` | `core` | Cryptographic SHA-256 fingerprinting for module cache identity. | MIT OR Apache-2.0 |
| **`thiserror`** | `2.0` | `core`, `xml`, `binary`, `cli`, `server`, `db`, `plsql`, `runtime` | Ergonomic, zero-cost custom error derivation. | MIT OR Apache-2.0 |
| **`clap`** | `4.5` | `cli` | Command-line argument parsing with derive macros. | MIT OR Apache-2.0 |
| **`axum`** | `0.8` | `server` | Asynchronous web framework built on Tokio and Hyper (with `multipart`). | MIT |
| **`tokio`** | `1.0` | `cli`, `server`, `db`, `runtime` | Asynchronous runtime with multi-threaded executor (`full`). | MIT |
| **`tower-http`** | `0.6` | `server` | HTTP middleware for CORS, tracing, and static files. | MIT |
| **`rust-embed`** | `8.5` | `server` | Compile-time asset embedding into the standalone binary. | MIT |
| **`mime_guess`** | `2.0` | `server` | Automatic MIME type detection for embedded web assets. | MIT |
| **`tracing`** | `0.1` | `server`, `runtime` | Structured diagnostic logging and instrumentation. | MIT |
| **`tracing-subscriber`**| `0.3` | `server` | Log formatting and environment filter configuration. | MIT OR Apache-2.0 |

### 3.3 Inter-Crate Workspace Boundaries

```text
oracle-forms-core     ◄── oracle-forms-xml (depends on core)
oracle-forms-core     ◄── oracle-forms-binary (depends on core)
oracle-forms-core     ◄── oracle-forms-plsql (depends on core)
oracle-forms-core     ◄── oracle-forms-db (depends on core)
oracle-forms-runtime  ◄── (depends on core, plsql, db)
oracle-forms-cli      ◄── (depends on core, xml, binary, plsql, db, runtime)
oracle-forms-server   ◄── (depends on core, xml, binary, plsql, db, runtime)
```

### 3.4 Database Connectivity Profile: Pure-Rust Thin Driver

The database connectivity crate (`oracle-forms-db`) defaults to the official **Oracle Pure-Rust Thin Driver** (`oracledb = "26.0.0-beta.5"`), supplemented by SQLite and Mock drivers:

- **Protocol**: Direct TCP socket to port 1521 speaking Oracle Net TNS/TTC wire protocol.
- **Client Footprint**: 0 MB (zero Instant Client, zero OCI libraries, zero C shared objects).
- **Authentication**: Native support for Oracle 12C/19c/21c/23ai verifiers (PBKDF2/SHA-512) and TLS encryption.
- **Performance**: In-process connection pooling (`oracledb::Pool`), ~8 ms query turnaround against local container testbeds.

### 3.5 Web Frontend Presentation Dependencies (`ui/`)

| Asset / Layer | Technology | Dependency Count | Notes |
| :--- | :--- | :--- | :--- |
| **HTML** | HTML5 Semantic Markup | **0** external files | Embedded into server binary via `rust-embed`. |
| **CSS** | CSS3 Custom Properties (Variables) | **0** external frameworks | Responsive layout, dark/light theme, zero Tailwind/Bootstrap. |
| **JavaScript** | Modern ES6+ (Native DOM, Fetch API) | **0** external packages | **Zero npm, zero Webpack, zero Node.js**. |
| **Canvas** | Scalable Vector Graphics (SVG 1.1) | **0** external libraries | Pan, zoom, coordinate scaling directly rendered in browser SVG. |
| **Fonts** | System monospace & sans-serif stacks | **0** web font requests | Pure offline operation with no external CDN lookups. |

---

## 4. Invariant Compliance Audit

| Policy / Constraint | Status | Verification Evidence |
| :--- | :--- | :--- |
| **Zero JVM in Rust Workspace** | **PASSED** | No JRE, JDK, Javalin, or JVM bindings exist in the Rust workspace (`crates/`). |
| **Zero Node.js / NPM Dependencies** | **PASSED** | No `package.json`, `node_modules`, or JS bundlers. Frontend is vanilla HTML/CSS/JS. |
| **Zero Oracle Client Dependencies** | **PASSED** | Pure-native Rust wire protocol driver (`oracledb` 26.0) connects over TCP :1521 without Oracle Instant Client, OCI C-bindings, or Windows DLLs. |
| **Air-Gapped / Offline Build Readiness** | **PASSED** | All Rust crates are standard crates.io packages vendorable via `cargo vendor`. |
| **Ephemeral Container Storage** | **PASSED** | Testbed containers run with `--rm`; no persistent data remains trapped in container layers. |
| **Host Path Neutrality** | **PASSED** | All file paths use workspace-relative references (`./`, `samples/`, `docs/`); zero hardcoded user paths. |
