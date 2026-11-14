# References index (setting-up-oracle-testbed)

The one source of truth for setting up, running, and troubleshooting the Oracle testbed and the Rust workbench. Start with [clean-clone-checklist.md](clean-clone-checklist.md): the ordered steps from a fresh `git clone` to a running testbed, per platform, linking to everything below.

## Contents

- [Setup](#setup)
- [Running and testing](#running-and-testing)
- [Reference and history](#reference-and-history)

## Setup

| Reference | What it covers |
|---|---|
| [clean-clone-checklist.md](clean-clone-checklist.md) | Ordered steps, both platforms |
| [podman-setup-guide.md](podman-setup-guide.md) | macOS host: Podman machine, Rosetta 2, XQuartz, restarting |
| [windows-podman-setup.md](windows-podman-setup.md) | Windows 11 host: WSL2 + Ubuntu, Podman in Ubuntu, the 16 fixes a fresh install needs, the Rust app in WSL, the coding agent in WSL, the abandoned Visual Studio route |
| [windows-podman-status-log.md](windows-podman-status-log.md) | Windows status log, prerequisites, memory budget, troubleshooting log |
| [installing-forms-14c.md](installing-forms-14c.md) | Download installers, response files, build the image, two-stage install, domain, `apply_forms14c_config.sh`, start the server (both platforms) |
| [dependencies.md](dependencies.md) | What the Oracle stack needs: host prerequisites, images, installers, every download with size, Ubuntu packages, Windows filesystem limits (the Rust dependencies are in the repo root `DEPENDENCIES.md`) |
| [database-23ai.md](database-23ai.md) | The Oracle Database Free container, seed schema, connection strings |
| [rust-workbench-setup.md](rust-workbench-setup.md) | Install and run the Rust web app on each platform, settings, tests |
| [oracle-reference-stack-map.md](oracle-reference-stack-map.md) | Every file and folder in `oracle-reference-stack/`, what it is for, `compose.yaml` explained |
| [oracle-home-location.md](oracle-home-location.md) | Why and how to put the Oracle Home on Ubuntu's disk (`provision_oracle_home.sh`) |
| [legacy-12c.md](legacy-12c.md) | The optional 12c Standalone setup |

## Running and testing

| Reference | What it covers |
|---|---|
| [starting-the-stack.md](starting-the-stack.md) | `start_testbed.py`: start, restart, and confirm everything; options; how to read the summary |
| [forms-and-reports-testing-guide.md](forms-and-reports-testing-guide.md) | Test the Forms server, FSAL, compiling, and Reports |
| [forms-builder-gui.md](forms-builder-gui.md) | Open the Forms Builder (`frmbld`) from the container (macOS XQuartz, Windows WSLg) and run a live form with FSAL |
| [reports-conversion.md](reports-conversion.md) | Oracle `rwconverter` and the native Rust Reports engine |
| [troubleshooting.md](troubleshooting.md) | Symptom, cause, fix for installing, starting, using, converting |

## Reference and history

| Reference | What it covers |
|---|---|
| [native-rust-workbench-architecture.md](native-rust-workbench-architecture.md), [rust-workbench-architecture.mmd](rust-workbench-architecture.mmd) | What the Rust workbench is: crates, UI, status, roadmap |
| [podman-architecture.mmd](podman-architecture.mmd) | Diagram of the containers, mounts, and network |
| [windows-wsl-change-log.md](windows-wsl-change-log.md) | Every change, error, and fix; add to it as you work |
