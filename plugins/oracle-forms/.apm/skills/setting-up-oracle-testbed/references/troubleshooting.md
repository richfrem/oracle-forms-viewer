# Troubleshooting: Forms 14c, WebLogic, the containers, and the Rust app

Symptom, cause, fix. Windows-only problems (WSL, `/mnt/c`, the Podman runtime directory, Java 26) have their own 16-row table in [windows-podman-setup.md](windows-podman-setup.md) section 10; the full per-error log is in [windows-wsl-change-log.md](windows-wsl-change-log.md). Start with `python3 start_testbed.py --status` to see what is up.

## Installing

| Symptom | Cause | Fix |
|---|---|---|
| `The current user is root or has superuser privilege. The Oracle Universal Installer cannot continue.` | The installer refuses to run as root | The image runs as the non-root `oracle` user (UID 1000, group `oinstall`); use `forms-converter-14c:latest` as built from `Containerfile.14c` |
| `INST-07319: Validation of Oracle Home location failed. The location specified already exists and is a nonempty directory` | `oraInst.loc` put the central inventory inside the Oracle Home | `inventory_loc=/home/oracle/oraInventory` (outside the Oracle Home) |
| `The OpenJDK JVM is not supported on this platform.` | The installer checks `java.vendor == "Oracle Corporation"` | Use Oracle JDK 17 (`jdk-17.0.12_linux-x64_bin.tar.gz`), as `Containerfile.14c` does |
| `Error: parsing reference " ": invalid reference format` | A space after a line-continuation backslash | Put the command on one line, or remove the trailing spaces |
| `WARNING: image platform (linux/amd64) does not match the expected platform (linux/arm64)` (Apple Silicon) | No explicit platform | Add `--platform linux/amd64` to every `podman run` and `podman build` |
| `exec container process .../fmw_14.1.2.0.0_fr_linux64.bin: Permission denied` | The `.bin` lost its execute bit | `chmod +x` the staged `.bin` |
| `Config option "DomainName" is not valid for updating a domain` (WLST) | `setOption('DomainName', ...)` in offline WLST | Omit it; the domain name comes from the output path of `writeDomain` |

## Starting the Forms server

| Symptom | Cause | Fix |
|---|---|---|
| `Authentication denied: Boot identity not valid` | The domain was renamed away from `base_domain`, which breaks the embedded LDAP path | Keep the name `base_domain`; `boot.properties` goes in `servers/AdminServer/security/` |
| `java.lang.UnsatisfiedLinkError: ... libifopmnwrp.so.0: libopmnperf.so: cannot open shared object file` (BEA-149231) | WebLogic's `setDomainEnv.sh` does not put `oracle_common/lib` on `LD_LIBRARY_PATH` | `bin/setUserOverrides.sh` exporting `LD_LIBRARY_PATH=/oracle_home_14c/lib:/oracle_home_14c/oracle_common/lib:${LD_LIBRARY_PATH}`; `apply_forms14c_config.sh` writes it |
| `FRM-93500: unexpected error while attempting to create the runtime process` with `NullPointerException ... "value" is null` | The development domain runs `formsapp` on the AdminServer; with no `forms.instance` property the instance name is null, and Java 17 rejects the null environment value | `EXTRA_JAVA_PROPERTIES="-Dforms.instance=forms1 ${EXTRA_JAVA_PROPERTIES}"` in `setUserOverrides.sh` |
| `FRM-93131: Cannot find base HTML file basesaa.txt` | The Forms environment is not set for the server | `setUserOverrides.sh` (`FORMS_INSTANCE`, `ORACLE_PATH`, `TNS_ADMIN`, `FORMS_PATH`) plus the `instances/forms1 -> ../forms1` symlink |
| `BEA-090066 ... boot.properties (Permission denied)` | The file or folder is not readable by the container's user | `chmod -R a+rwX` on the domain's `security` folder, then restart the container (Windows) |
| `frmweb: error while loading shared libraries: libiplsn.so.0` or `FRM-93552: runtime process terminated abnormally` | Oracle ships the 16 Forms web libraries as static `.a` files and relinks them at install time; the slim image lacked build tools | `gcc`, `make`, `binutils` are in `Containerfile.14c`; run `genshlib <name> 0` for each missing library (the script does all 16) |
| `Error initialising the Forms Jdapi: no frmjapi in java.library.path` | `libfrmjapi.so` is a link to a file that was never built | Build `libfrmjapi.so.0` with Oracle's makefile (`forms/lib/ins_forms.mk`), not `genshlib`; run Java with `-Djava.library.path=/oracle_home_14c/lib`; needs `libnsl`, `libXt`, `libaio` |
| `libfrmjapi.so.0: libaio.so.1: cannot open shared object file` | The image lacks `libaio` | It is in `Containerfile.14c`; add it to a custom image |
| `Error opening Oracle*Terminal file fmrweb` | `fmrweb.res` is under `forms/templates/resource/US/`, not the default lookup path | Copy it to `forms/admin/resource/US/` and put that folder in `ORACLE_PATH` (the script does both) |
| Server takes 15 to 44 minutes to reach RUNNING (Windows) | The Oracle Home is on the Windows drive | [oracle-home-location.md](oracle-home-location.md) |

## Using the server and forms

| Symptom | Cause | Fix |
|---|---|---|
| `Welcome to WebLogic Server 14.1.2: The original administration console is no longer supported and has been removed` | WebLogic 14.1.2 dropped the in-browser `/console` | Use the desktop WebLogic Remote Console on `http://localhost:7001`; Forms status is at `/forms/frmservlet/admin` |
| `Unexpected end of file from server` from FSAL | FSAL pointed at port 9001; in the development domain the Forms application is on the AdminServer | Use port 7001: `http://localhost:7001/forms/frmservlet?config=standaloneapp&form=<form>.fmx` |
| `ORA-12154: Cannot find alias FREEPDB1` | The domain's `tnsnames.ora` has no entry | The `FREEPDB1` entry pointing at `oracle-db-23ai:1521` (the script adds it) |
| The native Oracle **Logon** dialog appears | The form was opened without `userid`, or the alias did not resolve | Enter `DB_USER`, `DB_PASSWORD`, and `DB_SERVICE` (from `.env`); or use `launch_fsal.py`, which builds the URL from `.env` |
| `ORA-01017: invalid credential` | The wrong password | The `DB_USER` password in `.env` must match the one the seed script used |
| `FRM-40010: Cannot read form`; `FRM-10221: Cannot read file ...mmx` | The shipped `.fmx` or `.mmx` was compiled elsewhere | Recompile in the container (`compile_form.py`, or the sample compiles in `apply_forms14c_config.sh`) |
| `FRM-10044: Cannot create file` | `frmcmp_batch` cannot write into `/work` or the Oracle Home `forms/` | Compile in a job directory, then copy the result in; on Windows `compile_form.py` runs as root |
| `FRM-30174: Checked and Unchecked check box values must be distinct` | A check box item has no `CheckedValue` and `UncheckedValue` | Set them in the XML (for example `CheckedValue="Y" UncheckedValue="N"`), regenerate the `.fmb` with XML2Forms, recompile |
| `FRM-10039: Unable to start up the Form Builder` | The bare `bin/frmbld` was run; its environment script is an unfilled template | Use the filled command in `forms-builder-gui.md` |
| `must be declared AMOUNT` while compiling | The `test_items` table lacks the `amount` column | Re-run the seed script ([database-23ai.md](database-23ai.md)) |

## Converting

| Symptom | Cause | Fix |
|---|---|---|
| `PDE-PER001` on PLL to PLD | `frmcmp_batch` on 14c needs batch mode | `batch=yes` (the converters add it) |
| `rwconverter: error while loading shared libraries: libnsl.so.2` | The image lacks `libnsl2` | `libnsl2` is in `Containerfile.14c`; rebuild the image |
| PLD to PLL writes no `.pll` | `frmcmp_batch` cannot build a library from `.pld` text, and `Forms2XML` rejects libraries | No command-line route found; see [reports-conversion.md](reports-conversion.md) and the converting skill's known limits |
| XML to OLB or FMB fails on a 12c-format file | The dotted `12.2.1.19.0` Module version is not a number | Use `validate_forms_xml.py` first, and a numeric version such as `141020000` |

## The stack and the Rust app

| Symptom | Cause | Fix |
|---|---|---|
| `Podman runtime directory missing` in `--status` | `/run/user/<uid>` is gone after a reboot or `wsl --shutdown` (Windows) | `python3 start_testbed.py` recreates it and asks for the WSL password |
| `podman` shows no containers on Windows | You are using the Windows `podman` (the stopped `podman-machine-default`), not Ubuntu's | Run `podman` only in the Ubuntu shell |
| The workbench reports `Failed to connect to Oracle at 'localhost:1521/...': No connection could be made because the target machine actively refused it` | The database container is not running (WSL stops with Windows) | `python3 start_testbed.py`; confirm with `--status`; then click **Connect & Run** again. `Test-NetConnection 127.0.0.1 -Port 1521` from PowerShell (use `127.0.0.1`, not `localhost`) |
| `podman start` prints `"/" is not a shared mount` or `failed to connect to syslog` | Harmless warnings in rootless Podman on WSL | Ignore them; `podman info` shows the same `"/" is not a shared mount` line and the containers still work |
| The Rust app: `Address already in use` | Another copy holds port 7070 | `lsof -i :7070`; stop only a process you started |
| Preview in Oracle reports a check as missing | A container, the servlet, Java, or Python is not available to the server | `GET http://127.0.0.1:7070/api/forms/oracle-preview/readiness`; the list of what must be true first is in [windows-podman-status-log.md](windows-podman-status-log.md) |
| Samples keep appearing as `samples/*_edited_N_fmb.xml` | An old running app, or Save & launch used for testing | Restart the app on the current build; do not use Save & launch to test |
