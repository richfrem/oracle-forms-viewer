# Installing Oracle Forms 14c (Fusion Middleware 14.1.2) in the containers

The platform-neutral procedure: the same on macOS and on Windows (inside Ubuntu WSL2). Only the Podman setup differs: [podman-setup-guide.md](podman-setup-guide.md) (macOS) or [windows-podman-setup.md](windows-podman-setup.md) (Windows). Run every command from the repo root. Credentials come from the git-ignored `.env`; never type them into commands or docs.

Status: the procedure was first run on macOS (Apple Silicon, Rosetta 2) and then on Windows 11 in WSL2. The configuration script in step 7 was written from the manual steps and has **not been run on a clean Oracle Home**. Windows-specific fixes are in `windows-podman-setup.md` section 10.

> **`$OH` below is the Oracle Home folder:** `export OH="${OFW_ORACLE_HOME:-$PWD/oracle-reference-stack/oracle_home_14c}"` (run it from the repo root, after loading `.env`). **On Windows, set `OFW_ORACLE_HOME` in `.env` to a folder on Ubuntu's own disk, for example `/home/<you>/oracle_home_14c`, BEFORE you install. Never install into the repo folder (`oracle-reference-stack/oracle_home_14c`): it is on the slow Windows drive and WebLogic takes 15 to 44 minutes to boot.** Why and how: [oracle-home-location.md](oracle-home-location.md).

## Contents

- [What you end up with](#what-you-end-up-with)
- [1. Prerequisites](#1-prerequisites)
- [2. Download the installers](#2-download-the-installers)
- [3. Response files](#3-response-files)
- [4. Build the image](#4-build-the-image)
- [5. Install (two stages, one-shot containers)](#5-install-two-stages-one-shot-containers)
- [6. Check the install](#6-check-the-install)
- [7. Create the domain and apply the configuration](#7-create-the-domain-and-apply-the-configuration)
- [8. Start the server and check it](#8-start-the-server-and-check-it)

## What you end up with

| Piece | What it is |
|---|---|
| Image `forms-converter-14c:latest` | Oracle Linux 8, Oracle JDK 17, X11/Motif and system libraries, build tools. **No Oracle software inside it** |
| Oracle Home folder (`$OH`; on Windows on Ubuntu's disk; default `oracle-reference-stack/oracle_home_14c/` on a Mac; git-ignored, about 5.4 GB) | The installed Forms and Reports 14c, the WebLogic domain, and all configuration. Mounted into containers as `/oracle_home_14c`. Where it lives on disk is a setting: [oracle-home-location.md](oracle-home-location.md) |
| Container `forms-server-14c` | WebLogic with the Forms servlet, ports 7001 and 9001 |
| Short-lived containers (`--rm`) | The installers, conversions, compiles, domain creation: each starts, does one job, disappears. A container does not need to run all the time |

Because the software lives in a host folder and not in a container layer, rebuilding images or recreating containers never loses it.

## Where the Oracle Home goes (decide before you install)

The installers write into one host folder, mounted as `/oracle_home_14c`. Choose it **first**; moving it later means a slow copy.

| Host | Put it | Why |
|---|---|---|
| **Windows (Ubuntu WSL2)** | **On Ubuntu's own disk:** set `OFW_ORACLE_HOME=/home/<you>/oracle_home_14c` in `.env`, then `export OH="$OFW_ORACLE_HOME"`. **Never in the repo folder** (`/mnt/c/...`) | The Windows drive is slow for the 46,000 small files WebLogic opens: boots took 15 to 44 minutes there |
| macOS / Linux | The default is fine: leave `OFW_ORACLE_HOME` empty (folder `oracle-reference-stack/oracle_home_14c`) | A Mac boots in about 2 minutes with it |

The installers (`installer/`, 3.6 GB, read once) can stay in the repo folder on the Windows drive. Create the Oracle Home folder yourself and make it writable by the container's user before step 5: `mkdir -p "$OH" && chmod 777 "$OH"`.

If you already installed into the repo folder, move it with [oracle-home-location.md](oracle-home-location.md) (copy inside WSL, never with Windows Explorer).

## 1. Prerequisites

1. Podman working (macOS: Podman machine with Rosetta 2; Windows: Podman inside Ubuntu WSL2). The images are `linux/amd64`; pass `--platform linux/amd64` to every `podman run` and `podman build`.
2. `.env`: `python3 plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/scripts/create_env.py`, then `set -a; source .env; set +a` in each shell. It holds `ORACLE_PWD`, `DB_USER`, `DB_PASSWORD`, `DB_SERVICE`, `WLS_ADMIN_USER`, `WLS_ADMIN_PASSWORD`.
3. The network: `podman network create oracle-net` (skip if it exists).
4. The database container: [database-23ai.md](database-23ai.md).

## 2. Download the installers

From Oracle Software Delivery Cloud ([edelivery.oracle.com](https://edelivery.oracle.com/), Oracle sign-in required). Choose platform **Linux x86-64** on every host, including Apple Silicon and Windows: the container is Oracle Linux. Never use the Windows `.exe` installer for this setup.

| File | Size | Contains |
|---|---|---|
| `V1045135-01.zip` | 2.25 GB | `fmw_14.1.2.0.0_infrastructure.jar`: FMW Infrastructure 14.1.2.0.0 (WebLogic) |
| `V1045121-01.zip` | 1.42 GB | `fmw_14.1.2.0.0_fr_linux64.bin`: Forms and Reports 14.1.2.0.0 |
| `jdk-17.0.12_linux-x64_bin.tar.gz` | 183 MB | Oracle JDK 17, from Oracle's JDK archive. **It must be Oracle's JDK, not OpenJDK:** the installer checks the Java vendor and stops with "The OpenJDK JVM is not supported on this platform" |

eDelivery offers an Oracle Download Manager or a direct browser download of each zip. The exact click-by-click steps for the 14c files were not recorded (the same two download methods applied to the 12c files); if you have them, add them here.

Stage them in `oracle-reference-stack/installer/` (git-ignored; never commit installers):

```bash
mv ~/Downloads/V1045135-01.zip ~/Downloads/V1045121-01.zip ~/Downloads/jdk-17.0.12_linux-x64_bin.tar.gz oracle-reference-stack/installer/
cd oracle-reference-stack/installer && unzip -o V1045135-01.zip && unzip -o V1045121-01.zip && cd ../..
chmod +x oracle-reference-stack/installer/fmw_14.1.2.0.0_fr_linux64.bin     # the unzip does not keep the execute bit on WSL
```

## 3. Response files

Copy the three committed templates from `oracle-reference-stack/installer-templates/` into `installer/`: `oraInst.loc`, `infra14c_silent.rsp`, `forms14c_silent.rsp`. They contain:

- `oraInst.loc`: `inventory_loc=/home/oracle/oraInventory` and `inst_group=oinstall`. **The inventory must never be inside the Oracle Home.** If it is, the installer creates it first and then rejects the Oracle Home with `INST-07319: ... already exists and is a nonempty directory`.
- `infra14c_silent.rsp`: `ORACLE_HOME=/oracle_home_14c`, `INSTALL_TYPE=Fusion Middleware Infrastructure`.
- `forms14c_silent.rsp`: `ORACLE_HOME=/oracle_home_14c`, `INSTALL_TYPE=Forms and Reports Deployment`.

Keep these files LF-only; do not write them with PowerShell here-strings.

## 4. Build the image

```bash
cd oracle-reference-stack
podman build --platform linux/amd64 -t forms-converter-14c:latest -f Containerfile.14c .
cd ..
```

Why each part of `Containerfile.14c` is there:

| Part | Why |
|---|---|
| Base `oraclelinux:8-slim` | Matches Oracle's certified runtime |
| Oracle JDK 17.0.12 (unpacked to `/usr/lib/jvm`, `JAVA_HOME=/usr/lib/jvm/jdk-17.0.12`) | WebLogic 14.1.2 and Forms 14c need Java 17, and the installer rejects OpenJDK |
| `libX11 libXext libXrender libXtst libXt libXp motif` | Oracle's binaries link against X11 and Motif even in batch mode; without them: `libX11.so.6: cannot open shared object file` or `UnsatisfiedLinkError` |
| `libnsl` and `libnsl2` | `libnsl.so.1` for the Forms JNI bridge; `libnsl.so.2` for the Reports converter `bin/rwconverter` |
| `libaio` | `libfrmjapi.so.0` links `libaio.so.1` |
| `gcc make binutils glibc-devel` | `genshlib` and Oracle's makefile build the Forms runtime shared libraries (step 7) |
| `shadow-utils` | Creates the non-root user `oracle` (UID 1000) and group `oinstall`. **The installer refuses to run as root** ("The Oracle Universal Installer cannot continue") |
| `tar gzip procps-ng hostname which` | Unpacking, and process and host lookups used by the WebLogic scripts |
| `/etc/ld.so.conf.d/oracle_forms.conf` listing `/oracle_home_14c/lib` and `/oracle_home_14c/oracle_common/lib` | So Oracle's shared libraries are found at run time |

## 5. Install (two stages, one-shot containers)

```bash
# Stage 1: FMW Infrastructure (about 25 minutes)
podman run --rm --platform linux/amd64 \
  -v "$PWD/oracle-reference-stack/installer:/installer:Z" -v "$OH:/oracle_home_14c:Z" \
  forms-converter-14c:latest \
  java -jar /installer/fmw_14.1.2.0.0_infrastructure.jar -silent -ignoreSysPrereqs \
    -responseFile /installer/infra14c_silent.rsp -invPtrLoc /installer/oraInst.loc

# Stage 2: Forms and Reports
podman run --rm --platform linux/amd64 \
  -v "$PWD/oracle-reference-stack/installer:/installer:Z" -v "$OH:/oracle_home_14c:Z" \
  forms-converter-14c:latest \
  /installer/fmw_14.1.2.0.0_fr_linux64.bin -silent -ignoreSysPrereqs \
    -responseFile /installer/forms14c_silent.rsp -invPtrLoc /installer/oraInst.loc
```

Success shows `Percent Complete : 100` and "installed successfully". On Windows the Stage 2 process kept running after it reported success and ended with exit code 139; read the installer log, not only the exit code, and check the files in step 6. Run the commands on one line, or with no spaces after each backslash (`Error: parsing reference " "` means a trailing space after a `\`).

## 6. Check the install

Inside a throwaway container (the folders may be owned by container-mapped user IDs and unreadable from the host):

```bash
podman run --rm --platform linux/amd64 -v "$OH:/oracle_home_14c:Z" \
  forms-converter-14c:latest ls -l /oracle_home_14c/forms/java/frmsal.jar /oracle_home_14c/bin/frmcmp_batch /oracle_home_14c/bin/frmweb /oracle_home_14c/bin/rwconverter
```

The Oracle Home then holds `wlserver/`, `oracle_common/`, `bin/frmbld`, `frmcmp_batch`, `frmweb`, `rwconverter`, `forms/java/frmsal.jar`, and `jlib/frmxmltools.jar` (the Forms2XML and XML2Forms classes). **In 14c the `bin/*.sh` wrappers (for example `frmf2xml.sh`, `frmbld.sh`, `rwconverter.sh`) are unfilled templates with `%ORACLE_HOME%`-style placeholders,** so call the binaries and Java classes directly, as the skill scripts do.

## 7. Create the domain and apply the configuration

Installing places the software on disk; WebLogic still needs a domain to listen on ports 7001 and 9001.

```bash
podman run --rm --platform linux/amd64 --network oracle-net --env-file .env \
  -v "$OH:/oracle_home_14c:Z" -v "$PWD:/work:Z" \
  forms-converter-14c:latest \
  /oracle_home_14c/oracle_common/common/bin/wlst.sh /work/plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/scripts/create_forms_domain.py
```

It builds `base_domain` from `wls.jar` plus Oracle's `forms_development_template.jar`: a development domain with no RCU database schemas needed, with `formsapp`, `em.ear`, and `fads` deployed on the AdminServer (port 7001). It reads `WLS_ADMIN_PASSWORD` from the environment. **Keep the domain name `base_domain`**: renaming it breaks the embedded LDAP path and the server stops with `Authentication denied: Boot identity not valid`. In offline WLST do not call `setOption('DomainName', ...)`; the name comes from the output path.

Then apply every post-install adjustment with one script (`set -a; source .env; set +a` first):

```bash
bash oracle-reference-stack/apply_forms14c_config.sh
```

It: writes the domain's `bin/setUserOverrides.sh` (puts `oracle_common/lib` on `LD_LIBRARY_PATH`, so `libopmnperf.so` is found, otherwise `UnsatisfiedLinkError ... BEA-149231`; and adds `-Dforms.instance=forms1`, without which `FRM-93500 ... NullPointerException` appears under Java 17); generates `boot.properties` from `WLS_ADMIN_USER` and `WLS_ADMIN_PASSWORD` (WebLogic encrypts it on first boot); creates the `instances/forms1` symlink; adds the `FREEPDB1` alias to `tnsnames.ora` pointing at `oracle-db-23ai:1521`; copies the `fmrweb*.res` terminal resources into `forms/admin/resource/US` (otherwise `Error opening Oracle*Terminal file fmrweb`); builds the 16 `frmweb` shared libraries with `genshlib` (otherwise `libiplsn.so.0: cannot open shared object file`) and `libfrmjapi.so.0` with Oracle's makefile (not `genshlib`); creates the Reports preference and printer files `rwconverter` needs; compiles the sample menu and form; and prints a verification summary. The Oracle Home location is a setting (`OFW_ORACLE_HOME`).

## 8. Start the server and check it

```bash
podman run -d --name forms-server-14c --platform linux/amd64 --network oracle-net \
  -p 7001:7001 -p 9001:9001 \
  -v "$OH:/oracle_home_14c:Z" -v "$PWD:/work:Z" \
  -e ORACLE_HOME=/oracle_home_14c -e JAVA_HOME=/usr/lib/jvm/jdk-17.0.12 \
  -e DOMAIN_HOME=/oracle_home_14c/user_projects/domains/base_domain \
  forms-converter-14c:latest \
  /oracle_home_14c/user_projects/domains/base_domain/bin/startWebLogic.sh
```

Do this once. Afterwards use `python3 start_testbed.py` (it starts the existing containers and confirms each one). Check:

1. `podman logs forms-server-14c` ends with `Server state changed to RUNNING`. Boot time: about 2 minutes on macOS under Rosetta; **15 to 44 minutes on Windows when the Oracle Home is on the Windows drive** (see [oracle-home-location.md](oracle-home-location.md)).
2. `curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:7001/forms/frmservlet` returns `200`.
3. Use **port 7001**. In this development domain the Forms application is on the AdminServer; port 9001 is reserved for a separate `WLS_FORMS` managed-server cluster. FSAL on 9001 fails with `Unexpected end of file from server`.
4. WebLogic 14.1.2 no longer has the in-browser `/console`. For administration use the desktop WebLogic Remote Console (Oracle's `weblogic-remote-console` releases on GitHub; macOS and Windows builds), connect to `http://localhost:7001` with `WLS_ADMIN_USER` and `WLS_ADMIN_PASSWORD` from `.env`. The Forms listener status is at `http://localhost:7001/forms/frmservlet/admin`. The Remote Console was not re-checked on Windows.

### Alternative: the compose route (the original macOS method; not verified in the Windows sessions)

`oracle-reference-stack/compose.yaml` defines the install and server steps as services, so with Podman Compose installed the same steps can be run as:

```bash
podman compose -f oracle-reference-stack/compose.yaml run --rm infra-install-14c      # stage 1
podman compose -f oracle-reference-stack/compose.yaml run --rm forms-install-14c      # stage 2
podman compose -f oracle-reference-stack/compose.yaml up -d forms-server-14c          # start the server
```

The Windows setup used the plain `podman run` commands above because Podman Compose is not installed there and the commands are identical on both platforms. Known problems in `compose.yaml` are listed in [oracle-reference-stack-map.md](oracle-reference-stack-map.md); fix them before relying on it. The compose route still needs the image, the response files, and the domain steps (7) done as above.

Then run a form: [forms-and-reports-testing-guide.md](forms-and-reports-testing-guide.md), or `python3 plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/scripts/health_check.py`. Errors: [troubleshooting.md](troubleshooting.md).
