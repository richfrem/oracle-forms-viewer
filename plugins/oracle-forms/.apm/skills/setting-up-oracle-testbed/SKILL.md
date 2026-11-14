---
name: setting-up-oracle-testbed
description: "Provisions and configures the complete Oracle Forms 14c and Database 23ai Free container testbed on macOS or on Windows 11 (everything inside WSL2). Use when setting up the database, building the Forms 14c container, relinking runtime libraries, provisioning WebLogic domains, running the Rust workbench next to the testbed, or troubleshooting container errors."
---

# Setting Up Oracle Testbed

Provisions and configures the complete Oracle Forms 14c and Database 23ai Free container testbed on macOS or on Windows 11. On Windows, **everything runs inside WSL2**: Podman and the containers, Python, Java 21, and the Rust workbench itself. Use when setting up the database, building the Forms 14c container, relinking runtime libraries, provisioning WebLogic domains, running the Rust workbench next to the testbed, or troubleshooting container errors.

## Choose your platform first

| Host | Runbook | Notes |
| :--- | :--- | :--- |
| macOS (Apple Silicon) | [references/podman-setup-guide.md](references/podman-setup-guide.md) | Podman machine with Rosetta 2 (`linux/amd64`) |
| Windows 11 | [references/windows-podman-setup.md](references/windows-podman-setup.md) | **Everything inside Ubuntu on WSL2** (x86-64, no emulation): Podman, the containers, Python, Java 21, and the Rust app. No Visual Studio needed. Use `Linux x86-64` Oracle installers, not the Windows `.exe`. Full status log: [docs/WINDOWS-PODMAN-SETUP.md](../../../../../../docs/WINDOWS-PODMAN-SETUP.md) |

### Windows runbook (everything inside WSL2)

Run every command below from the Ubuntu shell (`wsl -d Ubuntu`), from the repo root, with `.env` loaded. The commands are the same as on macOS. This is the verified setup; [references/windows-podman-setup.md](references/windows-podman-setup.md) has the detail for each step, section 10 lists every fix a fresh install needs, and section 11 covers the Rust app.

1. **Host:** WSL2 with Ubuntu 24.04, `/etc/wsl.conf` `[automount] options = "metadata,umask=22,fmask=11"` (then `wsl --shutdown`), Podman 4.9 in Ubuntu, and `sudo mkdir -p /run/user/$(id -u)` once (repeat after every `wsl --shutdown`). Section 2 and 3 of the reference.
2. **`.env`:** `python3 scripts/create_env.py` (step 0 below).
3. **Installers and images:** Oracle Linux x86-64 installers in `oracle-reference-stack/installer/`; build `forms-converter-14c`; run the two silent installs. Sections 4 to 6 of the reference.
4. **Database:** start `oracle23ai-lite` and seed the schema (step 2 below). Section 7.
5. **Forms server:** create the WebLogic domain, then the domain hooks (`setUserOverrides.sh`, `boot.properties`, `instances/forms1` symlink, `fmrweb` resources), `tnsnames.ora`, and the runtime libraries (`genshlib` for 16 `frmweb` libraries; Oracle's makefile for `libfrmjapi.so.0`). Start `forms-server-14c`; it takes 15 to 45 minutes to reach RUNNING. Sections 8 and 10.
6. **Rust app, inside WSL too:** install Rust with `rustup`, then `cargo run -p oracle-forms-server` with `CARGO_TARGET_DIR=$HOME/ofw-target` (build output on WSL's own disk). Section 11 and `HOW-TO-RUN.md` section 6.
7. **Launch a form:** **Save & launch in Oracle** in the app (it updates the one file `samples/<form>_fmb.xml`; no copies), or the manual FSAL steps in `HOW-TO-RUN.md` section 7.5, or `python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/launch_fsal.py <form>.fmx --interactive`.
8. **Restart later:** `python3 start_testbed.py` (step 7 of the workflow below), or the restart steps in `HOW-TO-RUN.md` section 6.

Windows rules:
- Never run the Rust app as a native Windows build unless you cannot use WSL for it; that path needs Visual Studio and is documented only as an alternative (`docs/DEPENDENCIES.md` section 3.1).
- Use Java 17 or 21. WSL's OpenJDK 21 works. Windows Java 26 fails with `NoClassDefFoundError: java/applet/AppletStub`.
- Keep `.sh`, `.rsp`, and `.loc` files LF-only; do not write them with PowerShell heredocs.
- Container commands that write files on the Windows drive run as `root` (rootless Podman maps the container user to another UID).
- Do not trigger **Save & launch in Oracle** to test things; every click rewrites the form's XML. Use the manual FSAL steps.
- Keep the status log in `docs/WINDOWS-PODMAN-SETUP.md` current as steps run, and mark anything not run as not verified.

Not yet verified on Windows: `health_check.py`, `compile_form.py`, `convert_fmb.py --check`, `ofw_sales.fmb`, the `frmbld` GUI, and whether the root `exec` rule is needed inside WSL.

Host prerequisites a Windows setup must check (full list: [docs/DEPENDENCIES.md](../../../../../docs/DEPENDENCIES.md) sections 2.1, 2.4-2.6):
- WSL 2 current (`wsl --version`, verified 3.0.5.0) and an Ubuntu 24.04 distro with Python 3.12, `gcc`, and OpenJDK 21.
- Podman inside Ubuntu (`podman --version`, verified 4.9.3). Create `/run/user/$(id -u)` with sudo once and after every `wsl --shutdown`. Without it, every `podman build` and `podman run` fails.
- `metadata` in the `[automount]` options of `/etc/wsl.conf`, so `chmod +x` works on `/mnt/c` (verified: it fails without it).
- Rust in Ubuntu (`rustup`, minimal profile; 1.100.0 verified) for the app. No Visual Studio.
- Oracle installers in `oracle-reference-stack/installer/` (Linux x86-64 zips, JDK 17 tarball). Never commit them.
- Optional: `%USERPROFILE%\.wslconfig` with `memory=12GB` (applies to all distros; needs `wsl --shutdown`). Not required; the default worked.
## Contents

- [Quick start](#quick-start)
- [Workflow](#workflow)
- [Verification](#verification)

## Quick start

Run from this skill's root. The first step creates the git-ignored `.env` that every later command reads:

```bash
python3 scripts/create_env.py
```

After the first setup, bring the whole stack up (or back after a reboot) from the repo root with `python3 start_testbed.py` (`python` from Windows PowerShell). It starts the database and Forms (Fusion Middleware) server containers and starts the Rust app if it is not already running. See step 7 below.

## Workflow

Every step reads its credentials from `.env` at the repo root. Create that file first, and never type passwords into commands, docs, or commits.

0. **Create `.env`** (once per machine): `python3 scripts/create_env.py`. It copies `example.env` to `.env`, prompts for the values with hidden input for passwords, and checks that git ignores `.env`. Variables: `ORACLE_PWD` (database admin), `DB_USER`, `DB_PASSWORD`, `DB_SERVICE` (test schema), `OFW_DB_USER` and `OFW_DB_PASSWORD` (read by the Rust server and CLI), `WLS_ADMIN_USER` and `WLS_ADMIN_PASSWORD` (WebLogic admin).
1. **Load `.env`** in every shell before the commands below: `set -a; source .env; set +a`. Windows PowerShell does not read `.env`; set the variables by hand.
2. **Database and schema:** start the database container, then seed the test schema. The seed script takes the schema owner and password from `DB_USER` and `DB_PASSWORD`:
   ```bash
   (printf 'DEFINE db_user=%s\nDEFINE db_password=%s\n' "$DB_USER" "$DB_PASSWORD"; cat oracle-reference-stack/sql/seed_test_schema.sql) | podman exec -i oracle23ai-lite sqlplus -s system/$ORACLE_PWD@localhost/FREEPDB1
   ```
3. **WebLogic domain:** pass `.env` to the throwaway container so `create_forms_domain.py` can read `WLS_ADMIN_PASSWORD`. Then add the domain hooks, `tnsnames.ora`, and runtime libraries (Windows: `references/windows-podman-setup.md` sections 8 and 10):
   ```bash
   podman run --rm --platform linux/amd64 --network oracle-net --env-file .env -v "$PWD/oracle-reference-stack/oracle_home_14c:/oracle_home_14c:Z" -v "$PWD:/work:Z" forms-converter-14c:latest /oracle_home_14c/oracle_common/common/bin/wlst.sh /work/plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/scripts/create_forms_domain.py
   ```
4. **Health check:** `python3 scripts/health_check.py` reads `DB_USER`, `DB_PASSWORD`, and `DB_SERVICE` itself. It does not need `source .env`, but it does need the `.env` file at or above the script folder.
5. **Launch a form:** the FSAL logon uses the same values: user `DB_USER`, password `DB_PASSWORD`, service `DB_SERVICE`. The `compile_form.py`, `run_form.py`, `launch_fsal.py`, and `test_parity.py` scripts read them from `.env` as well.
6. Check that the file is ignored before any commit: `git check-ignore -v .env` must print a match. Never copy `.env` values into docs, logs, or scripts.

7. **Start and restart later:** once the containers exist, run `python start_testbed.py` from the repo root. It recreates the Podman runtime directory (Windows/WSL: asks for the WSL `sudo` password), starts `oracle23ai-lite`, waits for the database, starts `forms-server-14c` (15 to 45 minutes to reach RUNNING), builds and starts the Rust app if it is not already running, and prints the status. It never stops a running Rust app unless you pass `--restart-rust`. Run inside WSL it builds in WSL (`CARGO_TARGET_DIR=~/ofw-target`). Options: `--no-forms` (database and Rust app only), `--no-wait-forms`, `--no-rust`, `--no-containers`, `--restart-rust`, `--launch <form>.fmx`, `--status`, `--stop-rust`. It never creates or replaces containers. Do not run it on your own initiative: it starts the user's containers, and on Windows it needs the user's password.

Verify the output of each step before reporting success.
## Verification

Create at least three representative task scenarios and compare against a baseline.
Keep routing outcomes in `evals/evals.json` and task-success cases separately.
Report executed checks and any missing model coverage.

<!-- Author: implement the canonical script before publishing; --help alone is not a
task-success check. Put critical constraints early, name required dependencies and
link task-specific references directly. -->
