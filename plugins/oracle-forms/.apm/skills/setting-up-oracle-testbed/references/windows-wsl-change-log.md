# Windows (WSL2) setup: every change, every error, every configuration issue

Complete log of the work to run the Oracle Forms 14c, WebLogic, and Oracle Database Free testbed, and the Rust workbench, on Windows 11 with everything inside WSL2. It records what changed in the repo, each error met and its fix, and each configuration adjustment, so the next person does not repeat them. Entries marked **not verified** were not run or confirmed.

Date range: 2026-11-13 to 2026-11-14. Platform: Windows 11, WSL2 Ubuntu 24.04, rootless Podman 4.9.3, Forms 14.1.2, Oracle Database Free 23ai, Rust 1.100.0.

Companion documents: [windows-podman-setup.md](windows-podman-setup.md) (step by step, and the 16-row checklist in section 10), `docs/WINDOWS-PODMAN-SETUP.md` (status log), and `HOW-TO-RUN.md` sections 6 and 7.5.

## 1. How the setup is arranged

| Piece | Where it runs | Where its state lives |
|---|---|---|
| Podman, Python 3.12, Java 21, Rust app | Ubuntu in WSL2 | `~/ofw-target` (Rust build output); `/run/user/<uid>` (Podman runtime, recreated after `wsl --shutdown`) |
| Oracle Database Free (`oracle23ai-lite`) | container | the database's own storage in that container; holds the test schema and tables |
| WebLogic and Forms (`forms-server-14c`) | container from `forms-converter-14c:latest` | **the mounted host folder** `oracle-reference-stack/oracle_home_14c/` (domain, hooks, libraries, `tnsnames.ora`) |
| Converters and compilers | throwaway `podman run --rm` containers from the same image | the same mounted Oracle Home and the repo at `/work` |
| Forms Builder and FSAL windows | WSLg (not XQuartz): socket `/mnt/wslg/.X11-unix`, `DISPLAY=:0` | none |

The image holds only Oracle Linux, JDK 17, and system libraries. Rebuilding it or recreating a container does not lose the Oracle Home configuration. `forms-server-14c` is started with network `oracle-net`, ports 7001 and 9001, mounts `oracle_home_14c` to `/oracle_home_14c` and the repo to `/work`, and runs `startWebLogic.sh`. Its full settings were saved with `podman inspect` into the git-ignored `temp/image-backup/`.

**Reapplying the configuration:** `bash oracle-reference-stack/apply_forms14c_config.sh` (with `.env` loaded) applies every post-install adjustment in section 3. **Not verified:** that script has not been run end to end on a clean Oracle Home.

## 1b. The first approach, and why it was abandoned: native Windows with Visual C++

The first attempt built and ran the Rust app as a **native Windows program** (MSVC toolchain and Visual Studio build tools), with Windows Python and Windows Podman, and Windows Java for FSAL. It was a dead end. The user called it a red herring, and everything now runs inside WSL2. Do not repeat it unless you cannot use WSL. The native route is kept only as an alternative in `docs/DEPENDENCIES.md` section 3.1.

| Problem on the native Windows route | Cause | Result |
|---|---|---|
| `cargo build`: `LINK : fatal error LNK1104: cannot open file 'msvcrt.lib'` | The MSVC x64 build environment was not loaded | Needed a Developer PowerShell or `vcvars64.bat` |
| `cargo build`: `Cannot open include file 'stdint.h'` | Visual Studio needed the C++ workload and the Windows SDK, and `LIB` and `INCLUDE` set | Needed extra Visual Studio components |
| Preview dialog showed "Python 3", "Forms 14c container", and "Forms servlet" as missing | The app ran `python3` (Windows has `python`) and the Windows `podman`, while the containers are in WSL | Needed `oracle_preview.rs` and `run_form.py` changes to call WSL; none of this is needed when the app runs in WSL |
| FSAL window failed: `NoClassDefFoundError: java/applet/AppletStub` | Windows Java 26 removed `java.applet` | Needed Java 17 or 21; WSL's Java 21 works |
| Slow builds and many `wsl.exe` hops | Windows-drive builds and process calls across the boundary | Building in WSL (`CARGO_TARGET_DIR=$HOME/ofw-target`) is faster |
| Original plan: Windows Podman 5.7.0 with a 2 GiB machine (`podman-machine-default`) | Too small for WebLogic, Forms, and the database; memory is set in `%USERPROFILE%\.wslconfig` | Replaced by Podman installed inside Ubuntu in WSL2 (this is from the original plan; the machine itself was not used in the final setup) |

The full step-by-step record of this route (errors in order, the Visual Studio repair command, exit code 5007, the hidden "insert the disk" prompt, the `LIB`/`INCLUDE` workaround, and the native result) is in `windows-podman-setup.md` section 12 and `docs/DEPENDENCIES.md` section 3.1. A stale "being resolved" line about the Rust gate in `windows-podman-setup.md` section 8 was corrected.

The checked-in Windows branches in `oracle_preview.rs` and `run_form.py` remain, so the native route still works as an alternative. macOS behavior is unchanged.

## 2. Repo changes

| File | Change |
|---|---|
| `start_testbed.py` (new) | Starts the database and Forms containers and the Rust app; native WSL Rust start with `CARGO_TARGET_DIR` and `XDG_RUNTIME_DIR`; never kills a running app unless `--restart-rust`; `--stop-rust` works inside WSL; uses `pgrep` on Unix and WSL, `tasklist` on Windows |
| `plugins/.../compiling-and-running-oracle-forms/scripts/compile_form.py` | Podman command helper: root exec and `wsl`/`env` wrappers on Windows and WSL only; container paths use `as_posix()` |
| `.../scripts/run_form.py` | Same Podman helper; WSL Java 21 for FSAL on Windows; POSIX container paths |
| `.../converting-oracle-artifacts/scripts/converter_runtime.py` (new) | Shared 14c image name, Oracle Home path, Podman command for macOS, WSL, and Windows, and the `--check` probe |
| `.../scripts/convert_fmb.py`, `convert_pll.py`, `convert_rpt.py` | Moved to the 14c image and `/oracle_home_14c`; `batch=yes` on 14c; Forms2XML as a Java class with `-Djava.library.path` and `FORMS_API_TK_BYPASS=TRUE`; direct `rwconverter` call |
| `.../scripts/test_conversions.py` (new) | Runs every conversion path for the five types and prints a PASS, FAIL, BLOCKED, SKIP table |
| `crates/oracle-forms-server/src/service.rs` | Save Form updates `samples/<form>_fmb.xml` in place (temp file plus rename); summary-only fallback writes a numbered copy |
| `crates/oracle-forms-server/src/oracle_preview.rs` | `python_program()`, `wsl_distro()`, `podman_command()` (Windows through `wsl.exe`), `CREATE_NO_WINDOW` on Windows; non-Windows behavior unchanged |
| `crates/oracle-forms-server/src/routes.rs`, `ui/app.js` | XML Source tab loads the full XML for the open form, menu, library, or report; selecting a tree node still shows that node's slice. **Not verified in the running app** (not restarted); `cargo check` passes |
| `oracle-reference-stack/sql/seed_test_schema.sql` (renamed from `seed_forms_user.sql`) | Uses `&db_user` and `&db_password`; `test_items` has an `amount` column and five rows |
| `oracle-reference-stack/Containerfile.14c` | Adds `libnsl2` (needed by `rwconverter`). **Not rebuilt yet** |
| `oracle-reference-stack/apply_forms14c_config.sh`, `oracle-reference-stack/forms14c-config/` (new) | The reapply script and its templates |
| `example.env`, `scripts/create_env.py`, `.gitignore` | Credentials live in the git-ignored `.env`; no usernames or passwords in docs, scripts, SQL, or tests |
| `ORACLE-FORMS-BUILDER-GUI.md` | Windows section: `frmbld` works through WSLg; exact 14c command that fills the `frmbld.sh` template placeholders |
| Docs and skill | `HOW-TO-RUN.md`, `AGENTS.md`, `README.md`, `architecture.md`, `docs/*`, the setup skill, and the converting skill updated for all-in-WSL |

## 3. Configuration adjustments (all live in the mounted Oracle Home)

1. `bin/setUserOverrides.sh` in the domain: `LD_LIBRARY_PATH`, `FORMS_PATH`, `FORMS_INSTANCE`, `ORACLE_PATH`, `TNS_ADMIN`, `-Dforms.instance=forms1`.
2. `servers/AdminServer/security/boot.properties` generated from `WLS_ADMIN_USER` and `WLS_ADMIN_PASSWORD`; made readable by the container.
3. Symlink `config/fmwconfig/components/FORMS/instances/forms1 -> ../forms1`.
4. `FREEPDB1` entry in `config/fmwconfig/tnsnames.ora`, pointing at `oracle-db-23ai:1521`.
5. `fmrweb*.res` copied into `forms/admin/resource/US`.
6. The 16 `frmweb` shared libraries built with `genshlib <name> 0`.
7. `libfrmjapi.so.0` built with Oracle's makefile (`forms/lib/ins_forms.mk`), after adding unversioned links for `libaio`, `libX11`, `libXm`, `libXt`, `libXext`, `libXp`, `libstdc++`. `genshlib` is the wrong tool for this library.
8. `.mmx` and `.fmx` for the sample menu and form compiled and copied into `forms/`.
9. WSL: `[automount] options = "metadata,umask=22,fmask=11"` in `/etc/wsl.conf` (then `wsl --shutdown`); `/run/user/<uid>` recreated with sudo after every `wsl --shutdown`; `export XDG_RUNTIME_DIR=/run/user/$(id -u)`.
10. Podman containers writing to the Windows drive run as `--user root` (container root maps to the Windows user; the `oracle` user maps to a UID that cannot write).

## 4. Errors and fixes

| # | Error or symptom | Cause | Fix |
|---|---|---|---|
| 1 | `RunRoot ... not writable`, `IPAM error` after reboot | `/run/user/<uid>` gone after `wsl --shutdown` | Recreate it with sudo; export `XDG_RUNTIME_DIR` |
| 2 | `chmod: Operation not permitted` on `/mnt/c` | Drive mounted without `metadata` | `/etc/wsl.conf` automount options, then `wsl --shutdown` |
| 3 | `Permission denied` in the domain folder, `Unable to access jarfile frmsal.jar` | Container-created files owned by a subordinate UID | `podman unshare chmod -R a+rX oracle_home_14c` (slow) |
| 4 | `BEA-090066 ... boot.properties (Permission denied)` | File written from Windows, owned by the host user | `chmod -R a+rwX` on the `security` folder and file, restart the container |
| 5 | `FRM-93131 ... basesaa.txt` | Forms environment not set | `setUserOverrides.sh` plus the `instances/forms1` symlink plus `fmrweb` resources |
| 6 | `FRM-93552: runtime process terminated abnormally` | The 16 `frmweb` libraries never built | `genshlib` loop (section 3, item 6) |
| 7 | `no frmjapi in java.library.path` | `libfrmjapi.so` linked to a file that did not exist | Build with Oracle's makefile |
| 8 | `undefined symbol __intel_sse2_strncat` or `__gxx_personality_v0`; `ld: cannot find -laio / -lX11 ...` | `genshlib` links only its own objects; unversioned library names missing | Add the links, run `make -B -f ins_forms.mk` from `forms/lib` |
| 9 | `ORA-12154: Cannot find alias FREEPDB1` | `tnsnames.ora` had no entries | Add the `FREEPDB1` entry |
| 10 | `FRM-40010: Cannot read form`; `FRM-10221 ... .mmx` | The shipped `.fmx` and `.mmx` were compiled elsewhere | Recompile in the container |
| 11 | `FRM-10044: Cannot create file` | `frmcmp_batch` cannot write into `/work` or the Oracle Home `forms/` | Compile in a job directory and copy; `compile_form.py` also runs as root on Windows and WSL |
| 12 | `Cannot write to file` from XML2Forms | Container `oracle` user cannot write on `/mnt/c` | Run as `--user root` |
| 13 | `FRM-10039` from a bare `bin/frmbld` | `bin/*.sh` files are templates with `%ORACLE_HOME%`-style placeholders | Use the filled command in `ORACLE-FORMS-BUILDER-GUI.md` |
| 14 | `NoClassDefFoundError: java/applet/AppletStub` | Windows Java 26 removed `java.applet` | Use Java 17 or 21; WSL's OpenJDK 21 works |
| 15 | Server takes 15 to 45 minutes to reach RUNNING | WebLogic reads the Oracle Home from the Windows drive | Wait; the servlet answers right after RUNNING |
| 16 | `... must be declared AMOUNT` when compiling a sample form | My seed script had no `amount` column on `test_items` | Added the column and rows to the seed script, and altered the live table |
| 17 | App kept creating `samples/*_edited_N_fmb.xml` copies | Old running app and Save & launch rewrote the form each click | Save Form now updates the one file in place; restart the app; do not use Save & launch to test |
| 18 | `start_testbed.py` stopped the running app | It killed the app unconditionally; `--stop-rust` delegated to Windows | Fixed: it never stops a running app unless `--restart-rust`; `--stop-rust` is native in WSL |
| 19 | A Python console window popped up | App started detached without a console | `CREATE_NO_WINDOW` in `oracle_preview.rs` |
| 20 | `PDE-PER001` on PLL to PLD | `frmcmp_batch` on 14c needs `batch=yes` | Added to `convert_pll.py` and `convert_fmb.py` |
| 21 | Converters pointed at the 12c image | Hard-coded `forms-converter:latest` and `/oracle_home` | `converter_runtime.py` defaults to 14c; `OFW_CONVERTER_IMAGE` overrides |
| 22 | `rwconverter: error while loading shared libraries: libnsl.so.2` | Image lacks `libnsl2` | Added to `Containerfile.14c`; **rebuild not done yet** |
| 23 | PLD to PLL: `frmcmp_batch` writes no `.pll` (earlier run reported `FRM-10043`) | No command-line import from `.pld` found; `Forms2XML` rejects libraries | **Unresolved**; recorded as a known limit |
| 24 | XML to OLB rejected | Sample XML was a 12c version | 14c-format XML (from OLB to XML) converts back fine |
| 25 | XML Source tab blank until a tree node was picked | The tab only loaded node slices | Loads the full file by default (**not verified in the running app**) |
| 26 | Selecting one item in XML Source shows two lines | Start line appears one too early | **Open**; not investigated |
| 27 | Variables not expanded in `wsl bash -lc '...'` from PowerShell; PowerShell here-strings corrupted edits | Quoting layers | Write the commands to a script file and run `wsl -d Ubuntu -- bash <script>`; use single-quoted here-strings |

## 4b. Documentation gaps found and fixed (2026-11-14)

Gaps found when reviewing `windows-podman-setup.md` against what the setup needed:

| Gap | Fix |
|---|---|
| Section 0 code blocks were corrupted (`` `ash `` where `` ```bash `` belongs) | Repaired |
| No command installed the Ubuntu tools (`python3`, `gcc`, `unzip`, Java 21) or Ubuntu itself | Added to section 3 (`wsl --install -d Ubuntu-24.04`, `apt-get install`). The Java package name is **not verified** |
| The Fusion Middleware adjustments existed only as prose | `apply_forms14c_config.sh` and `forms14c-config/` (**not run on a clean Oracle Home**) |
| No record of the abandoned native Windows and Visual C++ route | Section 1b above |
| `rwconverter` failure and PLD to PLL finding not recorded anywhere | Errors 22 and 23 above; converting skill `SKILL.md` |
| `docs/DEPENDENCIES.md`: said host Java 26 was "unverified" (it is verified to fail); recommended native Windows Python and Java installs instead of the Ubuntu ones; had no Ubuntu package row; listed OpenJDK packages for the 14c image although `Containerfile.14c` installs Oracle JDK 17.0.12 from a tarball, and omitted `make`, `libaio`, `libnsl2`, `procps-ng`, `hostname`, `tar`, `gzip` | Corrected in section 2.1 and 2.2. Ubuntu package names other than `podman` and `build-essential` are **not verified** |
| `docs/DEPENDENCIES.md` section 2.6 still said the `metadata` mount option was "not yet tested" and that `chmod +x` fails on `/mnt/c`; there was no list of everything downloaded | Section 2.6 corrected (the `metadata` fix is verified); new section 2.4b lists every download with size and source |
| Repeated instruction to document every step was not followed, and nothing in the repo forced it | New always-on rule `.agent/rules/document-every-step.md`; `AGENTS.md` now has a mandatory-rules section at the top that makes `.agent/rules/` unskippable; new `CLAUDE.md` tells Claude to read `AGENTS.md` and `.agent/rules/` first |
| Remaining native-Windows and Visual Studio references still said "alternative" instead of "red herring" (`README.md`, `architecture.md`, `HOW-TO-RUN.md`, `docs/WINDOWS-PODMAN-SETUP.md`, `start_testbed.py` comments) | Updated; `windows-podman-setup.md` section 12 records the route |
| No single checklist for how the Rust app launches Fusion Middleware and what must be true first | Added as `windows-podman-setup.md` section 13 |
| No click-by-click steps for downloading the installers from Oracle eDelivery | **Still missing.** Only part numbers, file names, sizes, and destinations are recorded; the person who did the downloads needs to supply the steps |
| No `.gitattributes`, and Windows `core.autocrlf=true` would check out `*.sh`, `*.rsp`, `*.loc`, `*.entry`, and `Containerfile*` with CRLF, which breaks them in WSL and containers (`bash\r: No such file`) | Added `.gitattributes` with `eol=lf` for those patterns |
| Installer response files: confirmed present in `oracle-reference-stack/installer-templates/` and identical to the ones in `installer/` (`INSTALL_TYPE` is `Fusion Middleware Infrastructure` and `Forms and Reports Deployment`; inventory in `/home/oracle/oraInventory`) | No fix needed |

## 5. Open items and unverified claims

- `apply_forms14c_config.sh` has not been run on a clean Oracle Home.
- The image has not been rebuilt with `libnsl2`; report conversion (XML to RDF, RDF to XML) is untested.
- PLD to PLL has no working path on 14c.
- XML Source tab change was compiled (`cargo check`) but not seen in the running app.
- Two-lines-per-selected-item bug in XML Source is open.
- Whether the `--user root` rule is needed inside WSL is not verified.
- macOS was not re-run for any of these results.
- Rust 1.100 prints 8 manifest lint warnings (unused dependencies) while AGENTS.md requires zero warnings; needs a decision.
- XLIFF, `.rpt`, and `.rex` conversions are untested.
