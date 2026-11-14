# Windows Podman Setup (Podman inside WSL2)

**Host:** Windows 11 x64 with an Ubuntu 24.04 WSL2 distribution
**Container architecture:** `linux/amd64`, native on an x86-64 host (no emulation)
**Companion references:** [podman-setup-guide.md](podman-setup-guide.md) (macOS, Apple Silicon with Rosetta 2) · [windows-podman-status-log.md](windows-podman-status-log.md) (full status log, troubleshooting table, and per-step evidence)

Status: **verified end to end on Windows with everything inside WSL2**: the containers, the Forms server, the Rust app, and **Save & launch in Oracle**. Section 11 is the way to run the Rust app. The native Windows build with Visual Studio and C++ was tried first and was a red herring (section 12). Section 10 lists every fix a fresh Windows install needs.

---

> **`$OH` below is the Oracle Home folder:** `export OH="${OFW_ORACLE_HOME:-$PWD/oracle-reference-stack/oracle_home_14c}"` (run it from the repo root, after loading `.env`). **On Windows, set `OFW_ORACLE_HOME` in `.env` to a folder on Ubuntu's own disk, for example `/home/<you>/oracle_home_14c`, BEFORE you install. Never install into the repo folder (`oracle-reference-stack/oracle_home_14c`): it is on the slow Windows drive and WebLogic takes 15 to 44 minutes to boot.** Why and how: [oracle-home-location.md](oracle-home-location.md).

## Contents

- [1. Rules for this platform](#1-rules-for-this-platform)
- [0. Step 0: create the .env file](#0-step-0-create-the-env-file)
- [1b. Restarting later (after a reboot or `wsl --shutdown`)](#1b-restarting-later-after-a-reboot-or-wsl---shutdown)
- [2. Host prerequisites](#2-host-prerequisites)
- [3. Install Podman in Ubuntu](#3-install-podman-in-ubuntu)
- [4. Oracle installers and the Oracle Home](#4-oracle-installers-and-the-oracle-home)
- [5. Build the Forms 14c image](#5-build-the-forms-14c-image)
- [6. Install Forms 14c (two stages, run as one-shot containers)](#6-install-forms-14c-two-stages-run-as-one-shot-containers)
- [7. Oracle Database Free](#7-oracle-database-free)
- [8. WebLogic domain, Forms server, and sample forms](#8-weblogic-domain-forms-server-and-sample-forms)
- [9. Troubleshooting](#9-troubleshooting)
- [10. Checklist: every Windows adjustment a fresh install needs](#10-checklist-every-windows-adjustment-a-fresh-install-needs)
- [11. Run the Rust app inside WSL (recommended)](#11-run-the-rust-app-inside-wsl-recommended)
- [12. The first approach, abandoned: native Windows build with Visual Studio](#12-the-first-approach-abandoned-native-windows-build-with-visual-studio)
- [13. How the Rust app launches Fusion Middleware, and what must be true first](#13-how-the-rust-app-launches-fusion-middleware-and-what-must-be-true-first)
- [14. Run your AI coding agent inside WSL too](#14-run-your-ai-coding-agent-inside-wsl-too)

## 1. Rules for this platform

- **Credentials live in .env, not in docs.** Step 0 below creates it. Run `python3 scripts/create_env.py` from the skill root, or copy `example.env` to `.env` at the repo root and fill in the values. Load it in each Ubuntu shell with `set -a; source .env; set +a`. Pass it to containers with `--env-file`.
- Run **all container commands from the Ubuntu shell** (`wsl -d Ubuntu`), not PowerShell. Podman runs inside WSL. Podman for Windows is not used. If it is also installed (`podman.exe`, with a WSL distro `podman-machine-default` in `wsl -l -v`), ignore it: that PowerShell `podman` does not see the containers, which live in the Ubuntu distro's own Podman. Do not start `podman-machine-default`.
- **Run the Rust app inside WSL too** (section 11). It needs no Visual Studio, and it uses the same code path as macOS.
- **Never install or copy the Oracle Home into the repo folder or with Windows Explorer.** Install onto Ubuntu's disk (`OFW_ORACLE_HOME`); a Windows copy loses execute permissions and symbolic links.
- Use the **Linux x86-64** Oracle installers (`.zip` and `.bin`), never the Windows `.exe`. The container is Oracle Linux.
- Keep shell scripts and `.rsp`/`.loc` files **LF-only**. Do not write them with PowerShell here-strings.
- Use `curl.exe` in PowerShell, not the `curl` alias (it runs `Invoke-WebRequest`).
- Run repo paths through WSL: `/mnt/c/Users/<you>/source/repos/oracle-forms-workbench`. Never put absolute machine paths into committed docs; use `$PWD` or the repo-relative path.

---

## 0. Step 0: create the .env file

Do this first. Every later step reads its values from `.env` at the repo root, which git ignores.

```bash
cd plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed
python3 scripts/create_env.py
```

The script copies `example.env` to `.env`, then prompts for the database admin password (`ORACLE_PWD`), the test user and its password (`DB_USER`, `DB_PASSWORD`), the database service (`DB_SERVICE`), and the WebLogic admin password (`WLS_ADMIN_PASSWORD`). Passwords are entered with hidden input. It also asks for the optional `OFW_ORACLE_HOME` (the folder holding the Oracle Home, see [oracle-home-location.md](oracle-home-location.md)); press Enter to keep the default, and do not set it before that folder exists and `provision_oracle_home.sh --verify` reports VERIFIED. Back at the repo root, load the file in an Ubuntu shell:

```bash
cd /mnt/c/Users/<you>/source/repos/oracle-forms-workbench
set -a; source .env; set +a
```

In Windows PowerShell, `.env` is not read automatically. Set the variables by hand.

---

## 1b. Restarting later (after a reboot or `wsl --shutdown`)

After the first setup, one command from the repo root brings everything back (use `python` in PowerShell, `python3` in the Ubuntu shell):

```bash
python start_testbed.py              # database + Forms (Fusion Middleware) server + Rust app
python start_testbed.py --no-forms   # database + Rust app only (fast)
python start_testbed.py --status     # what is running
```

On Windows it runs Podman through WSL (`--distro` sets the distro name; default `Ubuntu`). If `/run/user/<uid>` is missing, it recreates it with `sudo` and prompts for your WSL password in the terminal. It starts the existing `oracle23ai-lite` and `forms-server-14c` containers, waits for the database, rebuilds and restarts the Rust app in the background (log: `temp/oracle-forms-server.log`), and waits for the Forms server to reach RUNNING. It does not create containers; run the setup steps below for that.

---

## 2. Host prerequisites

| Item | Requirement | Check |
|---|---|---|
| WSL | 2.x; this setup was verified on 3.0.5.0 | `wsl --version` (PowerShell). Update with `wsl --update` |
| Ubuntu distro | Ubuntu 24.04, WSL 2 | `wsl -l -v` |
| RAM | 16 GB minimum; 32 GB recommended | Target 12 GB for the WSL VM (see §3) |
| Free disk | About 25 GB (installers ~4 GB, Oracle Home ~3 GB, images, database) | `Get-PSDrive C` |
| Podman | Inside Ubuntu, 4.9.3 verified | `podman --version` |
| Host Java (FSAL only) | JDK 17 or 21. Host Java 26 is **unverified** with `frmsal.jar` | `java -version` |

### `%USERPROFILE%\.wslconfig` (applies to every WSL distro)

```ini
[wsl2]
memory=12GB
processors=8
swap=4GB
networkingMode=mirrored
dnsTunneling=true
firewall=true
autoProxy=true
```

Changing this requires `wsl --shutdown`, which stops every distro. Back up any existing file first. Status: `memory=` is **not yet set** on the verified machine.

### `/etc/wsl.conf` inside Ubuntu

```ini
[network]
generateResolvConf = false

[automount]
enabled = true
options = "metadata,umask=22,fmask=11"
```

`metadata` is required. Without it, `/mnt/c` rejects `chmod +x`, and the Oracle installers and scripts fail with `Operation not permitted`. Apply with `wsl --shutdown` from PowerShell, then reopen Ubuntu.

---

## 3. Install Podman in Ubuntu

If there is no Ubuntu distro yet, install one from PowerShell: `wsl --install -d Ubuntu-24.04`, then reopen it and create the Linux user. Then, in Ubuntu, install the tools every later step uses. The Podman install is verified (4.9.3). The other package names are what a fresh Ubuntu 24.04 needs for this setup (`python3` for the scripts, `gcc` and `make` for building Rust dependencies, `unzip` for the installers, OpenJDK 21 for FSAL); the exact Java package name was not recorded when this was first set up, so treat `openjdk-21-jre` as **not verified**:

```bash
sudo apt-get update                      # always first: a stale index gives "404 Not Found" on the install (change log, 2026-11-14)
sudo apt-get install -y podman python3 build-essential unzip openjdk-21-jre libxml2-utils   # libxml2-utils: xmllint for validate_forms_xml.py (optional)
podman --version
python3 --version      # 3.12 on Ubuntu 24.04
java -version          # 21; Windows Java 26 does not work with FSAL
```

**Runtime directory (one-time, and again after each `wsl --shutdown`):** Rootless Podman writes its state to `/run/user/<uid>`. WSL does not create that directory, and restarting WSL clears it. Without it, every `podman run` and `podman build` fails with `RunRoot ... not writable` or `mkdir /run/user/1000: permission denied`.

```bash
sudo mkdir -p /run/user/$(id -u) && sudo chown $(id -un): /run/user/$(id -u) && chmod 700 /run/user/$(id -u)
mkdir -p /run/user/$(id -u)/containers/networks
```

The `containers/networks` folder is required for `podman network create` and for any container with `--network`. Without it, `podman run --network oracle-net` fails with `IPAM error: failed to open database .../containers/networks/ipam.db: no such file or directory`.

Podman also reads `XDG_RUNTIME_DIR`. Login shells in WSL set it to `/mnt/wslg/runtime-dir`, which does not match the directory above. Export it to the matching path for the same shell before any `podman` command that uses networks:

```bash
export XDG_RUNTIME_DIR=/run/user/$(id -u)
```

With that export, `podman run --network oracle-net ...` succeeded on this machine. The sudo step is needed once per `wsl --shutdown`; the `mkdir` for `containers/networks` and the `export` are needed per shell session.

A permanent fix (for example, enabling systemd in `/etc/wsl.conf`) is not yet verified. Do not run `podman system reset`; it deletes existing images.

---

## 4. Oracle installers and the Oracle Home

Put these in `oracle-reference-stack/installer/` (gitignored; never commit):

| File | Source |
|---|---|
| `V1045135-01.zip` → `fmw_14.1.2.0.0_infrastructure.jar` | Oracle eDelivery, Linux x86-64 |
| `V1045121-01.zip` → `fmw_14.1.2.0.0_fr_linux64.bin` | Oracle eDelivery, Linux x86-64 |
| `jdk-17.0.12_linux-x64_bin.tar.gz` | Oracle JDK 17 archive, Linux x64 |

Extract the two zips in place with `unzip` or `tar -xf` from Ubuntu.

**Response files:** copy the committed templates from `oracle-reference-stack/installer-templates/` into `installer/` (`oraInst.loc`, `infra14c_silent.rsp`, `forms14c_silent.rsp`). Do not write them from PowerShell.

**Execute bit:** the extracted `.bin` lacks it. Set it on the staged copy from Ubuntu before the Forms install:

```bash
chmod +x oracle-reference-stack/installer/fmw_14.1.2.0.0_fr_linux64.bin
```

**Oracle Home:** `oracle-reference-stack/oracle_home_14c/` on the Windows drive. *(The Windows drive is slow for WebLogic's boot; the Oracle Home can instead live on Ubuntu's own disk, set with `OFW_ORACLE_HOME` and created or copied by `provision_oracle_home.sh`. Why, the protocol, and how to verify the copy: [oracle-home-location.md](oracle-home-location.md). Not yet run.)* The rootless container user (UID 1000) maps to a different host UID, so the folder needs `chmod 777` from Ubuntu before the first install:

```bash
mkdir -p "$OH" && chmod 777 "$OH"
```

After the install, the files inside belong to the mapped UID and are not readable from Windows. Check their size and contents from a container, not from Windows.

---

## 5. Build the Forms 14c image

```bash
cd /mnt/c/Users/<you>/source/repos/oracle-forms-workbench/oracle-reference-stack
podman build --platform linux/amd64 -t forms-converter-14c:latest -f Containerfile.14c .
```

Verified: 798 MB image, Oracle Linux 8 slim, JDK 17.0.12.

---

## 6. Install Forms 14c (two stages, run as one-shot containers)

Run from `oracle-reference-stack/` in Ubuntu. Replace `$PWD` paths with the absolute `/mnt/c/...` path if your shell does not expand them inside `bash -c`.

```bash
# Stage 1: FMW Infrastructure 14.1.2.0.0
podman run --rm --platform linux/amd64 \
  -v $PWD/installer:/installer:Z \
  -v $OH:/oracle_home_14c:Z \
  forms-converter-14c:latest \
  java -jar /installer/fmw_14.1.2.0.0_infrastructure.jar \
    -silent -ignoreSysPrereqs \
    -responseFile /installer/infra14c_silent.rsp \
    -invPtrLoc /installer/oraInst.loc

# Stage 2: Forms and Reports 14.1.2.0.0 (after chmod +x on the .bin)
podman run --rm --platform linux/amd64 \
  -v $PWD/installer:/installer:Z \
  -v $OH:/oracle_home_14c:Z \
  forms-converter-14c:latest \
  /installer/fmw_14.1.2.0.0_fr_linux64.bin \
    -silent -ignoreSysPrereqs \
    -responseFile /installer/forms14c_silent.rsp \
    -invPtrLoc /installer/oraInst.loc
```

Verified results on this machine:

- **Stage 1:** about 25 minutes. Logs `Percent Complete : 100` and "installed successfully". Oracle Home is 2.8 GB.
- **Stage 2:** logs "completed successfully". `forms/java/frmsal.jar` (386 KB) and `bin/frmcmp_batch` (17.9 MB) are present.
- **Known quirk:** the Stage 2 process did not exit after it reported success, and the run ended with exit code 139. Check the installer log, not only the exit code. Verify the files afterwards.

Verify the install from inside a container:

```bash
podman run --rm --platform linux/amd64 \
  -v $OH:/oracle_home_14c:Z \
  forms-converter-14c:latest \
  ls -l /oracle_home_14c/forms/java/frmsal.jar /oracle_home_14c/bin/frmcmp_batch
```

---

## 7. Oracle Database Free

Status: **verified** (container starts, schema created, queries run, port 1521 reachable).

```bash
export XDG_RUNTIME_DIR=/run/user/$(id -u)
mkdir -p /run/user/$(id -u)/containers/networks
podman network create oracle-net          # skip if it exists
podman run -d --name oracle23ai-lite --network oracle-net --network-alias oracle-db-23ai \
  -p 1521:1521 -e ORACLE_PWD=$ORACLE_PWD \
  container-registry.oracle.com/database/free:latest-lite
podman logs -f oracle23ai-lite            # wait for "DATABASE IS READY TO USE!" (about 6 minutes on first start)
```

Seed the schema from `oracle-reference-stack/` (script: `sql/seed_test_schema.sql`):

```bash
(printf 'DEFINE db_user=%s\nDEFINE db_password=%s\n' "$DB_USER" "$DB_PASSWORD"; cat sql/seed_test_schema.sql) | podman exec -i oracle23ai-lite sqlplus -s system/$ORACLE_PWD@localhost/FREEPDB1
```

It creates tablespace `FORMS_DATA` (FREEPDB1 has no `USERS` tablespace), user `$DB_USER`, and tables `departments` (3 rows), `employees` (6), and `test_items` (5). It runs four queries. The first-run `ORA-01918` from `DROP USER` is expected.

Check the login and the port:

```bash
printf "SELECT USER FROM dual;\nSELECT COUNT(*) FROM test_items;\nEXIT\n" | podman exec -i oracle23ai-lite sqlplus -s $DB_USER/$DB_PASSWORD@localhost/FREEPDB1
```

From PowerShell, use `Test-NetConnection 127.0.0.1 -Port 1521`. `localhost` tries IPv6 `::1` first and can report a false failure.

`$DB_PASSWORD` is a documented local-development default. Change it for anything beyond a throwaway container.

The image is labelled **Oracle AI Database 26ai Free**, and `product_component_version` reports version `23.0.0.0.0`. The repo's docs were written for 23ai. The SQL and connections in this repo work with it.

---

## 8. WebLogic domain, Forms server, and sample forms

Status: **verified end-to-end on Windows.** The Employee Directory form (`ofw_demo_employees`) runs through FSAL from WSL with live rows from `$DB_USER`. The ordered recipe is this page (sections 6 to 8, 10 and 13) and `clean-clone-checklist.md`.

**One script applies every adjustment after the domain exists.** From the repo root, in WSL, with `.env` loaded:

```bash
set -a; source .env; set +a
bash oracle-reference-stack/apply_forms14c_config.sh [--skip-samples] [--fix-perms]
```

It writes `bin/setUserOverrides.sh` and the `instances/forms1` symlink, generates `boot.properties` from `WLS_ADMIN_USER` and `WLS_ADMIN_PASSWORD` in `.env`, adds the `FREEPDB1` entry to `tnsnames.ora`, copies the `fmrweb*.res` terminal resources, builds the 16 `frmweb` libraries and `libfrmjapi.so.0`, compiles the sample `.mmx` and `.fmx`, and prints a verification summary. It is idempotent. The templates it uses are in `oracle-reference-stack/forms14c-config/` (`setUserOverrides.sh`, `tnsnames-FREEPDB1.entry`). **Status: written from the steps verified by hand below; not yet run end to end on a clean Oracle Home.** The manual steps that follow stay as the reference for what the script does.

Load the demo schemas (run from `oracle-reference-stack/` or with the repo path):

```bash
podman exec -i oracle23ai-lite sqlplus -s $DB_USER/$DB_PASSWORD@localhost/FREEPDB1 < samples/ofw_demo_employees.sql
podman exec -i oracle23ai-lite sqlplus -s $DB_USER/$DB_PASSWORD@localhost/FREEPDB1 < samples/ofw_demo_sales.sql
```

The `ORA-00942` lines from `ofw_demo_sales.sql`'s `DROP TABLE` statements are expected on the first run.

Create the domain (throwaway container; the mounts match the Oracle Home and repo layout):

```bash
podman run --rm --platform linux/amd64 --network oracle-net --env-file ../.env \
  -v "$OH:/oracle_home_14c:Z" -v "$PWD/..:/work:Z" \
  forms-converter-14c:latest \
  /oracle_home_14c/oracle_common/common/bin/wlst.sh \
  /work/plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/scripts/create_forms_domain.py
```

Run this from `oracle-reference-stack/`. Output ends with `Domain Creation Complete`.

Before the server can start, the container-created Oracle Home subfolders need read access for the WSL user. They are owned by subordinate UID 100999 and show `Permission denied` from WSL:

```bash
podman unshare chmod -R a+rX "$OH"    # slow; several minutes
```

Write the two files into the domain (`bin/setUserOverrides.sh`, `servers/AdminServer/security/boot.properties`) with LF endings and no BOM. `apply_forms14c_config.sh` does this: the `setUserOverrides.sh` template is `oracle-reference-stack/forms14c-config/setUserOverrides.sh`, and `boot.properties` is generated from `.env`.

Start the server:

```bash
podman run -d --name forms-server-14c --platform linux/amd64 --network oracle-net \
  -p 7001:7001 -p 9001:9001 \
  -v "$OH:/oracle_home_14c:Z" -v "$PWD/..:/work:Z" \
  -e ORACLE_HOME=/oracle_home_14c -e JAVA_HOME=/usr/lib/jvm/jdk-17.0.12 \
  forms-converter-14c:latest \
  /oracle_home_14c/user_projects/domains/base_domain/bin/startWebLogic.sh
```

Wait for `Server state changed to RUNNING` in `podman logs`, then confirm `curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:7001/forms/frmservlet` returns `200`.

On this machine the server took about 18 minutes to reach RUNNING, with the Oracle Home on the `/mnt/c` mount. The thread dump showed one execute thread doing file-existence checks in `com.oracle.cie.gdr` for most of that time. Plan for several minutes, and use `jstack` in the container (`/usr/lib/jvm/jdk-17.0.12/bin/jstack <java-pid>`) if the log stops for longer.

Before the server can read them, `boot.properties` and the `security` folder need to be readable by the container. Files written from Windows are owned by the host user, and the container maps that user differently. Run `chmod -R a+rwX` on the `security` folder and `boot.properties`, then restart the container.

Before launching, set the Forms environment in the domain's `bin/setUserOverrides.sh`. The server reads it at startup, and the variables carry into the `frmweb` processes it starts. Use the template at `forms/templates/config/default.env` as the reference:

```sh
#!/bin/sh
export LD_LIBRARY_PATH=/oracle_home_14c/lib:/oracle_home_14c/oracle_common/lib:${LD_LIBRARY_PATH}
export FORMS_PATH=/work/samples:/oracle_home_14c/forms
export FORMS_INSTANCE=/oracle_home_14c/user_projects/domains/base_domain/config/fmwconfig/components/FORMS/forms1
export ORACLE_PATH=/oracle_home_14c/forms:/oracle_home_14c/forms/admin/resource/US
export TNS_ADMIN=/oracle_home_14c/user_projects/domains/base_domain/config/fmwconfig
EXTRA_JAVA_PROPERTIES="-Dforms.instance=forms1 ${EXTRA_JAVA_PROPERTIES}"
export EXTRA_JAVA_PROPERTIES
```

Copy the terminal resources the Mac setup needed. Run this inside the container, which owns the Oracle Home:

```bash
podman exec forms-server-14c sh -c "mkdir -p /oracle_home_14c/forms/admin/resource/US && cp /oracle_home_14c/forms/templates/resource/US/fmrweb*.res /oracle_home_14c/forms/admin/resource/US/"
```

Restart the container after changing these files.
Two steps are needed before a form launches. Run both after the server is RUNNING.

**1. Build the runtime shared libraries (`genshlib`).** `frmweb` needs 16 `lib*.so.0` files. The installer ships only the static `.a` archives, so `frmweb` exits at launch with `FRM-93552` and `ldd` shows the missing libraries. Check with:

```bash
podman exec forms-server-14c sh -c 'export LD_LIBRARY_PATH=/oracle_home_14c/lib:/oracle_home_14c/oracle_common/lib; ldd /oracle_home_14c/bin/frmweb | grep "not found"'
```

Build each one (the names are the ones `ldd` reports, without `lib` and `.so.0`):

```bash
podman exec -u oracle forms-server-14c sh -c 'cd /oracle_home_14c/lib && for n in iplsn ibfrmw iffw ifcw ijcw ipfw imffrmw iwfw iwcw itw icw sosdw ixw iicw iiiw iqw; do /oracle_home_14c/bin/genshlib $n 0; done'
```

**2. Compile the sample form in the container.** The shipped `.fmx` in `samples/` fails with `FRM-40010: Cannot read form`. Compile the `.fmb` in a job directory, since compiling into `/work` or into the Oracle Home `forms/` folder fails with `FRM-10044: Cannot create file`:

```bash
podman exec -u oracle forms-server-14c sh -c 'mkdir -p /tmp/job && cp /work/samples/ofw_demo_employees.fmb /tmp/job/ && cd /tmp/job && export ORACLE_HOME=/oracle_home_14c LD_LIBRARY_PATH=/oracle_home_14c/lib:/oracle_home_14c/oracle_common/lib ORACLE_TERM=vt220 TERM=vt220 FORMS_PATH=/tmp/job:/work/samples:/oracle_home_14c/forms; /oracle_home_14c/bin/frmcmp_batch module=ofw_demo_employees.fmb module_type=form logon=no compile_all=yes'
```

The compile prints `No compilation errors`. It writes `ofw_demo_employees.fmx` into `/tmp/job/`. Copy the `.fmx` into the Oracle Home `forms/` folder so the server finds it:

```bash
podman exec -u oracle forms-server-14c cp /tmp/job/ofw_demo_employees.fmx /oracle_home_14c/forms/
```

The FSAL launch below then uses `form=ofw_demo_employees.fmx`. The launcher blocks while the form window is open, so it doesn't return until you close the form or it hits your timeout.
**3. Database alias (`tnsnames.ora`).** The domain's `config/fmwconfig/tnsnames.ora` is created from a template with no entries. Without an entry, the form fails with `ORA-12154: Cannot find alias FREEPDB1`. Append an entry that points at the database container's network alias:

```
FREEPDB1 =
  (DESCRIPTION =
    (ADDRESS_LIST =
      (ADDRESS = (PROTOCOL = TCP)(HOST = oracle-db-23ai)(PORT = 1521))
    )
    (CONNECT_DATA = (SERVICE_NAME = FREEPDB1))
  )
```

`oracle-db-23ai` resolves inside the Podman network, so the server container can reach the database. Use `127.0.0.1` only for host-side clients.

**4. Menu module.** Forms opens the menu with `FRM-10221: Cannot read file OFW_EMP_MENU.mmx` if the `.mmx` is stale. Recompile from the job directory the same way as the form, with `module_type=menu`, then copy the `.mmx` into the Oracle Home `forms/` folder:

```bash
podman exec -u oracle forms-server-14c sh -c 'mkdir -p /tmp/job2 && cp /work/samples/ofw_emp_menu.mmb /tmp/job2/ && cd /tmp/job2 && export ORACLE_HOME=/oracle_home_14c LD_LIBRARY_PATH=/oracle_home_14c/lib:/oracle_home_14c/oracle_common/lib ORACLE_TERM=vt220 TERM=vt220 FORMS_PATH=/tmp/job2:/work/samples:/oracle_home_14c/forms; /oracle_home_14c/bin/frmcmp_batch module=ofw_emp_menu.mmb module_type=menu logon=no compile_all=yes && cp /tmp/job2/ofw_emp_menu.mmx /oracle_home_14c/forms/'
```

**5. Logon password.** The `$DB_USER` password is `$DB_PASSWORD`, from `sql/seed_test_schema.sql`. `ORA-01017: invalid credential` means the wrong password was entered. `$WLS_ADMIN_PASSWORD` is the WebLogic admin password.
Launch a sample form from WSL (`samples/ofw_demo_employees.fmx`, after the server is RUNNING):

```bash
java -jar $OH/forms/java/frmsal.jar \
  -url "http://localhost:7001/forms/frmservlet?config=standaloneapp&form=ofw_demo_employees.fmx&userid=$DB_USER/$DB_PASSWORD@FREEPDB1"
```

The password is `$DB_PASSWORD` (the seed script's value). The testing guide's `$DB_PASSWORD` is out of date for the seeded `$DB_USER`.

**Status: verified.** The form opens through FSAL from WSL. The `FRM-93131 ... basesaa.txt` error seen on the first run came from missing Forms environment settings, fixed by the domain `setUserOverrides.sh`, the `instances/forms1` symlink, and the `fmrweb` resources (checklist section 10, row 5).

The FSAL needs the Oracle Home's `frmsal.jar` to be readable. If `Unable to access jarfile` appears, run `podman unshare chmod -R a+rX "$OH"` from `oracle-reference-stack/`.

Verified on Windows (inside WSL): `run_form.py` (through the app's **Save & launch in Oracle**), `launch_fsal.py` (run with `python3` in the Ubuntu shell), `health_check.py`, `compile_form.py` (compiles `test_db.fmb` and `ofw_sales.fmb`), `ofw_sales.fmb` compile and open, the `frmbld` GUI through WSLg (`forms-builder-gui.md`), a cold stop and start with `start_testbed.py`, `convert_fmb.py --check`, and the Rust gate (`cargo check`, `cargo test`). `launch_fsal.py` and the app's FSAL step need Java 17 or 21; WSL's Java 21 works. Artifact conversions are verified with `converting-oracle-artifacts/scripts/test_conversions.py`; results are in that skill's `SKILL.md`.
- `health_check.py`, `compile_form.py`, `run_form.py`, and `convert_fmb.py --check`.
- Workbench: `cargo check` and `cargo test` from the repo root (zero warnings, AGENTS.md gate).

The Rust gate (`cargo check`, `cargo test`) passes inside WSL with no Visual Studio. The `link.exe` and `msvcrt.lib` problem only affects the native Windows build, which was the first approach and is abandoned; see section 12.

---

## 9. Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| `RunRoot is pointing to a path (/run/user/1000/containers) which is not writable` | `/run/user/<uid>` missing (cleared by `wsl --shutdown`) | Run the `mkdir`/`chown`/`chmod` command in §3 |
| `IPAM error: failed to open database /run/user/<uid>/containers/networks/ipam.db: no such file or directory` | `containers/networks` missing, or `XDG_RUNTIME_DIR` points at `/mnt/wslg/runtime-dir` | `mkdir -p /run/user/$(id -u)/containers/networks` and `export XDG_RUNTIME_DIR=/run/user/$(id -u)` (§3) |
| `the container name "oracle23ai-lite" is already in use` | A failed earlier `podman run` left a container in Created state | `podman rm -f oracle23ai-lite`, then rerun |
| `database configuration mismatch` after `--runroot` or `--tmpdir` | Podman database built with another run root | Revert to the default run root. Do not switch on an existing store |
| `rootlessport listen tcp4 0.0.0.0:1521: bind: address already in use` | An older local Oracle listener holds port 1521 | Stop or remove the old install; confirm with `Get-NetTCPConnection -LocalPort 1521` |
| `chmod: changing permissions ... Operation not permitted` | `/mnt/c` mounted without `metadata` | Add the `[automount]` options in §2, then `wsl --shutdown` |
| `touch: cannot touch ...: Permission denied` in the Oracle Home | Container UID 1000 is not the host owner | `chmod 777` the Oracle Home folder (§4) |
| `exec container process .../fmw_14.1.2.0.0_fr_linux64.bin: Permission denied` | `.bin` lacks the execute bit after unzip | `chmod +x` the staged file (§4) |
| `podman compose` fails: `docker-compose` not found | No compose provider installed | Use plain `podman run` commands (this guide does) |
| `LINK : fatal error LNK1104: cannot open file 'msvcrt.lib'` (cargo) | You are building natively on Windows, which was a red herring | Do not fix it. Build and run in WSL (section 11); no Visual Studio is needed. If you must stay native, see section 12 (`vcvars64.bat` did not work on the verified install) |

---

## 10. Checklist: every Windows adjustment a fresh install needs

These are the fixes found on the first Windows run (WSL2 + Podman, x86-64). Each one is a one-time step unless it says "after every reboot". None of them changes how macOS works.

| # | Problem you will see | Cause | Fix |
|---|---|---|---|
| 1 | `chmod: Operation not permitted` on the Oracle Home | `/mnt/c` mounted without `metadata` | `[automount] options = "metadata,umask=22,fmask=11"` in `/etc/wsl.conf`, then `wsl --shutdown` (section 2) |
| 2 | `RunRoot ... not writable`, `IPAM error` (after every reboot) | `/run/user/<uid>` is gone | `sudo mkdir -p /run/user/$(id -u)` plus `chown`, `chmod 700`, `mkdir containers/networks`, and `export XDG_RUNTIME_DIR=/run/user/$(id -u)` |
| 3 | `Unable to access jarfile .../frmsal.jar`, `Permission denied` in the domain folder | Files written by the container belong to a subordinate UID | `podman unshare chmod -R a+rX "$OH"` from `oracle-reference-stack/` (slow) |
| 4 | `BEA-090066 ... boot.properties (Permission denied)` | Files written from Windows are owned by the host user | `chmod -R a+rwX` on the domain `security` folder and `boot.properties`, then restart the container |
| 5 | `FRM-93131: Cannot find base HTML file basesaa.txt` | Forms environment not set for the server | Domain `setUserOverrides.sh` exports `FORMS_INSTANCE`, `ORACLE_PATH`, `TNS_ADMIN`, `FORMS_PATH` and `-Dforms.instance=forms1`; add the `instances/forms1 -> ../forms1` symlink (section 8) |
| 6 | `FRM-93552: runtime process terminated abnormally` | The 16 `frmweb` shared libraries were never built | `genshlib <name> 0` for each name `ldd frmweb` reports as missing (section 8, step 1) |
| 7 | `Error initialising the Forms Jdapi: no frmjapi in java.library.path` | `libfrmjapi.so` is a link to a file that was never built | Build it with Oracle's own makefile (row 8 and the commands below the table). `genshlib` is the wrong tool for this library |
| 8 | `libfrmjapi.so.0: undefined symbol: __intel_sse2_strncat` or `__gxx_personality_v0` (after a `genshlib frmjapi` build), or `ld: cannot find -laio / -lX11 / -lXm / -lXt / -lXext / -lXp / -lstdc++` | `genshlib` links `libfrmjapi` from its own objects only. The real link needs other Oracle archives, X11/Motif, and the C++ runtime. The image has those runtime libraries but not the unversioned names the linker looks for | Add the unversioned links, then run `make -B -f ins_forms.mk` from `forms/lib` (commands below the table). Verify with the XML2Forms test below |
| 9 | `ORA-12154: Cannot find alias FREEPDB1` | The domain `tnsnames.ora` has no entries | Add a `FREEPDB1` entry that points at `oracle-db-23ai:1521` (section 8) |
| 10 | `FRM-40010: Cannot read form`, `FRM-10221: Cannot read file ...mmx` | The `.fmx` and `.mmx` shipped in `samples/` were compiled elsewhere | Recompile in the container (section 8, steps 2 and 4) |
| 11 | `FRM-10044: Cannot create file` | `frmcmp_batch` cannot write into `/work` or the Oracle Home `forms/` folder | Compile from a copy in a job directory such as `/tmp/job`, then copy the result in |
| 12 | `Could not launch ...` with `/work/temporacle-preview...` in the diagnostics | `run_form.py` built container paths with Windows backslashes, and bash swallowed them | Fixed in `run_form.py` (container paths use `as_posix()`). Same result on macOS |
| 13 | **Native Windows build only.** Preview dialog: "Python 3", "Forms 14c container", and "Forms servlet" show as missing | The app ran `python3` (Windows has `python`) and the Windows `podman` (the containers are in WSL). Does not happen when the app runs in WSL | Fixed in `oracle_preview.rs` and `run_form.py`: on Windows they use `python` and call Podman through `wsl -d <distro>` (`OFW_WSL_DISTRO`, default `Ubuntu`). macOS behavior is unchanged |
| 14 | The servlet stays "missing" for a long time after `podman start` | WebLogic reads the Oracle Home from the Windows drive | Wait. Measured boot times: 15, 18, and 44 minutes. Nothing is logged between the second `STARTING` and `ADMIN`; the servlet answers right after `RUNNING` |
| 15 | **Native Windows build only; not needed in WSL.** `cargo build` fails: `LNK1104 msvcrt.lib`, or `Cannot open include file 'stdint.h'` | Visual Studio needs the C++ workload and Windows SDK, and the build needs `LIB` and `INCLUDE` set. Building the app in WSL avoids all of it | `DEPENDENCIES.md` section 3.1 and `rust-workbench-setup.md` |
| 16 | FSAL window fails: `NoClassDefFoundError: java/applet/AppletStub` (host Java 26) | Java 26 removed `java.applet`, which Oracle's `frmsal.jar` needs | Use Java 17 or 21. `run_form.py` runs FSAL with the Java inside WSL on Windows (window through WSLg). Or install a JDK 21 on Windows and put it first on `PATH` |

After a reboot or `wsl --shutdown`, only rows 2 and 14 repeat.

**Building `libfrmjapi.so.0` (rows 7 and 8).** Run inside the Forms container as `oracle` (`podman exec -u oracle forms-server-14c sh -c '...'`). The links only add names; they do not change the image:

```sh
cd /oracle_home_14c/lib
ln -s /usr/lib64/libaio.so.1     libaio.so
ln -s /usr/lib64/libX11.so.6     libX11.so
ln -s /usr/lib64/libXm.so.4      libXm.so
ln -s /usr/lib64/libXt.so.6      libXt.so
ln -s /usr/lib64/libXext.so.6    libXext.so
ln -s /usr/lib64/libXp.so.6      libXp.so
ln -s /usr/lib64/libstdc++.so.6  libstdc++.so
cd /oracle_home_14c/forms/lib
ORACLE_HOME=/oracle_home_14c LD_LIBRARY_PATH=/oracle_home_14c/lib:/oracle_home_14c/oracle_common/lib \
  make -B -f ins_forms.mk /oracle_home_14c/lib/libfrmjapi.so.0
```

Oracle's makefile (`forms/lib/ins_forms.mk` and `env_forms.mk`) holds the real link line, including `libd2f.a`, the Oracle archives, and `-lstdc++`. Check the result without saving anything in `samples/`:

```sh
mkdir -p /tmp/x2f-test
ORACLE_HOME=/oracle_home_14c FORMS_API_TK_BYPASS=TRUE LD_LIBRARY_PATH=/oracle_home_14c/lib \
/usr/lib/jvm/jdk-17.0.12/bin/java -Djava.library.path=/oracle_home_14c/lib \
  -classpath /oracle_home_14c/jlib/frmjdapi.jar:/oracle_home_14c/jlib/frmxmltools.jar:/oracle_home_14c/oracle_common/modules/oracle.xdk/xmlparserv2.jar \
  oracle.forms.util.xmltools.XML2Forms OVERWRITE=YES OUTPUTDIR=/tmp/x2f-test /work/samples/ofw_demo_employees_fmb.xml
```

It prints `Module saved as /tmp/x2f-test/ofw_demo_employees.fmb`.

---

## 11. Run the Rust app inside WSL (recommended)

Run the Rust web server in the same Ubuntu distro as the containers. It is faster than the native Windows build (no `wsl.exe` hops, no slow Windows-drive builds), it needs **no Visual Studio and no Windows SDK**, and it runs almost the same code path as macOS: `python3`, `podman`, and Java 21 are Linux tools inside WSL. The browser stays on Windows (`http://127.0.0.1:7070`).

Verified: the app builds and serves in WSL; every required readiness check passes once the Forms servlet is up; **Save & launch in Oracle** converts the XML, compiles it, and opens the form; `cargo check` and `cargo test` pass in WSL (120 passed, 0 failed).

One-time setup, in the Ubuntu shell:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
sudo apt-get install -y build-essential    # only if `gcc --version` fails
```

Run it from the repo root:

```bash
cd /mnt/c/Users/<you>/source/repos/oracle-forms-workbench
. "$HOME/.cargo/env"
export XDG_RUNTIME_DIR=/run/user/$(id -u)   # Podman's runtime directory (checklist row 2)
export CARGO_TARGET_DIR=$HOME/ofw-target     # build output on WSL's own disk
cargo run -p oracle-forms-server             # http://127.0.0.1:7070
```

Stop it with `Ctrl + C`. Do not run a Windows copy at the same time; both use port 7070. `python3 start_testbed.py` starts the containers and this app together. The Windows-specific code in `oracle_preview.rs` and `run_form.py` (`python`, `wsl.exe`, root `exec`, WSL Java) only applies to the native Windows build, and does nothing inside WSL except that the container `exec` runs as root, which the Windows drive needs there too.

`aws-lc-sys` (the TLS dependency) compiles C during the build, and WSL already has `gcc`, so the Visual Studio work (row 15) and the Java 26 problem (row 16) do not apply here. WSL's Java is 21.


---

## 12. The first approach, abandoned: native Windows build with Visual Studio

The app was first built and run as a native Windows program with Visual Studio. It worked after repairs, but it needs far more setup than running the app in WSL, so everything now runs in WSL (section 11). This section records what was tried so nobody repeats it. Use it only if you cannot run the app in WSL. The full detail is in `DEPENDENCIES.md` section 3.1 ; the exact `LIB` and `INCLUDE` `set` lines are at the end of this section.

**Why it needs Visual Studio.** The default Rust target on Windows is `x86_64-pc-windows-msvc`. Rust links with `link.exe` against the MSVC runtime and the Windows SDK, and the `aws-lc-sys` TLS crate compiles C with `cl.exe`, which needs the MSVC and SDK headers (`stdint.h`, `vcruntime.h`).

**What went wrong, in order:**

| Step | Error | Cause | Fix |
|---|---|---|---|
| `cargo check` | `LINK : fatal error LNK1104: cannot open file 'msvcrt.lib'` | `msvcrt.lib` is in `VC\Tools\MSVC\<ver>\lib\onecore\x64`, which the default linker path does not search; this install also had no `vcvarsall.bat`, so `vcvars64.bat` could not load the environment | Set `LIB` and `INCLUDE` to the onecore, ucrt, um, and shared folders |
| `cargo check` | `Cannot open include file: 'stdint.h'` (compiling `aws-lc-sys`) | The Visual Studio install lacked the C/C++ headers (`vcruntime.h`) and the SDK UCRT headers | Repair the install (below) |
| Repair | Installer exits with code 5007 | A passive or quiet installer run was not elevated from the start | Run PowerShell as Administrator |
| Repair | Installer stalls | One package (the x86 VC++ redistributable) waits on a hidden "Please insert the disk" prompt; a Windows restart can also be needed | Dismiss the prompt, restart if asked |
| Preview dialog | "Python 3", "Forms 14c container", "Forms servlet" shown as missing | The app ran `python3` and the Windows `podman`, but the containers are in WSL | `python` and `wsl -d <distro>` branches in `oracle_preview.rs` and `run_form.py` (`OFW_WSL_DISTRO`, default `Ubuntu`) |
| FSAL launch | `NoClassDefFoundError: java/applet/AppletStub` | Windows Java 26 removed `java.applet` | Java 17 or 21 |

**Repair command (run PowerShell as Administrator):**

```powershell
& "C:\Program Files (x86)\Microsoft Visual Studio\Installer\vs_installer.exe" modify `
  --installPath "C:\Program Files\Microsoft Visual Studio\18\Community" `
  --add Microsoft.VisualStudio.Workload.NativeDesktop `
  --add Microsoft.VisualStudio.Component.Windows10SDK.19041 `
  --includeRecommended --passive --norestart
```

After the repair, confirm `vcruntime.h` exists under `VC\Tools\MSVC\<ver>\include`, then run `cargo check` and `cargo test` from the repo root.

**Result:** with `rustc` 1.94.1, Windows SDK 10.0.19041.0, and MSVC 14.51.36231, `cargo check` and `cargo test` passed natively after the repair and the `LIB`/`INCLUDE` workaround. It was then replaced by the WSL route, which needs none of this (Rust 1.100.0 in Ubuntu, gcc 13.3, `cargo test` 120 passed), is faster, and uses almost the same code path as macOS. The Windows-only branches in `oracle_preview.rs` and `run_form.py` remain so the native route still works as an alternative.

### The native route's exact commands (abandoned; a record only)

Verified once on Windows 11 with Visual Studio 2026 Community and Windows 10 SDK 10.0.19041.0. The native build needs the include and library paths set. In a `cmd` window (adjust the Visual Studio folder to your install):

```bat
set "VCT=<Visual Studio folder>\VC\Tools\MSVC\<version>"
set "SDK=C:\Program Files (x86)\Windows Kits\10"
set "SDKV=10.0.19041.0"
set "LIB=%VCT%\lib\onecore\x64;%SDK%\Lib\%SDKV%\ucrt\x64;%SDK%\Lib\%SDKV%\um\x64"
set "INCLUDE=%VCT%\include;%SDK%\Include\%SDKV%\ucrt;%SDK%\Include\%SDKV%\um;%SDK%\Include\%SDKV%\shared"
cargo run -p oracle-forms-server
```

From PowerShell: stop with `Get-Process oracle-forms-server -ErrorAction SilentlyContinue | Stop-Process`; confirm the port is free with `Get-NetTCPConnection -LocalPort 7070 -State Listen -ErrorAction SilentlyContinue` (no output); start again from the repo root with `cargo run -p oracle-forms-server`, or run `.\target\debug\oracle-forms-server.exe` if it is built (started from another folder, the samples and UI files are not found). To let the UI query the database, load `.env` first: `Get-Content .env | ForEach-Object { $k,$v = $_ -split '=',2; if ($k -and -not $k.StartsWith('#')) { Set-Item "env:$k" $v } }`, then `$env:OFW_SERVER_ENV_CREDENTIALS = '1'`. Do not run the native copy and the WSL copy together; both use port 7070.

---

## 13. How the Rust app launches Fusion Middleware, and what must be true first

The Rust app does not start or manage the Forms (Fusion Middleware) server. `python3 start_testbed.py` or `podman start` starts the containers; the app only uses them. This section is the one checklist for "I clicked Preview in Oracle and nothing opened". It is written from `architecture.md` (the Preview sequence) and `clean-clone-checklist.md` (item 5, "Windows (Podman inside WSL2): verified end-to-end"); the readiness list is what the Preview dialog reported on the verified machine.

**What happens when you use Preview in Oracle (inside WSL):**
1. The browser (Windows, `http://127.0.0.1:7070`) sends the form's XML to the Rust server (`/api/forms/oracle-preview`).
2. The server stages the XML in an ignored job folder and runs `python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/run_form.py` with it.
3. `run_form.py` converts the XML to an `.fmb` (Oracle's XML2Forms, run in the Forms container), compiles it to an `.fmx` (`frmcmp_batch`), and starts the FSAL launcher (`launch_fsal.py`) with the WSL Java 21.
4. FSAL opens the form through the Forms servlet at `http://127.0.0.1:7001/forms/frmservlet`; the window appears on the Windows desktop through WSLg.

**What must be true before it works** (check in this order; `GET http://127.0.0.1:7070/api/forms/oracle-preview/readiness` shows the same checks):
1. The app runs inside WSL (section 11), with `python3`, `podman`, and Java 21 on the `PATH` there.
2. `/run/user/<uid>` exists and `XDG_RUNTIME_DIR` points at it (checklist row 2). After `wsl --shutdown` it must be recreated.
3. The containers are running: `oracle23ai-lite` and `forms-server-14c`. `podman ps` shows both; the database must say "DATABASE IS READY TO USE".
4. The Forms server reached `Server state changed to RUNNING` (15 to 44 minutes after start on the Windows drive), and `curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:7001/forms/frmservlet` returns `200`.
5. The domain adjustments are in place (`apply_forms14c_config.sh`, or section 8 and checklist rows 3 to 11): `setUserOverrides.sh`, `boot.properties`, `instances/forms1`, the `FREEPDB1` alias, the `frmweb` libraries, and `libfrmjapi.so.0`.
6. `.env` exists with `DB_USER`, `DB_PASSWORD`, and `DB_SERVICE`; the seed script ran, so the tables the form uses exist (`ORA-00942` or `must be declared` errors mean a missing table or column).
7. WSLg works (a window opens from WSL); Java is 17 or 21, never Windows Java 26.

**To test the launch without rewriting any form XML, do not click Save & launch.** Use the manual steps in `HOW-TO-RUN.md` section 7, or run `python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/launch_fsal.py <form>.fmx --interactive`.

**Status:** verified on Windows inside WSL: Preview in Oracle converting, compiling, and opening `ofw_demo_employees` with live rows, and `launch_fsal.py`. The numbered list above is assembled from the existing docs and the verified run; I did not re-run every check in this session.

---

## 14. Run your AI coding agent inside WSL too

Everything else in this setup runs in the Ubuntu WSL2 distro, so run the coding agent (Claude Code, or another agent CLI) from the Ubuntu shell as well. Started from Windows, every command it runs goes through PowerShell and then `wsl -d Ubuntu -- bash`, and the quoting, path conversion, and password prompts broke repeatedly (change log, section 4). Started in WSL, `podman`, `python3`, `cargo`, `java`, and `start_testbed.py` run directly.

**Setup (in the Ubuntu shell, any folder; the installer puts the program in your home directory):**

```bash
curl -fsSL https://claude.ai/install.sh | bash     # installs Claude Code; open a new terminal if `claude` is not found
gh auth login                                      # so the agent can create pull requests from WSL
```

**Run it from the repo root** (the folder you start it in is its working directory):

```bash
cd /mnt/c/Users/<you>/source/repos/oracle-forms-workbench
claude
```

**Notes:**
- **sudo steps:** the agent cannot type a password. Run sudo commands yourself with the `!` prefix in the agent's prompt, or in a second Ubuntu terminal. After a reboot or `wsl --shutdown`, `python3 start_testbed.py` recreates `/run/user/<uid>` and asks for the sudo password.
- **Memory and history are per environment.** An agent started in WSL has its own memory and session history, separate from one started in Windows. The rules that must carry over are in the repo: `CLAUDE.md`, `AGENTS.md`, `.agent/rules/`, `architecture.md`.
- **The repo stays on the Windows drive** (`/mnt/c/...`). File reads and git are slower there, but moving it would break the paths in the docs.
- **A Python virtual environment is fine.** `start_testbed.py` and the skill scripts use only the standard library, so an active `.venv` does not matter.
- **Status:** the installer command was run by the repo owner. Whether the agent works smoothly from WSL on this machine has **not been verified** by the agent itself.
