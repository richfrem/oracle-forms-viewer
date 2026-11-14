# Oracle stack dependencies and downloads (Track 1)

What the Oracle reference stack needs: the host, the Podman images, the Oracle installers and everything downloaded for the Windows (WSL2) setup, the image versions, and the Windows filesystem limits. The Rust workbench's own dependencies (toolchain, Cargo crates) are in the repo root `DEPENDENCIES.md`. Ordered setup steps: [clean-clone-checklist.md](clean-clone-checklist.md). Section numbers are the original ones, so older references to "DEPENDENCIES.md 2.4b" still match.

## Contents

- [2.1 Host System Prerequisites](#21-host-system-prerequisites)
- [2.2 Container Images](#22-container-images)
- [2.3 Oracle Runtime Binaries (`oracle_home/` & `oracle_home_14c/`)](#23-oracle-runtime-binaries-oracle_home--oracle_home_14c)
- [2.4 Oracle Installers Staged for the 14c Build (`oracle-reference-stack/installer/`)](#24-oracle-installers-staged-for-the-14c-build-oracle-reference-stackinstaller)
- [2.4b Everything downloaded for the Windows (WSL2) setup](#24b-everything-downloaded-for-the-windows-wsl2-setup)
- [2.5 Container Image Versions (verified or pending)](#25-container-image-versions-verified-or-pending)
- [2.6 Windows Filesystem Limits (verified)](#26-windows-filesystem-limits-verified)

## 2. Container environment and tooling

### 2.1 Host System Prerequisites

| Dependency | Minimum Version | Purpose | Installation / Source |
| :--- | :--- | :--- | :--- |
| **Python** | 3.8+ | Host runner and APM automation scripts (`run_form.py`, `compile_form.py`, `launch_fsal.py`, `convert_fmb.py`, `health_check.py`). Uses **standard library only** (zero PyPI packages). | System Python or Homebrew |
| **Java Runtime (JRE/JDK)** | 8, 11, 17, or 21+ | Host runtime executing Forms Standalone Launcher client (`frmsal.jar`) for live desktop form rendering. | System Java or `brew install openjdk` |
| **Podman** | 4.0+ | Container virtualization engine. | macOS: `brew install podman`. Linux: distro package. Windows: install inside Ubuntu WSL2 with `sudo apt-get install -y podman` (see [windows-podman-status-log.md](windows-podman-status-log.md)) |
| **Rosetta 2** | macOS 11+ (Apple Silicon only) | Binary translation allowing Apple Silicon ARM64 machines to execute `linux/amd64` container images. | macOS only: `softwareupdate --install-rosetta`. Not needed on Windows x86-64 |
| **XQuartz** | 2.8+ (macOS only) | Host X11 window server on macOS for rendering the visual Oracle Forms Builder IDE (`frmbld`). | macOS only: `brew install --cask xquartz` |
| **X11 / Xorg** | Any modern | Native X11 server on Linux for Forms Builder display forwarding. | Native Linux distribution package. Windows: WSLg (Windows 11) |
| **WSL2** | WSL 2.x (verified: 3.0.5.0) | Linux VM that runs Podman and the containers on Windows. | Windows 11: `wsl --install`; update with `wsl --update` from an elevated PowerShell |
| **Ubuntu WSL distro** | 24.04 (verified) | Linux shell where Podman, `podman build`, and the container commands run on Windows. | `wsl --install -d Ubuntu` |
| **`.wslconfig`** | n/a | Sets the WSL VM memory and CPU limits. Location: `%USERPROFILE%\.wslconfig`. Applies to all distros. Changes need `wsl --shutdown` | Created by hand. Recommended `memory=12GB` (see the Windows guide) |
| **WSLg** | Built into Windows 11 | X11 display for Linux GUI apps, including `frmbld` (verified in WSL for this stack, 2026-11-14; see `forms-builder-gui.md`). | Included with Windows 11 WSL |
| **Java (host, FSAL client)** | JDK 17 or 21 | Runs `frmsal.jar`, the desktop Forms Standalone Launcher. **Java 26 fails** with `NoClassDefFoundError: java/applet/AppletStub` (verified on Windows). | Windows (recommended): OpenJDK 21 inside Ubuntu WSL, `sudo apt-get install -y openjdk-21-jre` (package name **not verified**; WSL's Java 21 itself is verified). Native Windows alternative: `winget install EclipseAdoptium.Temurin.21.JDK`. macOS: `brew install openjdk@17` |
| **Python (host runner)** | 3.8+ | Runs the skill scripts. Windows (recommended): `python3` 3.12 inside Ubuntu WSL (verified); the examples in this repo use `python3`. | Ubuntu: `sudo apt-get install -y python3`. Native Windows alternative: `python` or `py -3` (`winget install Python.Python.3.12`). macOS: Homebrew |
| **Ubuntu packages (Windows, inside WSL)** | Ubuntu 24.04 | Tools every Windows step uses: `podman` (4.9.3 verified), `python3`, `build-essential` (`gcc` 13.3 verified, `make`) for Rust dependencies, `unzip` for the Oracle installers, `curl` for `rustup`. | `sudo apt-get install -y podman python3 build-essential unzip curl openjdk-21-jre libxml2-utils`. `libxml2-utils` provides `xmllint`, the fast local backend of `validate_forms_xml.py` (optional: without it the validator falls back to `lxml` or to Java in a container; macOS ships `xmllint`). It is a host package only; it is not in `Containerfile.14c`. Packages other than `podman` and `build-essential` were **not recorded** when first installed. |
| **Rust (Windows, inside WSL)** | `rustup`, minimal profile; 1.100.0 verified | Builds and runs the workbench in WSL. See section 3.1. | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh -s -- -y --profile minimal` |

### 2.2 Container Images

#### 1. Oracle Forms 14c Enterprise (`forms-converter-14c:latest`)
Built via `oracle-reference-stack/Containerfile.14c`:

| Component / Package | Version / Source | Architectural Role |
| :--- | :--- | :--- |
| **Base OS** | `oraclelinux:8-slim` (`linux/amd64`) | Official Oracle Linux 8 minimal container base. |
| **Java Runtime** | Oracle JDK 17.0.12, from `installer/jdk-17.0.12_linux-x64_bin.tar.gz` (unpacked to `/usr/lib/jvm`, `JAVA_HOME=/usr/lib/jvm/jdk-17.0.12`) | Required by WebLogic Server 14.1.2 and Forms 14c; the Oracle installer checks the JDK vendor, so the OpenJDK packages are not used. |
| **Motif & X11** | `motif` (`libXm.so.4`), `libXp`, `libX11`, `libXext`, `libXrender`, `libXtst`, `libXt` | Windowing and visual widget rendering libraries. |
| **C Toolchain & Linker** | `gcc`, `make`, `binutils`, `glibc-devel`, `libnsl`, `libnsl2`, `libaio` | Required for Oracle `genshlib` generation of native runtime shared libraries (`libiplsn.so.0`, etc.) and for Oracle's makefile build of `libfrmjapi.so.0`. `libnsl2` provides `libnsl.so.2` for `bin/rwconverter` (added to the Containerfile; image not yet rebuilt). |
| **System Utilities** | `tar`, `gzip`, `shadow-utils`, `procps-ng`, `hostname`, `which` | Unpacking, the `oracle:oinstall` user (UID 1000), and process and host lookups used by WebLogic scripts. |
| **Installed FMW Stack** | Oracle Forms 14.1.2 Enterprise (`oracle_home_14c/`) | WebLogic Server 14.1.2 (`AdminServer` on port 7001), Forms Listener Servlet, and Reports Server. |

#### 2. Oracle Database 23ai Free (`oracle23ai-lite`)
Pulled from Oracle Container Registry:

| Component / Package | Version / Source | Architectural Role |
| :--- | :--- | :--- |
| **Container Image** | `container-registry.oracle.com/database/free:latest-lite` | Multi-arch official Oracle Database 23ai Free. |
| **Native Architecture** | **Native ARM64 (`aarch64`)** on Apple Silicon; `amd64` on x86_64 | Runs bare-metal native without Rosetta 2 emulation overhead. |
| **Database Listener** | Port 1521 (PDB `FREEPDB1`) | Ground-truth relational backend for live Forms blocks, queries, and transactions. |

#### 3. Legacy Oracle Forms 12c Standalone (`forms-converter:latest`)
Built via `oracle-reference-stack/Containerfile`:

| Component / Package | Version / Source | Architectural Role |
| :--- | :--- | :--- |
| **Base OS** | `oraclelinux:8-slim` (`linux/amd64`) | Official Oracle Linux 8 minimal container base. |
| **Java Runtime** | `java-1.8.0-openjdk-headless`<br/>`java-1.8.0-openjdk` | Required by Oracle Forms2XML and Forms Builder Java GUI subsystems. |
| **Motif & X11** | `motif` (`libXm.so.4`), `libXp`, `libX11`, `libXext`, `libXrender`, `libXtst`, `libXt` | Windowing and visual widget rendering libraries for Forms Builder GUI. |
| **C Runtime & Linker** | `glibc-devel`, `libnsl`, `libnsl2` | Native JNI and network service linkers for Oracle binaries. `libnsl2` provides `libnsl.so.2`, which the Forms 14c Reports converter (`bin/rwconverter`) needs to start; it is listed in `Containerfile.14c` and takes effect once the image is rebuilt. |
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

### 2.4 Oracle Installers Staged for the 14c Build (`oracle-reference-stack/installer/`)

Gitignored. Never commit. Source: Oracle eDelivery (Oracle sign-in required) and Oracle JDK archive.

| File | Source | Used by | Platform |
| :--- | :--- | :--- | :--- |
| `V1045135-01.zip` → `fmw_14.1.2.0.0_infrastructure.jar` | eDelivery, Linux x86-64 | Stage 1 WebLogic/FMW Infrastructure | Linux x86-64 on Windows and macOS (use Linux files for the container, never the Windows `.exe`) |
| `V1045121-01.zip` → `fmw_14.1.2.0.0_fr_linux64.bin` | eDelivery, Linux x86-64 | Stage 2 Forms & Reports | Same as above. Part number to be re-confirmed against the eDelivery listing |
| `jdk-17.0.12_linux-x64_bin.tar.gz` | Oracle JDK 17 archive | `Containerfile.14c` (container JDK) | Same as above (Linux x64, not the host JDK) |
| Response files (`oraInst.loc`, `infra14c_silent.rsp`, `forms14c_silent.rsp`) | Committed as templates in `oracle-reference-stack/installer-templates/`; copied into `installer/` | Stage 1 and 2 silent installs | Verified: both installs accepted these files |

Verified on Windows (WSL, Podman) 2026-11-13: Stage 1 (FMW Infrastructure 14.1.2.0.0) and Stage 2 (Forms & Reports 14.1.2.0.0) both logged success into `oracle_home_14c/` (2.8 GB after Stage 1). Stage 2's `.bin` needs `chmod +x` on the staged copy; the extracted zip does not keep the execute bit.

### 2.4b Everything downloaded for the Windows (WSL2) setup

Sizes are from the verified Windows machine, 2026-11-14. Nothing in this table is committed to git.

| What | Size | Where it came from | Where it lives |
| :--- | :--- | :--- | :--- |
| `V1045135-01.zip` (FMW Infrastructure 14.1.2.0.0) | 2.25 GB | Oracle eDelivery (Oracle sign-in required) | `oracle-reference-stack/installer/`; unzips to `fmw_14.1.2.0.0_infrastructure.jar` (2.25 GB) |
| `V1045121-01.zip` (Forms and Reports 14.1.2.0.0) | 1.42 GB | Oracle eDelivery (Oracle sign-in required) | `oracle-reference-stack/installer/`; unzips to `fmw_14.1.2.0.0_fr_linux64.bin` (1.42 GB) |
| `jdk-17.0.12_linux-x64_bin.tar.gz` | 183 MB | Oracle JDK 17 archive (Linux x64) | `oracle-reference-stack/installer/`; copied into the image by `Containerfile.14c` |
| Oracle Linux base image `container-registry.oracle.com/os/oraclelinux:8-slim` | 119 MB | Oracle Container Registry, pulled by `podman build` | Podman storage in WSL |
| Oracle Database image `container-registry.oracle.com/database/free:latest-lite` | 1.82 GB | Oracle Container Registry (`podman run` pulls it) | Podman storage in WSL |
| `forms-converter-14c:latest` (built locally, not downloaded) | 798 MB | Built from `Containerfile.14c`; packages come from the Oracle Linux repositories during the build | Podman storage in WSL |
| Ubuntu 24.04 WSL distro and WSL update | not recorded | `wsl --install -d Ubuntu-24.04`, `wsl --update` | Windows |
| Ubuntu packages: `podman` 4.9.3, `python3` 3.12.3, `build-essential` (gcc 13.3.0), `unzip`, `curl`, OpenJDK 21.0.9 | not recorded | Ubuntu apt repositories | Ubuntu WSL |
| Rust toolchain via `rustup` (`rustc` and `cargo` 1.100.0) | `~/.rustup` 600 MB, `~/.cargo` 271 MB | `https://sh.rustup.rs` | Ubuntu home directory |
| Rust crates (from `Cargo.lock`) and build output | `~/ofw-target` 1.5 GB | crates.io, by `cargo build` | `CARGO_TARGET_DIR=$HOME/ofw-target` |
| Oracle Home after the two installs (the result, not a download) | 5.4 GB | Created by the silent installs | `oracle-reference-stack/oracle_home_14c/`, mounted at `/oracle_home_14c` |

Total disk for this stack is roughly 20 GB. `plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/references/windows-podman-status-log.md` section 1 has the budget.

### 2.5 Container Image Versions (verified or pending)

| Image | Tag | Status |
| :--- | :--- | :--- |
| `forms-converter-14c:latest` (built here) | Local build from `Containerfile.14c` | Built on Windows WSL (798 MB). Stage 1 and Stage 2 installs ran in containers from this image |
| `container-registry.oracle.com/database/free` | `latest-lite` | Verified on Windows WSL (2026-11-13): starts, `$DB_USER` schema and sample queries work on port 1521. The banner says "Oracle AI Database 26ai Free", version `23.0.0.0.0`. The repo docs say 23ai. Needs `XDG_RUNTIME_DIR` and `containers/networks` (see windows-podman-status-log.md §3) |
| `oracle-reference-stack/sql/seed_test_schema.sql` | Committed | Creates `$DB_USER` (dev password `$DB_PASSWORD`), tablespace `FORMS_DATA`, tables and sample rows |
| `container-registry.oracle.com/os/oraclelinux` | `8-slim` | Base image for both Containerfiles |

### 2.6 Windows Filesystem Limits (verified)

- The Windows drive is mounted into WSL over 9p (`/mnt/c`). Without the `metadata` mount option, `chmod +x` fails with `Operation not permitted`. **Fix, verified:** set `[automount] options = "metadata,umask=22,fmask=11"` in `/etc/wsl.conf`, then `wsl --shutdown`. With it, the Oracle Home can stay on the Windows drive (`oracle-reference-stack/oracle_home_14c`), and both installs and the running server worked from there.
- The container's `oracle` user (UID 1000) maps to a different host UID under rootless Podman, so it cannot write to the Windows drive. Containers that write there run as `--user root`, which maps to your Windows user. Container-created subfolders may need `podman unshare chmod -R a+rX oracle_home_14c` before the host can read them.
- WebLogic reads the Oracle Home over this mount, so the Forms server took 15 to 44 minutes to reach RUNNING.

---

