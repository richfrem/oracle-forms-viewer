# Map of `oracle-reference-stack/`

What every file and folder in the repo's `oracle-reference-stack/` is for, and which page of this skill uses it. This page is the folder's documentation; the folder's own `README.md` points here.

## Contents

- [Files and folders](#files-and-folders)
- [compose.yaml](#composeyaml)
- [Git status](#git-status)

## Files and folders

| Entry | What it is | Used by |
|---|---|---|
| `Containerfile.14c` | Recipe for the Forms 14c image `forms-converter-14c`: Oracle Linux 8, Oracle JDK 17, X11/Motif, `libnsl`/`libnsl2`/`libaio`, build tools, the non-root `oracle` user. **No Oracle software inside** | [installing-forms-14c.md](installing-forms-14c.md) step 4 |
| `Containerfile` | The same for the legacy 12c image `forms-converter` (OpenJDK 8) | [legacy-12c.md](legacy-12c.md) |
| `installer-templates/` | The three committed response files `oraInst.loc`, `infra14c_silent.rsp`, `forms14c_silent.rsp`, copied into `installer/` | [installing-forms-14c.md](installing-forms-14c.md) step 3 |
| `installer/` | Where you stage the downloaded Oracle installers and the copied response files. Git-ignored | [installing-forms-14c.md](installing-forms-14c.md) steps 2 and 5 |
| `oracle_home_14c/` | The installed Forms and Reports 14c and the WebLogic domain, plus all configuration. Mounted as `/oracle_home_14c`. Git-ignored, about 5.4 GB. **On Windows do not install here** (slow Windows drive): set `OFW_ORACLE_HOME` to a folder on Ubuntu's disk before installing. Its location is that setting | [oracle-home-location.md](oracle-home-location.md) |
| `oracle_home/` | The same for the 12c install (only on machines that set it up). Git-ignored | [legacy-12c.md](legacy-12c.md) |
| `apply_forms14c_config.sh` | Applies every post-install adjustment to the Oracle Home: domain hooks, `boot.properties` from `.env`, `tnsnames.ora` alias, terminal resources, `frmweb` and `libfrmjapi` libraries, Reports preference and printer files, sample compiles | [installing-forms-14c.md](installing-forms-14c.md) step 7 |
| `forms14c-config/` | Inputs of that script: `setUserOverrides.sh`, `tnsnames-FREEPDB1.entry`, and `fingerprint.sh` (used by the provisioning script) | the two scripts |
| `restore_oracle_home_attributes.sh` | Repairs an Oracle Home copied with Windows Explorer: restores execute permissions, symbolic links, owner, and any missing files, using the original as the reference. Not yet run | [oracle-home-location.md](oracle-home-location.md) |
| `provision_oracle_home.sh` | Creates, copies, verifies, and backs up the Oracle Home on a chosen disk; `--dry-run` first | [oracle-home-location.md](oracle-home-location.md) |
| `sql/seed_test_schema.sql` | Creates the test schema owner, tablespace, and tables; takes the owner and password as variables | [database-23ai.md](database-23ai.md) |
| `compose.yaml` | A declarative description of the containers and the install steps (see below) | the original macOS route; not used on Windows |
| `README.md` | Short overview of the folder, pointing here | |

## compose.yaml

It describes five services: `oracle-db-23ai` (the database, container `oracle23ai-lite`, port 1521), `forms-converter-12c`, `infra-install-14c` and `forms-install-14c` (the two 14c silent installs), and `forms-server-14c` (WebLogic, ports 7001 and 9001). The Oracle Home mount source is `${OFW_ORACLE_HOME:-./oracle_home_14c}`.

**The verified Windows procedure does not use it; the original macOS procedure did.** The Windows setup uses plain `podman run` commands (and `start_testbed.py` to start the containers), because Podman Compose is not installed there and the commands are the same on both platforms. The compose route (`podman compose ... run --rm infra-install-14c`, `forms-install-14c`, `up -d forms-server-14c`) is described in [installing-forms-14c.md](installing-forms-14c.md) and was **not verified** in the Windows sessions. Known problems in the file, not fixed:
- Fixed: `ORACLE_PWD` is read from `.env` (load it first, see the stack map's compose notes).
- `version: '3.8'` is obsolete.
- `DISPLAY=host.containers.internal:0` is the macOS (XQuartz) setting; WSLg uses `DISPLAY=:0` with a socket mount ([forms-builder-gui.md](forms-builder-gui.md)).
- Compose would name the network with a project prefix, not `oracle-net`; **not verified**.

Use it as a reference for ports, mounts, and environment variables, or fix those points before relying on it.

## Git status

Committed: `Containerfile`, `Containerfile.14c`, `compose.yaml`, `installer-templates/`, `forms14c-config/`, `sql/`, `apply_forms14c_config.sh`, `provision_oracle_home.sh`, `README.md`. Git-ignored (never commit): `installer/`, `oracle_home_14c/`, `oracle_home/`, and any `.env`.
