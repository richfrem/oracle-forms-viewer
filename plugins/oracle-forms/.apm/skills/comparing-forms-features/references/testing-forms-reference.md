# Testing Reference: Oracle Forms (14c FSAL vs Native Rust)

Comprehensive operational guide for verifying and testing Oracle Forms modules across both Oracle Fusion Middleware 14c and the native pure-Rust Oracle Forms Workbench.

---

## 1. Environment & Prerequisites

Ensure the reference containers are active:
```bash
podman ps --filter "name=oracle23ai-lite" --filter "name=forms-server-14c"
```
If not running:
```bash
podman start oracle23ai-lite forms-server-14c
```

Database connection details:
- **Host / Port**: `localhost:1521` (Host) or `10.88.0.93:1521` (Container network)
- **Service / PDB**: `FREEPDB1`
- **Developer User**: `forms_user` / `Welcome123#`

---

## 2. Testing on Oracle Fusion Middleware 14c (Desktop FSAL)

### Step 1: Seed Schema in Oracle 23ai
```bash
podman exec -i oracle23ai-lite sqlplus -s forms_user/Welcome123#@localhost:1521/FREEPDB1 < samples/ofw_demo_employees.sql
podman exec -i oracle23ai-lite sqlplus -s forms_user/Welcome123#@localhost:1521/FREEPDB1 < samples/ofw_demo_sales.sql
```

### Step 2: Headless Compilation (.xml -> .fmb -> .fmx)
Convert XML to `.fmb` using containerized `XML2Forms`:
```bash
podman exec forms-server-14c /bin/bash -c "
  export ORACLE_HOME=/oracle_home_14c
  export LD_LIBRARY_PATH=\$ORACLE_HOME/lib:\$LD_LIBRARY_PATH
  export FORMS_PATH=/work:\$FORMS_PATH
  export FORMS_API_TK_BYPASS=TRUE
  CLASSPATH=\"\$ORACLE_HOME/jlib/frmjdapi.jar:\$ORACLE_HOME/jlib/frmxmltools.jar:\$ORACLE_HOME/oracle_common/modules/oracle.xdk/xmlparserv2.jar\"
  java -Djava.library.path=\"\$ORACLE_HOME/lib\" -classpath \"\$CLASSPATH\" oracle.forms.util.xmltools.XML2Forms OVERWRITE=YES /work/samples/ofw_demo_employees_fmb.xml
"
```

Compile `.fmb` to executable `.fmx` via `frmcmp_batch`:
```bash
python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/compile_form.py samples/ofw_demo_employees.fmb
```

Stage generated `.fmx` into WebLogic Forms home:
```bash
cp samples/ofw_demo_employees.fmx oracle-reference-stack/oracle_home_14c/forms/
```

### Step 3: Launch Live Forms on Desktop (FSAL)
Run the native Forms Standalone Launcher desktop client:
```bash
# Employee Directory:
java -jar oracle-reference-stack/oracle_home_14c/forms/java/frmsal.jar \
  -url "http://localhost:7001/forms/frmservlet?config=standaloneapp&form=ofw_demo_employees.fmx"

# Sales Order Entry:
java -jar oracle-reference-stack/oracle_home_14c/forms/java/frmsal.jar \
  -url "http://localhost:7001/forms/frmservlet?config=standaloneapp&form=ofw_sales.fmx"
```
Logon prompt: `forms_user` / `Welcome123#` @ `10.88.0.93:1521/FREEPDB1`

---

## 3. Testing on Native Pure-Rust Oracle Forms Workbench

### Method A: Browser UI (Interactive Live Session)
1. Launch the server with live database credentials:
   ```bash
   OFW_SERVER_ENV_CREDENTIALS=1 OFW_DB_USER=forms_user OFW_DB_PASSWORD=Welcome123# cargo run -p oracle-forms-server
   ```
2. Open in browser: `http://127.0.0.1:7070`
3. Select the target form in the **Sample** dropdown:
   - `ofw_demo_employees_fmb.xml` (or `.fmb`)
   - `ofw_sales_fmb.xml` (or `.fmb`)
4. Click **▶ Run Form**:
   - Spawns an interactive session via `/api/sessions`.
   - Executes `WHEN-NEW-FORM-INSTANCE` trigger.
   - Automatically queries initial block over `oracle-thin` wire protocol.
5. Interactive testing checklist:
   - **Navigation**: Click `< Previous` and `Next >` buttons to step through records.
   - **POST-QUERY calculation**: Confirm calculated fields (e.g. `Full name`) update dynamically.
   - **Master-Detail Filtering**: In `ofw_sales`, select different customer rows and observe orders filtering in the detail grid.
   - **Search / Criteria**: Enter filter text (e.g. `smith`) and click `Search`.

### Method B: Standalone Headless CLI
Inspect module structure, blocks, items, and triggers:
```bash
cargo run -p oracle-forms-cli -- inspect samples/ofw_demo_employees_fmb.xml
```

Execute direct database query over pure-Rust wire protocol:
```bash
OFW_DB_USER=forms_user OFW_DB_PASSWORD=Welcome123# cargo run -p oracle-forms-cli -- query samples/ofw_demo_employees_fmb.xml --block EMP --backend oracle-thin
```

---

## 4. Parity Verification Checklist

| Verification Item | What to Look For | Expected Outcome |
| :--- | :--- | :--- |
| **Record Data Parity** | Column values in row 1 | Exact match (`Omar Smith`, ID `250` or `Linda Adams`, ID `281`) |
| **Calculated Items** | `POST-QUERY` trigger | Formula concatenation mirrors across both runtimes |
| **Query Latency** | Timing comparison | 14c FSAL: 3–5s startup; Rust: 1–5 ms direct wire latency |
| **Grid / Table Layout** | Tab canvas & multi-record blocks | Multi-row customer grid and order lines align to coordinates |
| **Validation Triggers** | `WHEN-VALIDATE-ITEM` | Field validation prevents invalid commits on both runtimes |
