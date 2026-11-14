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

Moved to the setup skill, with the rest of the setup how-to: [plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/references/dependencies.md](plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/references/dependencies.md). It covers the host prerequisites (2.1), container images (2.2), Oracle runtime binaries (2.3), installers and everything downloaded with sizes (2.4 and 2.4b), image versions (2.5), and Windows filesystem limits (2.6). The section numbers are kept so existing references stay valid.

---

## 3. Track 2: Pure-Native Rust Workspace (`oracle-forms-workbench`)

### 3.1 Host Toolchain Requirements

| Tool | Minimum Version | Channel | Invariant Rule |
| :--- | :--- | :--- | :--- |
| **Rust Compiler (`rustc`)** | 1.75.0+ | Stable (Edition 2021) | **Zero JVM, zero Node.js, zero Oracle Client runtime required.** Verified with 1.94.1 (native Windows) and 1.100.0 (WSL Ubuntu). |
| **Cargo** | 1.75.0+ | Stable | Workspace manager. |
| **C compiler** (`gcc` on Linux, macOS, and WSL) | Any recent version | Distro package | The TLS dependency `aws-lc-sys` compiles C during the build. On Ubuntu: `sudo apt-get install -y build-essential`. gcc 13.3 is verified in WSL. |
| **Windows: build and run inside WSL** (recommended) | Ubuntu 24.04 on WSL2, Rust installed with `rustup` | Stable | **No Visual Studio and no Windows SDK needed.** Verified: `cargo check` (0 errors), `cargo test` (120 passed, 0 failed), and the app serving on port 7070 and launching forms from the Preview dialog. |
| **MSVC linker and Windows SDK** (native Windows build only; a red herring, **not needed in WSL**) | Visual Studio 2022/18 "Desktop development with C++" workload; Windows SDK 10.0.19041+ | Stable | Required by `link.exe` and `cl.exe` only if you build the app as a native Windows program. See the alternative below. |

**Recommended on Windows: build and run in WSL.** The containers already run in WSL, so the Rust app runs there too, with almost the same code path as macOS. In the Ubuntu shell:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
. "$HOME/.cargo/env"
cd /mnt/c/Users/<user>/source/repos/oracle-forms-workbench
export CARGO_TARGET_DIR=$HOME/ofw-target    # build output on WSL's own disk: much faster than /mnt/c
cargo check && cargo test
```

Keep the build output (`CARGO_TARGET_DIR`) on WSL's own disk. The source can stay on the Windows drive. Rust 1.100 prints 8 manifest lint warnings (a redundant `homepage` and an unused `tracing` dependency in some `Cargo.toml` files); they are not code warnings, and Rust 1.94 prints none. The run and restart steps are in `plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/references/rust-workbench-setup.md` and `plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/references/starting-the-stack.md`.

**Native Windows build: a red herring, not needed.** It was tried first; Visual Studio and the C++ requirements took far more setup, and running everything in WSL worked much better. Use this only if you cannot run the app in WSL. The default Rust target on Windows is `x86_64-pc-windows-msvc`. Rust links with `link.exe` and uses the MSVC C runtime and Windows SDK, and the `aws-lc-sys` crate also compiles C with `cl.exe`, which needs the MSVC and SDK headers (`stdint.h`, `vcruntime.h`). So the native build requires Visual Studio. The Preview dialog also has to reach the containers through `wsl.exe`, and FSAL needs Java 17 or 21 (Windows Java 26 fails with `java/applet/AppletStub`).

Verified on this machine: `rustc 1.94.1`, Windows SDK 10.0.19041.0, MSVC 14.51.36231. After the repair below and the link-environment workaround, `cargo check` and `cargo test` passed natively. `msvcrt.lib` is in `VC\Tools\MSVC\<ver>\lib\onecore\x64`, which the default linker path does not search, and the install here lacks `vcvarsall.bat`, so `vcvars64.bat` cannot load the environment. Setting `LIB` and `INCLUDE` to the onecore, ucrt, um, and shared folders fixes the link and the C compile (the exact `set` lines are in `plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/references/windows-podman-setup.md` section 12). Before the repair, the C compile of `aws-lc-sys` failed with `Cannot open include file: 'stdint.h'`, because the VS install was missing the C/C++ headers (`vcruntime.h`) and the SDK UCRT headers.

**Repair command (run PowerShell as Administrator).** A passive or quiet installer run must be elevated from the start. Running it unelevated exits with code 5007:

```powershell
& "C:\Program Files (x86)\Microsoft Visual Studio\Installer\vs_installer.exe" modify `
  --installPath "C:\Program Files\Microsoft Visual Studio\18\Community" `
  --add Microsoft.VisualStudio.Workload.NativeDesktop `
  --add Microsoft.VisualStudio.Component.Windows10SDK.19041 `
  --includeRecommended --passive --norestart
```

The installer can need a Windows restart, and one package (the x86 VC++ redistributable) can stall on a hidden "Please insert the disk" prompt. After the repair, confirm `vcruntime.h` exists under `VC\Tools\MSVC\<ver>\include`, then run `cargo check` and `cargo test` from the repo root.
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
