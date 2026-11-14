# Running Native Oracle Forms Builder GUI (frmbld) & Live Forms

This guide documents how to launch and run the official, visual **Oracle Forms Builder (`frmbld`)** application (12c & 14c) from the containerized Linux installation on macOS, Linux, and Windows, as well as running live forms against Oracle Database 23ai Free via Forms Standalone Launcher (FSAL).

---

## 1. Overview & Purpose

The containerized Oracle installation in `oracle-reference-stack/` serves two distinct purposes:

1. **Batch Conversion Pipeline (`frmf2xml.sh`)**: Headless conversion of binary `.fmb`, `.mmb`, and `.olb` files into canonical XML.
2. **Interactive Native Forms Builder GUI (`frmbld`)**: The full official visual designer and IDE, enabling you to inspect forms, canvases, layout hierarchies, triggers, and properties exactly as Oracle designed them.

Running the native GUI provides the **ultimate ground-truth reference** when evaluating layout rendering, font metrics, coordinate scaling, and trigger execution against our pure-native Rust workbench (`oracle-forms-workbench`).

```mermaid
flowchart LR
    subgraph Host["Host Machine (macOS / Linux / Windows)"]
        X11[XQuartz / X11 Server<br/>Port 6000]
        WS[Workspace Repository<br/>./samples, ./temp]
    end

    subgraph Container["Podman Linux Container (forms-converter:latest)"]
        OH["Oracle Home 12c<br/>/oracle_home"]
        FRMBLD["Oracle Forms Builder<br/>/oracle_home/bin/frmbld"]
    end

    FRMBLD -->|X11 Protocol Display Forwarding| X11
    WS -->|Mounted at /work:Z| FRMBLD
    OH -->|Mounted at /oracle_home:Z| FRMBLD
```

---

## 2. Prerequisites (Host Setup)

### macOS Setup (XQuartz)

Oracle Forms Builder is an X11/Motif application. On macOS, **XQuartz** is required to render its graphical window:

1. **Install XQuartz** via Homebrew (or download from [xquartz.org](https://www.xquartz.org)):
   ```bash
   brew install --cask xquartz
   ```

2. **Enable Network Client Connections (One-Time Setup)**:
   - Open XQuartz: `open -a XQuartz`
   - Open Settings: press `Cmd + ,` or navigate to **XQuartz > Settings...**
   - Click the **Security** tab.
   - Check **"Allow connections from network clients"**.
   - **Important:** Quit XQuartz completely (`Cmd + Q`) and relaunch it for this change to take effect.

3. **Grant Host Display Authorization**:
   Run this in your terminal before launching the container:
   ```bash
   xhost + 127.0.0.1
   xhost + host.containers.internal
   ```

### Linux Setup
On native Linux (X11 / Xorg), display authorization is straightforward:
```bash
xhost +local:
```

### Windows Setup (everything inside WSL2)

- Windows uses Podman inside Ubuntu WSL2, not Podman Desktop. See [docs/WINDOWS-PODMAN-SETUP.md](docs/WINDOWS-PODMAN-SETUP.md).
- **No XQuartz.** XQuartz is the macOS X server. On Windows 11, **WSLg** is the X server and it is built in: Linux GUI programs appear on the Windows desktop with no setup. The display is `DISPLAY=:0`, and its X socket is `/mnt/wslg/.X11-unix`, which you mount into the container as `/tmp/.X11-unix`.
- **Verified (2026-11-14):** the Forms Builder (`frmbld`, 14.1.2) runs from the `forms-converter-14c` image and opens `ofw_sales.fmb` (`/work/samples/ofw_sales.fmb`) on the Windows desktop through WSLg. Live forms through FSAL run the same way (see [HOW-TO-RUN.md](HOW-TO-RUN.md) section 7.5); use Java 17 or 21 (WSL's OpenJDK 21 works, Windows Java 26 does not).
- On **Windows 10**, an X11 server such as VcXsrv would be needed. This has not been tested.

**Run the Forms Builder 14c on Windows (Ubuntu WSL shell, repo root).** Do not run the bare `/oracle_home_14c/bin/frmbld`: it exits at once with `FRM-10039: Unable to start up the Form Builder`, because it needs the environment that Oracle's `frmbld.sh` sets (Java home, class path, instance, `TNS_ADMIN`). In this 14c install the instance scripts were never generated, so `frmbld.sh` exists only as a template with `%PLACEHOLDERS%` under `forms/templates/scripts/`. Fill them in and run it:

```bash
podman run --rm --name frmbld --platform linux/amd64 \
  -e DISPLAY=:0 -e ORACLE_TERM=vt220 -e TERM=vt220 -e JAVA_HOME=/usr/lib/jvm/jdk-17.0.12 \
  -e FORMS_PATH=/work/samples:/oracle_home_14c/forms \
  -v /mnt/wslg/.X11-unix:/tmp/.X11-unix \
  -v "$PWD/oracle-reference-stack/oracle_home_14c:/oracle_home_14c:Z" -v "$PWD:/work:Z" \
  forms-converter-14c:latest sh -c '
    D=/oracle_home_14c/user_projects/domains/base_domain
    sed -e "s#%ORACLE_HOME%#/oracle_home_14c#g" \
        -e "s#%CT_JAVA_HOME%#/usr/lib/jvm/jdk-17.0.12#g" \
        -e "s#%TNS_ADMIN%#$D/config/fmwconfig#g" \
        -e "s#%FORMS_INSTANCE%#$D/config/fmwconfig/components/FORMS/forms1#g" \
        -e "s#%DOMAIN_HOME%#$D#g" \
        /oracle_home_14c/forms/templates/scripts/frmbld.sh > /tmp/frmbld-run.sh
    sh /tmp/frmbld-run.sh'
```

This starts a short-lived container next to the running ones and removes it when you close the Builder. The same template folder holds the other Oracle batch scripts: `frmcmp.sh`, `frmcmp_batch.sh`, `frmf2xml.sh` (FMB to XML), `frmxml2f.sh` (XML to FMB), `frmf2xlf.sh` (XLIFF), `frmxmlsg.sh`, and `frmxmlv.sh`. They are templates, not on the path. To run a conversion without filling in a template, call the Java classes directly (see the conversion notes in the setup skill's `windows-podman-setup.md`).

The Option B command further down is for the 12c image (`forms-converter:latest`, `/oracle_home`, JRE 8) and is not valid for the 14c install above.
---

## 3. Launching Oracle Forms Builder & Live Forms

### Option A: Running Live Forms via Desktop FSAL (Recommended)

To run any form live with database connectivity on your native desktop (without XQuartz or browser plugins), use the APM runner:

```bash
# Compile and run form live on desktop:
python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/run_form.py samples/test_db.fmb
```

---

### Option B: Launching Forms Builder IDE (`frmbld`) via Direct Podman Command

If you prefer to run Podman directly without the script:

```bash
# 1. Authorize X11 connection on host
xhost + 127.0.0.1 host.containers.internal

# 2. Launch Forms Builder inside the container
podman run --rm -it \
  --platform linux/amd64 \
  -e DISPLAY="host.containers.internal:0" \
  -e ORACLE_HOME="/oracle_home" \
  -e JAVA_HOME="/usr/lib/jvm/jre-1.8.0-openjdk" \
  -e LD_LIBRARY_PATH="/oracle_home/lib:/usr/lib/jvm/jre-1.8.0-openjdk/lib/amd64/server:/usr/lib/jvm/jre-1.8.0-openjdk/lib/amd64" \
  -e FORMS_BUILDER_CLASSPATH="/oracle_home/jlib/frmbld.jar:/oracle_home/jlib/importer.jar:/oracle_home/jlib/debugger.jar:/oracle_home/jlib/utj.jar:/oracle_home/oracle_common/jlib/ewt3.jar:/oracle_home/oracle_common/modules/oracle.bali.share/share.jar:/oracle_home/jlib/dfc.jar:/oracle_home/oracle_common/modules/oracle.help/ohj.jar:/oracle_home/oracle_common/modules/oracle.help/help-share.jar:/oracle_home/oracle_common/modules/oracle.help/oracle_ice.jar:/oracle_home/oracle_common/modules/oracle.bali.jewt/jewt4.jar:/oracle_home/forms/java/frmwebutil.jar:/oracle_home/forms/java/frmall.jar" \
  -e FORMS_PATH="/work:/work/samples:/work/temp/example-fmb:/work/temp/example-fmb/internal-testing" \
  -v "$(pwd)/oracle-reference-stack/oracle_home:/oracle_home:Z" \
  -v "$(pwd):/work:Z" \
  forms-converter:latest \
  /oracle_home/bin/frmbld
```

*(On Linux, replace `host.containers.internal:0` with `${DISPLAY:-:0}`)*.

---

## 4. Navigating Inside Forms Builder

Once Forms Builder starts, the classic Oracle Forms IDE interface appears on your desktop:

### 1. Opening a Form
- Click **File > Open** (or press `Ctrl + O`).
- In the file dialog, double-click the **`work/`** directory (this is your project root).
- Navigate to:
  - `work/samples/` for public test forms (`simple_module.fmb`, `object_library.olb`).
  - `work/temp/example-fmb/` for your internal test modules (`rccm0000.fmb`).

### 2. Viewing the Visual 2D Canvas Layout
- In the **Object Navigator** tree, expand your loaded module (e.g. `RCCM0000`).
- Expand the **Canvases** node.
- Double-click the canvas icon (e.g. `CRCCMAINMENU`), or right-click and choose **Layout Editor** (`F2`).
- The native 2D layout editor displays with full vector rendering, fonts, coordinate grids, and frame groupings.

### 3. Inspecting Triggers and PL/SQL
- In the Object Navigator, expand **Triggers** (at the module, block, or item level) or **Program Units**.
- Double-click any trigger icon or press `F11` to open the integrated **PL/SQL Editor**.

### 4. Property Inspector
- Select any block, item, canvas, window, or trigger and press `F4` (or right-click > **Property Palette**).
- Inspect properties such as `X Position`, `Y Position`, `Width`, `Height`, `Bevel`, `Canvas Name`, and `Property Class`.

---

## 5. Resolving Dependencies (`FORMS_PATH`)

Complex Forms often reference:
- **Attached Libraries** (`.pll` files, e.g. `juslib.pll`, `itslib.pll`)
- **Object Libraries** (`.olb` files)
- **Subclassed Property Classes**

When Forms Builder opens a module, it searches the directories listed in the `FORMS_PATH` environment variable. The launcher script pre-configures `FORMS_PATH` with:
- `/work` (project root)
- `/work/samples` (shared fixtures)
- `/work/temp/example-fmb` (local client libraries and modules)

If your form depends on libraries in a different folder, add that folder to the `FORMS_PATH` environment variable in `run_forms_builder.py` or pass it via `-e FORMS_PATH="...:$FORMS_PATH"`.

---

## 6. Troubleshooting

| Symptom | Cause | Solution |
|---|---|---|
| **`FRM-91111: Internal Error: Cannot open display`** | XQuartz is not running, or network connections are disallowed. | 1. Open XQuartz.<br/>2. In **Settings > Security**, check **"Allow connections from network clients"**.<br/>3. Quit XQuartz (`Cmd + Q`) and restart it.<br/>4. Run `xhost + 127.0.0.1` and `xhost + host.containers.internal`. |
| **`podman: command not found`** | Podman is not installed or not in PATH. | Install via Homebrew: `brew install podman` |
| **`Error: cannot connect to the Podman socket`** | Podman machine VM is not running. | Run `podman machine start` |
| **Fonts look small or low-resolution** | High-DPI / Retina display scaling in XQuartz. | In XQuartz Settings > **Output**, check or uncheck **"Enable full screen mode"** or adjust display scaling via `xrandr`. |
| **Missing Attached Library error on Open** | The referenced `.pll` or `.olb` is not in `FORMS_PATH`. | Place the dependent `.pll` / `.olb` files into `temp/example-fmb/` so `FORMS_PATH` discovers them automatically. |

---

## 7. Running Live Forms via Forms Standalone Launcher (FSAL)

To run a form interactively with real database transactions against the local Oracle Database 23ai container:

1. **Start the Oracle Database 23ai Free container**:
   ```bash
   podman start oracle23ai-lite
   ```
2. **Compile the form into runtime format (`.fmx`)**:
   ```bash
   python3 plugins/oracle-forms/.apm/skills/converting-oracle-artifacts/scripts/convert_fmb.py samples/simple_module.fmb --compile
   ```
3. **Launch the live form via FSAL**:
   ```bash
   java -jar /path/to/frmsal.jar -url "http://localhost:9001/forms/frmservlet?config=standaloneapp&form=simple_module.fmx&userid=scott/tiger@oracle-db-23ai:1521/FREEPDB1"
   ```
This launches the native desktop Java client window directly connected to the server and database.

## Workbench parity notes

Workbench supports Forms Builder-like capabilities in a modern UI: property palette, layout edit mode, wizard, menu module attach, PL/SQL split view, and Reports preview/property sheet. Not full Builder parity.
