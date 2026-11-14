# How to Run Oracle Forms Workbench

This guide explains how to compile, test, and run the **Oracle Forms Workbench** (`oracle-forms-workbench`) native Rust server, CLI, and database query engine.

Zero JVM, zero Node.js, and zero Oracle runtime software are required.

---

## 1. Prerequisites

You only need standard Rust and Cargo installed:

```bash
# Verify Rust is installed (Rust 1.80+ recommended)
rustc --version
cargo --version
```

If not installed: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`

---

## 2. Starting the Web Viewer Server

### Default Port (7070)

From the repository root, run:

```bash
cargo run -p oracle-forms-server
```

You will see:
```text
============================================================
  Oracle Forms Native Viewer (Rust / Axum Server)
  Listening on http://127.0.0.1:7070
============================================================
```

Open your browser to:
👉 **[http://127.0.0.1:7070](http://127.0.0.1:7070)**

### Custom Port (e.g., 8080 or 7071)

Pass the `PORT` environment variable:

```bash
PORT=8080 cargo run -p oracle-forms-server
```

### Network Access & Database Credentials

The server is a local tool: it can read files and connect to databases, so by default it
listens on `127.0.0.1` only, answers only to `localhost` / `127.0.0.1` / `[::1]` Host names,
and refuses API calls from other websites (cross-origin requests).

| Variable | Default | Effect |
| :--- | :--- | :--- |
| `PORT` | `7070` | Listening port |
| `OFW_HOST` | `127.0.0.1` | Listening address. A non-loopback value (e.g. `0.0.0.0`) exposes the server to your network and prints a warning |
| `OFW_ALLOWED_HOSTS` | *(none)* | Extra comma-separated Host names to accept (needed with a non-loopback `OFW_HOST`) |
| `OFW_SERVER_ENV_CREDENTIALS` | off | When `1`, database requests from the UI that carry no credentials use `OFW_DB_USER` / `OFW_DB_PASSWORD` |

There are no built-in database credentials. To query Oracle from the web UI, start the server with:

```bash
OFW_DB_USER=$DB_USER OFW_DB_PASSWORD='<password>' OFW_SERVER_ENV_CREDENTIALS=1 \
  cargo run -p oracle-forms-server
```

If the database is unreachable or the credentials are missing, you get an error. The server and
CLI never fall back to mock data.

---

## 3. Using the Web Workbench

### Running a Form Live (triggers + PL/SQL)

**▶ Run Form** starts a live session: the form's triggers, program units and attached
`.pld` libraries run on the server, against the database chosen in the **DB** selector.

1. Open a form (e.g. `test_db_fmb.xml`).
2. Choose a backend. For Oracle, enter the DB user and password in the toolbar (sent only
   to this local server, never stored). `SQLite` / `Mock` need no credentials.
3. Click **▶ Run Form**. Startup triggers run; push buttons fire `WHEN-BUTTON-PRESSED`,
   leaving an edited field fires `WHEN-VALIDATE-ITEM`, `SHOW_ALERT` opens a real dialog.
4. Sessions are **read-only** by default (queries run; DML, locks and commits are refused).
   Tick **Allow writes** before starting to enable `💾 Commit`.
5. Open **Session trace** under the canvas to see every trigger, SQL statement and built-in.

Anything the runtime cannot do (e.g. `HOST`, `OLE2`, `CALL_FORM`, LOVs, timers) stops the
trigger with a clear "Not supported" message instead of silently skipping it.

**Export XML** returns the original Forms2XML file with only your layout edits applied
(lossless). Forms opened from a binary `.fmb` can only be exported as a labelled summary.



Once the server is running at `http://127.0.0.1:7070`:

1. **Open Sample Forms**:
   - Use the **"Select Sample"** dropdown in the top header.
   - Choose any sample (e.g. `test_db_fmb.xml`, `orders_fmb.xml`, `claims_fmb.xml`, `simple_module.fmb`).
2. **Open Your Own Files**:
   - Drag & drop any `.fmb`, `.xml`, `.pld`, `.mmb`, or `.olb` file into the browser window.
   - Or type a local file path into the **"File Path"** input box and click **"Open"**.
3. **Execute Live Database Queries (Phase A)**:
   - Select your backend in the **DB:** dropdown:
     - `Oracle 23ai (:1521)`: Connects directly to Oracle Database 23ai Free (`localhost:1521/FREEPDB1`).
     - `SQLite (Embedded)`: Uses in-memory SQLite with pre-seeded datasets.
     - `Mock Offline`: Offline driver returning deterministic test rows.
   - Click **⚡ Query DB** (or press keyboard shortcut `Ctrl+F11` / `F8`).
   - The canvas automatically populates the form fields with live data!
   - Use the record navigator (`[◀] Rec 1 of 4 [▶]`) to browse records.
4. **Interactive Features**:
   - **Canvas View**: Pan, zoom, and visually inspect 2D coordinates, buttons, frames, prompts, and layout elements.
   - **Object Navigator**: Tree navigation across Blocks, Items, Triggers, Program Units, LOVs, and Alerts.
   - **PL/SQL Tab**: Syntax-highlighted view of procedures, functions, package bodies, and triggers with clean line numbers.
   - **XML Tab**: Direct view of raw XML elements and line boundaries.

### Interactive Preview & Runtime Frame

Enable **Interactive Preview** to edit local field values, toggle checkboxes, and test form behaviors
inside the existing canvas layout. The runtime frame provides classic Oracle Forms menus, toolbars, and status bar.

---

## 4. Running the CLI Toolkit

You can inspect modules, extract PL/SQL, or query database records directly from your terminal:

```bash
# 1. Execute live query against Oracle Database 23ai Free (:1521)
#    (credentials come from OFW_DB_USER / OFW_DB_PASSWORD; there are no defaults)
OFW_DB_USER=$DB_USER OFW_DB_PASSWORD='<password>' \
  cargo run -p oracle-forms-cli -- query samples/test_db_fmb.xml --backend oracle-thin

# 2. Execute query offline using mock driver
cargo run -p oracle-forms-cli -- query samples/orders_fmb.xml --backend mock

# 3. Inspect an XML Form (Module summary)
cargo run -p oracle-forms-cli -- inspect samples/orders_fmb.xml

# 4. Inspect a Binary .FMB Module directly (ROS 60050 stream decoding)
cargo run -p oracle-forms-cli -- inspect samples/simple_module.fmb

# 5. Output the complete module index as JSON
cargo run -p oracle-forms-cli -- inspect samples/orders_fmb.xml --json

# 6. Dump all extracted PL/SQL (triggers and procedures) into a directory as .sql files
cargo run -p oracle-forms-cli -- dump-sql samples/orders_fmb.xml -o ./extracted-sql
```

---

## 5. Running the Automated Test Suite

To run all unit, differential, and conformance tests:

```bash
cargo test
```

All 5 integration test suites will execute:
- `test_orders_geometry_and_triggers`
- `test_claims_geometry_and_triggers`
- `test_pld_parsing`
- `test_simple_module_binary_parsing`
- `test_direct_binary_fmb_parsing`

---

## 6. Managing & Restarting the Server Process

### Start or Restart Everything: `start_testbed.py`

One Python script starts and restarts the whole stack on macOS, Linux, and Windows. Run it from the repository root (`python` on Windows, `python3` elsewhere):

```bash
python start_testbed.py                  # database + Forms (Fusion Middleware) server + Rust app
python start_testbed.py --no-forms       # database + Rust app only (fast)
python start_testbed.py --no-wait-forms  # start everything, do not wait for the Forms server
python start_testbed.py --status         # show what is running
python start_testbed.py --stop-rust      # stop only the Rust app
python start_testbed.py --restart-rust   # rebuild and restart the Rust app even if it is running
python start_testbed.py --launch ofw_demo_employees.fmx   # open a deployed form in the Forms desktop client
```

It makes Podman usable (macOS: starts the Podman machine; Windows: recreates the WSL runtime directory and asks for your WSL `sudo` password), starts `oracle23ai-lite` and waits until the database is ready, starts `forms-server-14c`, builds and starts the Rust app in the background if it is not already running (it never stops a running app unless you pass `--restart-rust`; log: `temp/oracle-forms-server.log`, or `temp/oracle-forms-server.wsl.log`-style output when you start it by hand in WSL), then waits for the Forms server to reach RUNNING (15 to 45 minutes). Run inside WSL, it builds in WSL with `CARGO_TARGET_DIR=~/ofw-target`; run from Windows PowerShell, it uses the Windows build. It starts existing containers only; first-time setup is in the setup skill. Credentials come from `.env`. Add `--db-credentials` to start the Rust app with `OFW_SERVER_ENV_CREDENTIALS=1`.

### Stop the Server
Press `Ctrl + C` in the terminal where `cargo run` is running.

### Check if Port 7070 is in Use
```bash
lsof -i :7070
```

### Kill any Process on Port 7070
```bash
kill -9 $(lsof -t -i:7070) 2>/dev/null || true
```

### One-Line Server Restart

If you made code/UI changes or encounter `Address already in use (os error 48)`:

```bash
kill -9 $(lsof -t -i:7070) 2>/dev/null || true; cargo run -p oracle-forms-server
```

### Windows: run everything inside WSL (recommended)

On Windows, run the Rust app **inside the same Ubuntu WSL distro as the containers**, the same way you would on macOS or Linux. It is faster (no `wsl.exe` hops, no slow Windows-drive builds), it needs **no Visual Studio and no Windows SDK**, and it uses almost the same code path as macOS, because `python3`, `podman`, and Java 21 are all Linux tools inside WSL. The browser stays on Windows and opens `http://127.0.0.1:7070`.

Verified on Windows 11: the app builds and runs in WSL; the Preview dialog's readiness checks all pass; **Save & launch in Oracle** converts, compiles, and opens the form; and `cargo test` passes 120 tests in WSL (0 failed, 0 errors).

One-time setup, in the Ubuntu shell (`wsl -d Ubuntu`). Rust needs a C compiler (`gcc`; run `sudo apt-get install -y build-essential` if `gcc --version` fails):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
```

Run the app from the repo root in the Ubuntu shell:

```bash
cd /mnt/c/Users/<you>/source/repos/oracle-forms-workbench     # your clone
. "$HOME/.cargo/env"
export XDG_RUNTIME_DIR=/run/user/$(id -u)   # Podman's runtime directory (see "After a Windows reboot" below)
export CARGO_TARGET_DIR=$HOME/ofw-target     # build output on WSL's own disk: much faster than /mnt/c
cargo run -p oracle-forms-server             # serves http://127.0.0.1:7070
```

Stop it with `Ctrl + C`. To run it in the background: `setsid nohup cargo run -p oracle-forms-server > temp/oracle-forms-server.wsl.log 2>&1 &`. To let the UI use the credentials from `.env`: `set -a; source .env; set +a` first, then add `OFW_SERVER_ENV_CREDENTIALS=1` before `cargo run`. `python3 start_testbed.py` (below) does the same steps. Do not run a Windows copy and the WSL copy at the same time; both use port 7070.

Known cosmetic point: Rust 1.100 prints 8 manifest lint warnings (a redundant `homepage` and an unused `tracing` dependency in some `Cargo.toml` files). They are not code warnings, and Rust 1.94 prints none.

### Windows: native build (alternative; not needed)

Kept for reference, in case you cannot use WSL for the app. The native Windows build needs Visual Studio with the C++ workload and a Windows SDK (see `docs/DEPENDENCIES.md` section 3.1), and the Preview dialog then reaches the containers through `wsl.exe`. It is slower and has more moving parts than running the app in WSL. Verified on Windows 11 with Visual Studio 2026 Community and the Windows 10 SDK 10.0.19041.0. Rust on Windows builds with the MSVC toolchain, and the build needs the include and library paths set. Run this in a `cmd` window (adjust the Visual Studio folder to your install):

```bat
set "VCT=<Visual Studio folder>\VC\Tools\MSVC\<version>"
set "SDK=C:\Program Files (x86)\Windows Kits\10"
set "SDKV=10.0.19041.0"
set "LIB=%VCT%\lib\onecore\x64;%SDK%\Lib\%SDKV%\ucrt\x64;%SDK%\Lib\%SDKV%\um\x64"
set "INCLUDE=%VCT%\include;%SDK%\Include\%SDKV%\ucrt;%SDK%\Include\%SDKV%\um;%SDK%\Include\%SDKV%\shared"
cargo run -p oracle-forms-server
```

The server listens on `http://127.0.0.1:7070`. To stop and restart it from PowerShell:

```powershell
Get-Process oracle-forms-server -ErrorAction SilentlyContinue | Stop-Process     # stop
Get-NetTCPConnection -LocalPort 7070 -State Listen -ErrorAction SilentlyContinue  # confirm the port is free (no output)
cargo run -p oracle-forms-server                                                  # start (repo root, paths above set)
```

If you built it already, run `.\target\debug\oracle-forms-server.exe` from the repo root instead of `cargo run`. Starting it from a different folder means the samples and UI files are not found.

To let the UI query the database, load `.env` first. In PowerShell: `Get-Content .env | ForEach-Object { $k,$v = $_ -split '=',2; if ($k -and -not $k.StartsWith('#')) { Set-Item "env:$k" $v } }`. Then start the server with `$env:OFW_SERVER_ENV_CREDENTIALS = '1'`.

#### After a Windows reboot or `wsl --shutdown`

The quick way: from the repository root, run `python start_testbed.py`. It recreates the WSL runtime directory (it asks for your WSL `sudo` password in the terminal), starts the database and Forms server containers, rebuilds and restarts the Rust app, and waits for the Forms server to reach RUNNING (15 to 45 minutes). Use `--no-forms` to skip the Forms server, `--no-wait-forms` to return right after starting it, and `--status` to see what is running. The manual steps below do the same thing one command at a time.

The database runs in a container inside WSL, and WSL stops with Windows. Until the container is back, the workbench reports `Failed to connect to Oracle at 'localhost:1521/...': No connection could be made because the target machine actively refused it`. Restart the pieces in this order, from the Ubuntu shell (`wsl -d Ubuntu`):

1. Recreate the rootless Podman runtime directory (needs `sudo`; this is the step an agent cannot do for you):
   ```bash
   sudo mkdir -p /run/user/$(id -u) && sudo chown $(id -un): /run/user/$(id -u) && chmod 700 /run/user/$(id -u)
   mkdir -p /run/user/$(id -u)/containers/networks
   export XDG_RUNTIME_DIR=/run/user/$(id -u)
   ```
2. Start the database container. Its data is kept, so there is no reseed:
   ```bash
   podman start oracle23ai-lite
   podman logs -f oracle23ai-lite    # wait for DATABASE IS READY TO USE!
   ```
   Warnings such as `"/" is not a shared mount` and `failed to connect to syslog` are harmless.
3. Start the Forms server only if you need the Oracle FSAL comparison. It takes about 15 to 45 minutes to reach RUNNING: `podman start forms-server-14c`. The Rust app does not need it.
4. Confirm from PowerShell that the database port answers: `Test-NetConnection 127.0.0.1 -Port 1521`. Use `127.0.0.1`, not `localhost`.
5. Restart the Rust app (in WSL: the commands in "run everything inside WSL" above, or `python3 start_testbed.py`), then click **Connect & Run** again in the workbench.
---

## 7. Running Forms Live via Desktop FSAL (Track 1 Container Testbed)

In addition to the database-free native Rust viewer, this repository includes an official Oracle Forms 14c + WebLogic + Oracle Database 23ai Free testbed running via Podman containers. Forms run as native desktop applications via the **Forms Standalone Launcher (FSAL)** without needing XQuartz, X11 forwarding, or browser plugins.

### Prerequisites

1. **Podman Containers Running**:
   - `oracle23ai-lite` on port 1521
   - `forms-server-14c` on port 7001
   - Start them, and the Rust app, with `python start_testbed.py` (section 6).
2. **Java Runtime on Host**: Java 8, 11, 17, or 21+ (`java -version`).
3. **Python 3**: For running the APM automation scripts.

### 1. Verify Testbed Health

Run the automated health check:
```bash
python3 plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/scripts/health_check.py
```
Expected output:
```text
=== Oracle Testbed Health Check ===
[1/3] Checking Podman containers...
  - Oracle Database 23ai: RUNNING (ok)
  - Oracle Forms 14c Server: RUNNING (ok)
[2/3] Checking Forms Listener Servlet (Port 7001)...
  - Forms Listener Servlet: UP AND RUNNING (ok)
[3/3] Checking Database Connection & Query (FREEPDB1)...
  - Database Query test_items: SUCCESS (ok)

All systems operational! Ready to compile and run Forms.
```

### 2. One-Command Form Runner (`run_form.py`)

Convert (if XML), compile (`.fmb` -> `.fmx`), and launch any form immediately in a native desktop window:

```bash
# Run the database-connected test form:
python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/run_form.py samples/test_db.fmb

# Or run with automatic database logon (bypasses logon dialog):
python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/run_form.py samples/test_db.fmb --auto-login

# Run a custom XML form:
python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/run_form.py samples/simple_module_fmb.xml
```

### 3. Step-by-Step Manual Workflow

If you prefer to compile and launch separately:

#### Step A: Headless Batch Compile
```bash
python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/compile_form.py samples/test_db.fmb
```

#### Step B: Launch Desktop FSAL Client
```bash
python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/launch_fsal.py test_db.fmx
```

### 4. Database Logon Credentials

Credentials live in a git-ignored `.env` file at the repo root, not in these docs. Copy `example.env` to `.env` and fill in the values. The variables are:

- `DB_USER`, `DB_PASSWORD`, `DB_SERVICE`: the test schema user, its password, and the database service. Used for the Forms logon dialog and the FSAL `userid=` parameter.
- `OFW_DB_USER`, `OFW_DB_PASSWORD`: read by the Rust server and CLI.
- `ORACLE_PWD`: the database admin password, used when the database container starts.
- `WLS_ADMIN_USER`, `WLS_ADMIN_PASSWORD`: the WebLogic admin account.

Load the file into a bash shell before running the commands below:

```bash
set -a; source .env; set +a
```

Windows PowerShell does not read `.env` automatically. Set the variables by hand, or load them from the file.

When prompted by the native Forms **Logon** dialog, enter the values from `.env`:
- **Username**: `$DB_USER`
- **Password**: `$DB_PASSWORD`
- **Database**: `$DB_SERVICE`

Click **Connect** to load the form and execute live PL/SQL triggers against Oracle Database 23ai.


### 5. Windows (Podman inside WSL2): verified end-to-end

On Windows, Podman runs inside an Ubuntu WSL2 distribution. Run every container command from the Ubuntu shell (`wsl -d Ubuntu`). The Rust app and the launcher run in WSL too (see "Windows: run everything inside WSL" in section 6), using WSLg for the Forms window. Verified on Windows 11 with a sample form: the Employee Directory form (`ofw_demo_employees`) opened with live rows from `$DB_USER` in FREEPDB1.

**After a reboot or `wsl --shutdown`, the verified order is:**

1. Start the containers (steps 1 and 6 below, or `python start_testbed.py` from the repo root).
2. Wait until the Forms servlet answers: `curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:7001/forms/frmservlet` prints `200`. WebLogic reads the Oracle Home from the Windows drive, so this took 15, 18, and 44 minutes on three boots. The log gives no progress between the second `STARTING` line and `ADMIN`; `RUNNING` follows in under a second.
3. Launch the form with step 8. The sample form and menu from step 7 stay compiled in the Oracle Home, so step 7 is not needed again.

Run from `oracle-reference-stack/` in the Ubuntu shell, in this order. Steps 1 to 5 are one-time per machine, except step 1 (runtime directory) and step 6 (start the server), which are needed after every `wsl --shutdown` or reboot.

1. **Runtime directory** (after every `wsl --shutdown`):
   ```bash
   sudo mkdir -p /run/user/$(id -u) && sudo chown $(id -un): /run/user/$(id -u) && chmod 700 /run/user/$(id -u)
   mkdir -p /run/user/$(id -u)/containers/networks
   export XDG_RUNTIME_DIR=/run/user/$(id -u)
   ```
2. **Database** (`oracle23ai-lite` on `oracle-net`) and demo data:
   ```bash
   (printf 'DEFINE db_user=%s\nDEFINE db_password=%s\n' "$DB_USER" "$DB_PASSWORD"; cat sql/seed_test_schema.sql) | podman exec -i oracle23ai-lite sqlplus -s system/$ORACLE_PWD@localhost/FREEPDB1
   podman exec -i oracle23ai-lite sqlplus -s $DB_USER/$DB_PASSWORD@localhost/FREEPDB1 < ../samples/ofw_demo_employees.sql
   ```
3. **Forms server domain hooks** (one-time): `oracle_home_14c/user_projects/domains/base_domain/bin/setUserOverrides.sh` (exports `LD_LIBRARY_PATH`, `FORMS_PATH`, `FORMS_INSTANCE`, `ORACLE_PATH`, `TNS_ADMIN`, `-Dforms.instance=forms1`), `servers/AdminServer/security/boot.properties`, and an `instances/forms1 -> ../forms1` symlink under `config/fmwconfig/components/FORMS/`. The full contents are in `plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/references/windows-podman-setup.md`, section 8.
4. **Forms runtime libraries** (one-time): build the 16 `lib*.so.0` files that `frmweb` needs with `genshlib` (check with `ldd` first; it lists the missing names), and build `libfrmjapi.so.0` for the XML-to-FMB conversion that **Save & launch in Oracle** uses. `genshlib` cannot build `libfrmjapi` (it fails with `undefined symbol: __intel_sse2_strncat` or `__gxx_personality_v0`); it needs Oracle's own makefile. The exact commands, and every other Windows adjustment, are in the setup skill: `plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/references/windows-podman-setup.md`, section 10.
5. **Database alias** (one-time): add a `FREEPDB1` entry to `config/fmwconfig/tnsnames.ora` that points at host `oracle-db-23ai`, port 1521, service `FREEPDB1`. Without it the logon fails with `ORA-12154`.
6. **Start the Forms server** (after the first setup, `python start_testbed.py` does this and the runtime-directory step for you):
   ```bash
   podman run -d --name forms-server-14c --platform linux/amd64 --network oracle-net -p 7001:7001 -p 9001:9001 \
     -v "$PWD/oracle_home_14c:/oracle_home_14c:Z" -v "$PWD/..:/work:Z" \
     -e ORACLE_HOME=/oracle_home_14c -e DOMAIN_HOME=/oracle_home_14c/user_projects/domains/base_domain \
     -e DB_URL=oracle-db-23ai:1521/FREEPDB1 -e DISPLAY=:0 -e JAVA_HOME=/usr/lib/jvm/jdk-17.0.12 \
     forms-converter-14c:latest /oracle_home_14c/user_projects/domains/base_domain/bin/startWebLogic.sh
   podman logs -f forms-server-14c   # wait for "Server state changed to RUNNING"
   ```
   On a `/mnt/c` mount this took 15 to 45 minutes to reach RUNNING (measured: 18, 44, and 15). Confirm with `curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:7001/forms/frmservlet` (expect `200`).
7. **Compile the sample form and menu** in a job directory. Compiling into `/work` or the Oracle Home `forms/` folder fails with `FRM-10044`:
   ```bash
   podman exec -u oracle forms-server-14c sh -c 'mkdir -p /tmp/job && cp /work/samples/ofw_demo_employees.fmb /work/samples/ofw_emp_menu.mmb /tmp/job/ && cd /tmp/job && export ORACLE_HOME=/oracle_home_14c LD_LIBRARY_PATH=/oracle_home_14c/lib:/oracle_home_14c/oracle_common/lib ORACLE_TERM=vt220 TERM=vt220 FORMS_PATH=/tmp/job:/work/samples:/oracle_home_14c/forms && /oracle_home_14c/bin/frmcmp_batch module=ofw_emp_menu.mmb module_type=menu logon=no compile_all=yes && /oracle_home_14c/bin/frmcmp_batch module=ofw_demo_employees.fmb module_type=form logon=no compile_all=yes && cp /tmp/job/ofw_demo_employees.fmx /tmp/job/ofw_emp_menu.mmx /oracle_home_14c/forms/'
   ```
   Each step prints `No compilation errors`. The shipped `.fmx` and `.mmx` in `samples/` are stale and fail with `FRM-40010` and `FRM-10221`.
8. **Launch the form** from the Ubuntu shell, at the repo root. The launcher stays open while the form runs:
   ```bash
   java -jar oracle-reference-stack/oracle_home_14c/forms/java/frmsal.jar \
     -url "http://localhost:7001/forms/frmservlet?config=standaloneapp&form=ofw_demo_employees.fmx&userid=$DB_USER/$DB_PASSWORD@FREEPDB1"
   ```
   The Employee Directory window opens with the demo rows. If the logon dialog appears instead, use `$DB_USER` / `$DB_PASSWORD` / `FREEPDB1`.

**Verified on Windows (everything inside WSL):** steps 1 to 8 above; the Rust app building and running in WSL (`cargo run -p oracle-forms-server`, port 7070); the Preview dialog's readiness checklist (all required checks pass once the Forms servlet is up); **Save & launch in Oracle** (XML to FMB conversion, compile, and the Forms client opening the form), which updates the one file `samples/<form>_fmb.xml` and creates no copies; `launch_fsal.py` run with `python3` in the Ubuntu shell; and the full gate in WSL: `cargo check` (0 errors) and `cargo test` (120 passed, 0 failed). The Windows-native build of the app is also verified (section 6, alternative), but it needs Visual Studio and Java 17 or 21 for FSAL, and it is slower.

**Not verified on Windows:** `compile_form.py` and `health_check.py` in the skill scripts, `ofw_sales.fmb`, `convert_fmb.py --check`, and `frmbld` (the Forms Builder GUI). Use steps 1 to 8 above, or **Save & launch in Oracle**, for the documented, working routes.

## Menus, Browse/Open and Reports (new)

- Use **Browse / Open...** in the header to open any `.fmb`, `.xml`, `.olb`, `.mmb` or report file (samples list or upload).
- To try custom menus: open `ofw_demo_employees_fmb.xml`, then pick `OFW_EMP_MENU` in the canvas toolbar menu selector.
- Open `ofw_employee_directory_report.xml` in the Reports tab for the PL/SQL split view and data model diagram.
- After editing `ui/*`, touch `crates/oracle-forms-server/src/assets.rs` (assets are embedded at compile time) and hard-refresh the browser.
