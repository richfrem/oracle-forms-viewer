# Windows Podman Setup (Podman inside WSL2)

**Host:** Windows 11 x64 with an Ubuntu 24.04 WSL2 distribution
**Container architecture:** `linux/amd64`, native on an x86-64 host (no emulation)
**Companion references:** [podman-setup-guide.md](podman-setup-guide.md) (macOS, Apple Silicon with Rosetta 2) · [docs/WINDOWS-PODMAN-SETUP.md](../../../../../../docs/WINDOWS-PODMAN-SETUP.md) (full status log, troubleshooting table, and per-step evidence)

Status: **verified through the Forms & Reports install.** The database, WebLogic domain, FSAL, and workbench steps are not yet verified on Windows. Each section below says which.

---

## 1. Rules for this platform

- Run **all container commands from the Ubuntu shell** (`wsl -d Ubuntu`), not PowerShell. Podman runs inside WSL. Podman for Windows is not used.
- Use the **Linux x86-64** Oracle installers (`.zip` and `.bin`), never the Windows `.exe`. The container is Oracle Linux.
- Keep shell scripts and `.rsp`/`.loc` files **LF-only**. Do not write them with PowerShell here-strings.
- Use `curl.exe` in PowerShell, not the `curl` alias (it runs `Invoke-WebRequest`).
- Run repo paths through WSL: `/mnt/c/Users/<you>/source/repos/oracle-forms-workbench`. Never put absolute machine paths into committed docs; use `$PWD` or the repo-relative path.

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

```bash
sudo apt-get update
sudo apt-get install -y podman
podman --version
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

**Oracle Home:** `oracle-reference-stack/oracle_home_14c/` on the Windows drive. The rootless container user (UID 1000) maps to a different host UID, so the folder needs `chmod 777` from Ubuntu before the first install:

```bash
chmod 777 oracle-reference-stack/oracle_home_14c
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
  -v $PWD/oracle_home_14c:/oracle_home_14c:Z \
  forms-converter-14c:latest \
  java -jar /installer/fmw_14.1.2.0.0_infrastructure.jar \
    -silent -ignoreSysPrereqs \
    -responseFile /installer/infra14c_silent.rsp \
    -invPtrLoc /installer/oraInst.loc

# Stage 2: Forms and Reports 14.1.2.0.0 (after chmod +x on the .bin)
podman run --rm --platform linux/amd64 \
  -v $PWD/installer:/installer:Z \
  -v $PWD/oracle_home_14c:/oracle_home_14c:Z \
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
  -v $PWD/oracle_home_14c:/oracle_home_14c:Z \
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
  -p 1521:1521 -e ORACLE_PWD=Welcome1 \
  container-registry.oracle.com/database/free:latest-lite
podman logs -f oracle23ai-lite            # wait for "DATABASE IS READY TO USE!" (about 6 minutes on first start)
```

Seed the schema from `oracle-reference-stack/` (script: `sql/seed_forms_user.sql`):

```bash
podman exec -i oracle23ai-lite sqlplus -s system/Welcome1@localhost/FREEPDB1 < sql/seed_forms_user.sql
```

It creates tablespace `FORMS_DATA` (FREEPDB1 has no `USERS` tablespace), user `forms_user`, and tables `departments` (3 rows), `employees` (6), and `test_items` (5). It runs four queries. The first-run `ORA-01918` from `DROP USER` is expected.

Check the login and the port:

```bash
printf "SELECT USER FROM dual;\nSELECT COUNT(*) FROM test_items;\nEXIT\n" | podman exec -i oracle23ai-lite sqlplus -s forms_user/Welcome1@localhost/FREEPDB1
```

From PowerShell, use `Test-NetConnection 127.0.0.1 -Port 1521`. `localhost` tries IPv6 `::1` first and can report a false failure.

`Welcome1` is a documented local-development default. Change it for anything beyond a throwaway container.

The image is labelled **Oracle AI Database 26ai Free**, and `product_component_version` reports version `23.0.0.0.0`. The repo's docs were written for 23ai. The SQL and connections in this repo work with it.

---

## 8. WebLogic domain, Forms server, and sample forms

Status: **verified end-to-end on Windows.** The Employee Directory form (`ofw_demo_employees`) runs through FSAL from WSL with live rows from `forms_user`. The ordered recipe is in `HOW-TO-RUN.md` section 7.5. Steps marked "by hand" are not yet committed as templates.

Load the demo schemas (run from `oracle-reference-stack/` or with the repo path):

```bash
podman exec -i oracle23ai-lite sqlplus -s forms_user/Welcome1@localhost/FREEPDB1 < samples/ofw_demo_employees.sql
podman exec -i oracle23ai-lite sqlplus -s forms_user/Welcome1@localhost/FREEPDB1 < samples/ofw_demo_sales.sql
```

The `ORA-00942` lines from `ofw_demo_sales.sql`'s `DROP TABLE` statements are expected on the first run.

Create the domain (throwaway container; the mounts match the Oracle Home and repo layout):

```bash
podman run --rm --platform linux/amd64 --network oracle-net \
  -v "$PWD/oracle_home_14c:/oracle_home_14c:Z" -v "$PWD/..:/work:Z" \
  forms-converter-14c:latest \
  /oracle_home_14c/oracle_common/common/bin/wlst.sh \
  /work/plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/scripts/create_forms_domain.py
```

Run this from `oracle-reference-stack/`. Output ends with `Domain Creation Complete`.

Before the server can start, the container-created Oracle Home subfolders need read access for the WSL user. They are owned by subordinate UID 100999 and show `Permission denied` from WSL:

```bash
podman unshare chmod -R a+rX oracle_home_14c    # slow; several minutes
```

Write the two files into the domain (`bin/setUserOverrides.sh`, `servers/AdminServer/security/boot.properties`) with LF endings and no BOM. The plan is to commit these as templates; until then, create them by hand.

Start the server:

```bash
podman run -d --name forms-server-14c --platform linux/amd64 --network oracle-net \
  -p 7001:7001 -p 9001:9001 \
  -v "$PWD/oracle_home_14c:/oracle_home_14c:Z" -v "$PWD/..:/work:Z" \
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

**5. Logon password.** The `forms_user` password is `Welcome1`, from `sql/seed_forms_user.sql`. `ORA-01017: invalid credential` means the wrong password was entered. `Welcome123#` is the WebLogic admin password.
Launch a sample form from WSL (`samples/ofw_demo_employees.fmx`, after the server is RUNNING):

```bash
java -jar oracle-reference-stack/oracle_home_14c/forms/java/frmsal.jar \
  -url "http://localhost:7001/forms/frmservlet?config=standaloneapp&form=ofw_demo_employees.fmx&userid=forms_user/Welcome1@FREEPDB1"
```

The password is `Welcome1` (the seed script's value). The testing guide's `Welcome123#` is out of date for the seeded `forms_user`.

**Status: blocked.** On this machine FSAL connects to the server but stops with `FRM-93131: Cannot find base HTML file basesaa.txt`. The file exists at `config/fmwconfig/components/FORMS/forms1/server/basesaa.txt`. The domain has no `formsweb.cfg`, and no `standaloneapp` section was found. A hand-written `[standaloneapp]` section did not clear the error. The next step is to find where the Forms web-app configuration for `standaloneapp` is created in an Oracle Forms 14c domain, and to check the FSAL client's own base-HTML lookup.

The FSAL needs the Oracle Home's `frmsal.jar` to be readable. If `Unable to access jarfile` appears, run `podman unshare chmod -R a+rX oracle_home_14c` from `oracle-reference-stack/`.

Not yet verified on Windows: `health_check.py`, `compile_form.py`, `run_form.py`, `launch_fsal.py`, `convert_fmb.py --check`, and the Rust gate. The Mac-written scripts assume the Mac layout; use the steps above until they are checked. `ofw_sales.fmb` has not been run.
- FSAL launch from the Windows host with `frmsal.jar` read from the Oracle Home. Whether the Windows-side Java can read the files written as the mapped UID is **unverified**.
- `health_check.py`, `compile_form.py`, `run_form.py`, and `convert_fmb.py --check`.
- Workbench: `cargo check` and `cargo test` from the repo root (zero warnings, AGENTS.md gate).

The Rust gate has a Windows toolchain issue: `link.exe` cannot find `msvcrt.lib` unless the Visual Studio x64 build environment is loaded. Load it from a Developer PowerShell or `vcvars64.bat` before running cargo. Status: being resolved.

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
| `LINK : fatal error LNK1104: cannot open file 'msvcrt.lib'` (cargo) | MSVC x64 build environment not loaded | Run cargo from a Developer PowerShell or after `vcvars64.bat` |
