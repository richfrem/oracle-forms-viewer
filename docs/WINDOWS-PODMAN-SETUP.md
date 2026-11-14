# Windows Setup: Oracle Forms 14c + Oracle Database Free (Podman in WSL2)

> **Status: Forms 14c runs on Windows (WSL2 Podman) with a live sample form. Other steps are marked below.** This guide is written from a real setup run on a Windows 11 workstation. Each step is marked with its status. A step marked **Not yet run** has not been verified, and its commands may change once it is run.

**Architecture decision:** Podman runs **inside an Ubuntu WSL2 distribution**, not as Podman for Windows. This matches the macOS/Linux instructions in [PODMAN-CONVERTER-SETUP-AND-HISTORY.md](PODMAN-CONVERTER-SETUP-AND-HISTORY.md). Run container commands from the Ubuntu shell (`wsl`). Windows is used only for the FSAL desktop client and the Rust workbench.

**Why not the Podman-for-Windows path:** Podman for Windows created a separate Podman machine with its own VM, memory, and port handling. It conflicted with an earlier local Oracle 23ai install (port 1521 already in use). WSL-only keeps one Linux engine and one set of commands.

---

## 0. Status log

| Date | Step | Status | Notes |
|---|---|---|---|
| 2026-11-13 | Created branch `docs/windows-podman-setup` | Done | |
| 2026-11-13 | Baseline: WSL 2.7.8.0, Ubuntu distro (WSL 2) present | Done | Stopped at time of check |
| 2026-11-13 | Baseline: `%USERPROFILE%\.wslconfig` exists with `networkingMode=mirrored`, `dnsTunneling=true`, `firewall=true`, `autoProxy=true` | Done | No `memory=` line, so WSL default applies (about 50% of host RAM) |
| 2026-11-13 | Podman for Windows 5.7.0 present, machine started | Done | Machine resources reported about 15.4 GiB. Superseded by the WSL-only decision |
| 2026-11-13 | Created network `oracle-net` in Podman | Done | Created in the Podman-for-Windows machine. Must be recreated in Ubuntu WSL |
| 2026-11-13 | Pulled `container-registry.oracle.com/database/free:latest-lite` | Done (in Windows Podman) | Registry now lists **Oracle AI Database 26ai Free** under this path. Confirm the tag before reuse |
| 2026-11-13 | Start `oracle23ai-lite` on port 1521 | **Failed**: `rootlessport listen tcp4 0.0.0.0:1521: bind: address already in use` | Port held by a previous local Oracle install. Being removed by the user |
| 2026-11-13 | Old Windows Oracle install and 23ai Lite container removed | Done (user) | Port 1521 has no Windows listener (`Get-NetTCPConnection` empty) |
| 2026-11-13 | WSL updated from 2.7.8.0 to **3.0.5.0** | Done (user) | Latest per winget at time of check |
| 2026-11-13 | Downloads present in the user's Downloads folder, readable (`tar -tf`) | Done | `V1045135-01.zip` 2148 MB (contains `fmw_14.1.2.0.0_infrastructure.jar`); `V1045121-01.zip` 1356 MB (contains `fmw_14.1.2.0.0_fr_linux64.bin`). Part numbers not yet confirmed against the eDelivery listing |
| 2026-11-13 | Download JDK 17.0.12 (Linux x64) | Done | `jdk-17.0.12_linux-x64_bin.tar.gz`, 174 MB, readable. SHA-256 `311F1448312ECAB391FE2A1B2AC140D6E1C7AEA6FBF08416B466A58874F2B40F` (computed locally; not checked against Oracle's published hash) |
| 2026-11-13 | Copied the three downloads into `oracle-reference-stack/installer/` | Done | Confirmed ignored by git (`.gitignore` line 9). `git status` shows no installer files |
| 2026-11-13 | Extracted `V1045135-01.zip` and `V1045121-01.zip` in `installer/` with `tar -xf` | Done | Produced `fmw_14.1.2.0.0_infrastructure.jar` (2148 MB) and `fmw_14.1.2.0.0_fr_linux64.bin` (1357 MB) |
| 2026-11-13 | Extracted JDK with `tar -xzf` into `installer/jdk-17.0.12/` | Done | `Containerfile.14c` copies the tarball itself, which is still present |
| 2026-11-13 | Created `/run/user/1000` with sudo (one-time) | Done (user) | Required before any `podman build` or `podman run` in this WSL distro |
| 2026-11-13 | Built `localhost/forms-converter-14c:latest` in WSL | Done | `podman build --platform linux/amd64 -t forms-converter-14c:latest -f Containerfile.14c .` run from `oracle-reference-stack/` in Ubuntu via `/mnt/c/...`. Image 798 MB, based on Oracle Linux 8 slim, JDK 17.0.12 installed. Build ran as `linux/amd64` on x86-64 WSL (no emulation) |
| 2026-11-13 | Bind-mount check: `oracle-reference-stack/oracle_home_14c` (Windows folder) into the container as UID 1000 | **FAILED** | Write succeeded. `chmod +x` returned `Operation not permitted`. WSL mounts `C:\` over 9p with `access=client` and no `metadata` option. Test files removed afterward. Folder is empty |
| 2026-11-13 | Added `metadata,umask=22,fmask=11` to `[automount]` in `/etc/wsl.conf`; `wsl --shutdown` | Done (user) | Verified in `/proc/mounts` as `metadata` on `C:\` |
| 2026-11-13 | Bind-mount check repeated with metadata | **PASSED** | Write, `chmod +x`, exec, symlink, and mkdir all work as UID 1000. Required `chmod 777` on the Oracle Home folder on the Windows side, because rootless Podman maps container UID 1000 to a different host UID. Test files removed |
| 2026-11-13 | Restarted WSL cleared `/run/user/1000` | Known issue | Must recreate it with sudo after every `wsl --shutdown`, until a permanent fix is added |
| 2026-11-13 | Response files created as committed templates in `oracle-reference-stack/installer-templates/` (`oraInst.loc`, `infra14c_silent.rsp`, `forms14c_silent.rsp`) and copied to `installer/` | Done | LF endings verified. `INSTALL_TYPE` values are unverified against the installer |
| 2026-11-13 | Stage 1: `java -jar /installer/fmw_14.1.2.0.0_infrastructure.jar -silent -ignoreSysPrereqs -responseFile /installer/infra14c_silent.rsp -invPtrLoc /installer/oraInst.loc` | Done | Exit code 0, `Percent Complete : 100`, "installed successfully". Oracle Home is 2.8 GB (`wlserver`, `oracle_common`, `oui`, `inventory`, `OPatch` present). Took about 25 minutes. Prerequisite checks passed (CPU, swap 4096 MB, temp space). The installer's log copy to `/home/oracle/oraInventory` did not persist after `--rm`; the inventory lives inside the Oracle Home instead. The Oracle Home is readable only by UID 1000 on the host side, so Windows-side `du` fails with permission denied; check size from a container |
| 2026-11-13 | Stage 2: `/installer/fmw_14.1.2.0.0_fr_linux64.bin -silent -ignoreSysPrereqs -responseFile /installer/forms14c_silent.rsp -invPtrLoc /installer/oraInst.loc` | Done | Log: "Oracle Forms and Reports 14.1.2.0.0 completed successfully" (100%). Verified from inside a container: `forms/java/frmsal.jar` (386 KB) and `bin/frmcmp_batch` (17.9 MB) exist. First attempt failed with `Permission denied`: the extracted `.bin` lacked the execute bit. Fixed with `chmod +x` on the staged copy in `installer/` (WSL). `INSTALL_TYPE=Forms and Reports Deployment` worked. The installer process did not exit after success and the run ended with exit 139; the container was left behind, and it was gone on the next check. Check the install log, not only the exit code |
| 2026-11-13 | Response files (`oraInst.loc`, `infra14c_silent.rsp`, `forms14c_silent.rsp`) | Done | Committed as templates in `oracle-reference-stack/installer-templates/`; both install types accepted by the installer |
| 2026-11-13 | Podman inside Ubuntu WSL | Already installed: 4.9.3 (Ubuntu 24.04.3, x86_64). Newest in apt. Upgrade needs `sudo` password | `podman update` was not run; it changes container config, not Podman itself |
| 2026-11-13 | Oracle Database Free: `podman pull container-registry.oracle.com/database/free:latest-lite` in WSL | Done | Image runs as "Oracle AI Database 26ai Free", product version `23.0.0.0.0`. The repo's docs say 23ai; treat the banner as the source of truth |
| 2026-11-13 | Runtime dirs: `sudo mkdir -p /run/user/$(id -u)` (user), then `mkdir -p /run/user/$(id -u)/containers/networks` (no sudo) | Done | `networks` is required for `--network`. Without it: `IPAM error: failed to open database .../containers/networks/ipam.db` |
| 2026-11-13 | `XDG_RUNTIME_DIR` mismatch: login shells set `/mnt/wslg/runtime-dir`; the IPAM and exec steps failed until it was exported as `/run/user/<uid>` | Done | `export XDG_RUNTIME_DIR=/run/user/$(id -u)` before `podman run --network` or `podman exec` |
| 2026-11-13 | Started `oracle23ai-lite` on `oracle-net`, port 1521, `ORACLE_PWD=Welcome1` | Done | Container is `Up`. Log line `DATABASE IS READY TO USE!` after about 6 minutes on first start |
| 2026-11-13 | Seed schema: `oracle-reference-stack/sql/seed_forms_user.sql` run through `podman exec -i ... sqlplus` | Done | Created tablespace `FORMS_DATA` (no `USERS` tablespace exists in FREEPDB1), user `forms_user`, tables `departments` (3 rows), `employees` (6), `test_items` (5). Four queries returned expected rows. First-run `DROP USER` error is expected |
| 2026-11-13 | Login as `forms_user` in the container; host TCP to port 1521 | Done | `SELECT USER FROM dual` returns `FORMS_USER`; `count(*) test_items` = 5. Windows `TcpClient` to `127.0.0.1:1521` succeeded. `Test-NetConnection localhost` tried `::1` first and reported failure, then the IPv4 check passed |
| 2026-11-13 | Rust gate: `cargo check` and `cargo test` | **Blocked** | Link fixed with `LIB`. C compile of `aws-lc-sys` fails: `stdint.h` and `vcruntime.h` missing. The repair needs an elevated PowerShell (`vs_installer.exe modify ... --passive` exits 5007 otherwise). Commands are in `DEPENDENCIES.md` §3.1. The WSL Rust route (rustup, no Visual Studio) is the alternative |
| 2026-11-13 | Loaded sample demo data as `forms_user`: `samples/ofw_demo_employees.sql` (8 employees) and `samples/ofw_demo_sales.sql` (40 customers, 20 products, 140 orders, 420 order lines) | Done | `ofw_demo_sales.sql` prints `ORA-00942` for its four `DROP TABLE` lines on the first run. Expected, because the tables don't exist yet. Run the file as `sqlplus forms_user/Welcome1@localhost/FREEPDB1` (the file's header comment shows a password-less form, which prompts) |
| 2026-11-13 | WebLogic domain created with `wlst.sh create_forms_domain.py` in a throwaway `forms-converter-14c` container | Done | Output ends with "Domain Creation Complete". Domain at `oracle-reference-stack/oracle_home_14c/user_projects/domains/base_domain`. Run with `--network oracle-net` and both Oracle Home and repo mounts |
| 2026-11-13 | Oracle Home subfolders created by the container (owned by subordinate UID 100999) were unreadable from WSL. `frmsal.jar` and the domain folder returned `Permission denied` | Fixed | `podman unshare chmod -R a+rX oracle_home_14c` (run from `oracle-reference-stack/`; slow, several minutes) |
| 2026-11-13 | `boot.properties` (`weblogic` / `Welcome123#`) and `bin/setUserOverrides.sh` (LD_LIBRARY_PATH, FORMS_PATH, `-Dforms.instance=forms1`) written into the domain | Done (by hand) | Not yet committed as templates. Written LF with no BOM |
| 2026-11-13 | Started `forms-server-14c` from `base_domain/bin/startWebLogic.sh`, on `oracle-net`, ports 7001 and 9001 | Starting | Log shows the WebLogic launch line. Confirm `RUNNING` before FSAL |
| 2026-11-13 | `boot.properties` unreadable by the container (`BEA-090066 ... Permission denied`) because files written from Windows are owned by the host user | Fixed | `chmod -R a+rwX` on the `security` folder and `boot.properties` (run from WSL against the `/mnt/c` path). Restart the container after |
| 2026-11-13 | Server stayed in STARTING for about 18 minutes. A thread dump showed an execute thread in `com.oracle.cie.gdr` file-existence checks on the 9p-mounted Oracle Home, while Coherence threads were idle | Observed | Slow file I/O on `/mnt/c` is the likely cause. Reached RUNNING at 10:14 UTC. Moving the domain to the WSL ext4 filesystem may shorten this; not tested |
| 2026-11-13 | Mac history (`PODMAN-CONVERTER-SETUP-AND-HISTORY.md` §9 and §12.6): `fmrweb.res` must be under `forms/admin/resource/US`, and `ORACLE_PATH` must include that folder. Not applied before this run | Done (run 2) | Copied `fmrweb.res` and `fmrweb_utf8.res` from `forms/templates/resource/US/` into `forms/admin/resource/US/` inside the container (as `oracle`) |
| 2026-11-13 | `setUserOverrides.sh` now exports `FORMS_INSTANCE` (the forms1 instance folder), `ORACLE_PATH`, `TNS_ADMIN`, and `FORMS_PATH`, following `forms/templates/config/default.env`. Earlier versions did not set `FORMS_INSTANCE`, which that template uses to build its config paths | Done (run 2) | Hypothesis for `FRM-93131 basesaa.txt`: the lookup is relative to `FORMS_INSTANCE`, which was unset. Result in the next row || 2026-11-13 | `frmweb` exited at launch (`FRM-93552`). `ldd frmweb` listed 16 missing `lib*.so.0` (`libiplsn`, `libibfrmw`, `libiffw`, `libifcw`, `libijcw`, `libipfw`, `libimffrmw`, `libiwfw`, `libiwcw`, `libitw`, `libicw`, `libsosdw`, `libixw`, `libiicw`, `libiiiw`, `libiqw`). Only the static `.a` archives existed. `genshlib` had never run | Fixed | Ran `genshlib <name> 0` for each of the 16 inside the container (`/oracle_home_14c/bin/genshlib`, `gcc` and `ar` present in the image). All exit 0; `ldd frmweb` now reports none missing. Matches the Mac notes (§9, the 16-library row) |
| 2026-11-13 | `FRM-40010: Cannot read form ofw_demo_employees.fmx` on the shipped `.fmx`. The shipped `.fmx` was not compiled in this environment | Fixed by recompiling | Compiled `ofw_demo_employees.fmb` with the container's `frmcmp_batch` (no errors) and installed the result in `oracle_home_14c/forms/`. See the compile row below |
| 2026-11-13 | `frmcmp_batch` with `output_file=` into `/oracle_home_14c/forms` or `/tmp/job` fails with `FRM-10044: Cannot create file`. Copying the `.fmb` into `/tmp/job` and compiling there with no `output_file=` works | Workaround | Compile from a copy in a job directory; the output lands next to the copy. Keep compile output out of `/work` |
| 2026-11-13 | FSAL with `form=ofw_demo_employees.fmx` after the compile: the Forms window opens. The launcher was stopped by its 120-second timeout (exit 124) while the form was still running | Observed | Confirm on screen. Visual result is the user's to check || 2026-11-13 | `ORA-12154: Cannot find alias FREEPDB1 in .../config/fmwconfig/tnsnames.ora`. The domain's `tnsnames.ora` is the stock template with no entries | Fixed | Appended a `FREEPDB1` entry pointing at host `oracle-db-23ai` (the database container's network alias, resolvable from the server container on `oracle-net`), port 1521, service `FREEPDB1` |
| 2026-11-13 | `FRM-10221: Cannot read file OFW_EMP_MENU.mmx`. The shipped `.mmx` was stale | Fixed | Compiled `ofw_emp_menu.mmb` with `frmcmp_batch module_type=menu` in a job directory and copied the `.mmx` into the Oracle Home `forms/` folder |
| 2026-11-13 | `ORA-01017: invalid credential` at logon. The docs gave `Welcome123#` for `forms_user`, but the seed script sets `Welcome1` | Fixed | `HOW-TO-RUN.md` now says `Welcome1` for `forms_user`. `Welcome123#` stays the WebLogic admin password |
| 2026-11-13 | FSAL relaunch with `ofw_demo_employees.fmx`: the form window stayed open until the launcher's timeout, with no error in the log | Observed | Confirm on screen || 2026-11-13 | **Milestone: Employee Directory form runs on Windows.** FSAL from WSL with `ofw_demo_employees.fmx` (recompiled), `forms_user` / `Welcome1` / `FREEPDB1`, live rows displayed | Done | Full ordered recipe in `HOW-TO-RUN.md` section 7.5 and `references/windows-podman-setup.md` section 8 || 2026-11-13 | `forms-server-14c` reached RUNNING; `http://127.0.0.1:7001/forms/frmservlet` returns 200 | Done | |
| 2026-11-13 | FSAL from WSL (OpenJDK 21.0.9): `frmsal.jar` runs and connects to the server, then stops with `FRM-93131: Cannot find base HTML file basesaa.txt` | **Blocked** | `basesaa.txt` exists at `config/fmwconfig/components/FORMS/forms1/server/`. No `formsweb.cfg` exists in the domain, and no `standaloneapp` config section was found. A minimal `formsweb.cfg` with `[standaloneapp]` written by hand did not change the error. Needs the Forms web-app configuration, which the domain template should have created |
| Not yet run | Configure `.wslconfig` memory limit | Pending | Requires `wsl --shutdown`, which stops the Ubuntu distro. Needs user approval |

---

## 1. Prerequisites (Windows host)

| Item | Requirement | Verified on this machine |
|---|---|---|
| Windows | 11 (x64) | Windows 11 Enterprise 10.0.26200 |
| WSL | WSL2, version 2.x | WSL 2.7.8.0 |
| Ubuntu distro | Installed, version 2 | Ubuntu, WSL 2 |
| Host RAM | 16 GB+ (32 GB recommended) | 31.6 GB |
| Free disk | About 25 GB (installers, Oracle Home, images, DB) | About 88 GB free on C: |
| Java (FSAL client) | JDK 17 or 21 on Windows | Host has Java 26. Not yet verified with FSAL |

---

## 2. Memory budget for WSL2

WSL2 sizes its VM from `%USERPROFILE%\.wslconfig`. Podman's `--memory` option does **not** apply to the WSL VM.

**Target:** 12 GB for the VM, leaving room for Windows.

```ini
# %USERPROFILE%\.wslconfig
[wsl2]
memory=12GB
processors=8
swap=4GB
# Keep your existing lines below:
networkingMode=mirrored
dnsTunneling=true
firewall=true
autoProxy=true
```

Apply with `wsl --shutdown` from PowerShell. **This stops all WSL distributions, including Ubuntu.** Status: **Not yet run**, pending approval.

---

## 3. Install Podman inside Ubuntu WSL

Status: **Not yet run.** Commands below are the planned steps and may need adjustment.

From PowerShell:

```powershell
wsl -d Ubuntu
```

From the Ubuntu shell:

```bash
sudo apt-get update
sudo apt-get install -y podman
podman --version
```

Verify the container runtime:

```bash
podman run --rm --platform linux/amd64 docker.io/library/alpine uname -m
# Expected: x86_64 (native on an x86-64 host, no emulation)
```

---

## 4. Oracle Database Free

Status: **verified** (container starts, schema created, queries run, port 1521 reachable).

From Ubuntu, after the runtime-directory setup in §3:

```bash
export XDG_RUNTIME_DIR=/run/user/$(id -u)
mkdir -p /run/user/$(id -u)/containers/networks
podman network create oracle-net          # skip if it exists
podman run -d --name oracle23ai-lite --network oracle-net --network-alias oracle-db-23ai \
  -p 1521:1521 -e ORACLE_PWD=Welcome1 \
  container-registry.oracle.com/database/free:latest-lite
podman logs -f oracle23ai-lite            # wait for "DATABASE IS READY TO USE!" (about 6 minutes on first start)
```

Seed the test schema (run from `oracle-reference-stack/`):

```bash
podman exec -i oracle23ai-lite sqlplus -s system/Welcome1@localhost/FREEPDB1 < sql/seed_forms_user.sql
```

The script creates tablespace `FORMS_DATA`, user `forms_user`, and the tables `departments`, `employees`, and `test_items`. It loads sample rows and runs four queries. The first-run `ORA-01918` from `DROP USER` is expected.

Verify:

```bash
printf "SELECT USER FROM dual;\nSELECT COUNT(*) FROM test_items;\nEXIT\n" | podman exec -i oracle23ai-lite sqlplus -s forms_user/Welcome1@localhost/FREEPDB1
```

From PowerShell, `Test-NetConnection 127.0.0.1 -Port 1521` should report `TcpTestSucceeded : True`. Use `127.0.0.1` rather than `localhost`, which tries IPv6 `::1` first.

`Welcome1` is a documented local-development default. Change it for anything beyond a throwaway container.

**Image version:** the registry labels this image **Oracle AI Database 26ai Free**. `product_component_version` reports `Oracle AI Database 26ai Free`, version `23.0.0.0.0`. The repo's SQL and connection docs were written for 23ai. The connection and SQL here work with this image; the version label is a documentation difference, not a blocker.

---

## 5. Installers (Linux x86-64, on Windows)

Use the **Linux x86-64** files even on Windows. The container is Oracle Linux. The Windows `.exe` installer is for a native Windows install, which this guide does not use.

| File | Source | Status |
|---|---|---|
| `V1045135-01.zip` (FMW Infrastructure 14.1.2) | https://edelivery.oracle.com/ (Linux x86-64) | Downloading (user) |
| `V1045121-01.zip` (Forms & Reports 14.1.2) | https://edelivery.oracle.com/ (Linux x86-64) | Downloading (user). Part number not confirmed by search |
| `jdk-17.0.12_linux-x64_bin.tar.gz` | https://www.oracle.com/java/technologies/javase/jdk17-archive-downloads.html | Not yet downloaded. Direct URL may no longer resolve |

Staging into the gitignored `oracle-reference-stack/installer/` folder happens after download. Do not commit these files.

---

## 6. Later phases (not yet run)

- Build the Forms 14c image and run both silent installs (`Containerfile.14c`, `compose.yaml`).
- Check NTFS bind-mount behavior for `oracle_home_14c` (symlinks, execute bits, ownership) before the full install.
- Create the WebLogic domain and boot configuration.
- Run `health_check.py`, then compile and launch a sample form with FSAL.
- Run `cargo check` and `cargo test` from the repo root.

---

## 7. Troubleshooting log

| Symptom | Cause | Resolution |
|---|---|---|
| `rootlessport listen tcp4 0.0.0.0:1521: bind: address already in use` | An existing local Oracle listener holds port 1521 | Remove the old Oracle install and container before starting a new one (in progress) |
| `podman compose` fails: `docker-compose` not found | No compose provider installed | Use plain `podman run` commands, or install a compose provider in WSL (not yet decided) |
| `podman build` fails: `RunRoot is pointing to a path (/run/user/1000/containers) which is not writable` / `mkdir /run/user/1000: permission denied` | Podman in WSL writes its events and run state to `/run/user/<uid>`, which doesn't exist for this user. Exporting `XDG_RUNTIME_DIR` does not change this. Login shells set it to `/mnt/wslg/runtime-dir` | Create the directory once with sudo: `sudo mkdir -p /run/user/$(id -u) && sudo chown $(id -un): /run/user/$(id -u) && chmod 700 /run/user/$(id -u)`. **Status: awaiting the sudo step.** Do not run `podman system reset`; it deletes existing images |
| `database configuration mismatch` after passing `--runroot` / `--tmpdir` | Podman's database was created with a different run root. Mixing run roots causes this | Revert to the default run root and use the sudo fix above. Do not switch run roots on an existing store |
