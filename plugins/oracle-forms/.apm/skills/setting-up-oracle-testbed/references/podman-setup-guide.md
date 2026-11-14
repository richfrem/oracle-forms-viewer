# macOS (Apple Silicon): Podman host setup

For Windows 11 (Podman inside Ubuntu on WSL2) use [windows-podman-setup.md](windows-podman-setup.md). Everything after the host setup is the same on both platforms: [installing-forms-14c.md](installing-forms-14c.md), [database-23ai.md](database-23ai.md), [rust-workbench-setup.md](rust-workbench-setup.md). The ordered end-to-end list is [clean-clone-checklist.md](clean-clone-checklist.md).

**Why a container on a Mac.** Oracle Forms and its batch converters exist only for Linux x86-64 and Windows; there is no macOS or ARM64 release. Podman runs the official Linux installation in a `linux/amd64` container, translated by Rosetta 2 at near-native speed, so a Mac needs no Windows virtual machine or remote server. The Oracle software lives in a folder on your disk that is mounted into the container, not inside an image, so rebuilding images or recreating containers loses nothing.

Status: this is the original setup, run on an Apple Silicon Mac in early November 2026. It was **not re-run during the Windows work**.

## Your coding agent on macOS

Everything (Podman machine, Python, Java, Rust, and the agent) runs natively on the Mac, so there is no equivalent of the Windows WSL steps. `xmllint` (used by `validate_forms_xml.py`) ships with macOS.

## 0. Create the `.env` file

Every later step reads its values from `.env` at the repo root, which git ignores. Never commit it; never type passwords into commands or docs.

```bash
python3 plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/scripts/create_env.py
set -a; source .env; set +a
```

The script copies `example.env` to `.env` and prompts for `ORACLE_PWD`, `DB_USER`, `DB_PASSWORD`, `DB_SERVICE`, and `WLS_ADMIN_PASSWORD` (passwords with hidden input). It also asks for the optional `OFW_ORACLE_HOME` (the folder holding the Oracle Home, see [oracle-home-location.md](oracle-home-location.md)); press Enter to keep the default, and do not set it before that folder exists and `provision_oracle_home.sh --verify` reports VERIFIED. Load it with the second line in each shell.

## 1. Install Podman and the machine

```bash
brew install podman
```

Create the Podman machine with enough resources for the Oracle tools (the original setup used 4 vCPUs, 8 GB RAM, 50 GB disk; WebLogic is the heaviest part, and more memory was not tested):

```bash
podman machine init --cpus 4 --memory 8192 --disk-size 50
podman machine start
```

To start over from an existing default machine, `podman machine rm -f podman-machine-default` first. **That deletes the machine and everything in it.** Ask before running it on a machine you did not just create.

## 2. Check that Rosetta 2 runs `linux/amd64`

```bash
podman run --rm --platform linux/amd64 alpine uname -m      # expect: x86_64
```

`x86_64` means Rosetta 2 translation is working. Always pass `--platform linux/amd64` to `podman run` and `podman build`. Without it Podman prints `WARNING: image platform (linux/amd64) does not match the expected platform (linux/arm64)` before running the image anyway.

## 3. Display for the Forms Builder (optional)

The Forms Builder (`frmbld`) is an X11 application. On macOS it needs XQuartz (`brew install --cask xquartz`, allow network client connections, then `xhost`). The steps are in [forms-builder-gui.md](forms-builder-gui.md). FSAL (the desktop Forms client) needs a host Java, 17 or 21: `brew install openjdk@17`.

## 4. Continue

1. Create the network and the database: [database-23ai.md](database-23ai.md).
2. Install Forms 14c: [installing-forms-14c.md](installing-forms-14c.md).
3. Install and run the Rust workbench: [rust-workbench-setup.md](rust-workbench-setup.md).
4. Test: [forms-and-reports-testing-guide.md](forms-and-reports-testing-guide.md) and `scripts/health_check.py`.

## Restarting later

After the first setup, one command from the repo root starts everything and confirms it: `python3 start_testbed.py`. It starts the Podman machine if it is stopped, starts `oracle23ai-lite` and `forms-server-14c` (about 2 minutes to reach RUNNING under Rosetta 2), builds and starts the Rust app, and prints a summary table. `python3 start_testbed.py --status` only reports.

## The original 12c setup

The first containers used Forms 12c Standalone with OpenJDK 8. That is optional and superseded; the facts worth keeping are in [legacy-12c.md](legacy-12c.md).
