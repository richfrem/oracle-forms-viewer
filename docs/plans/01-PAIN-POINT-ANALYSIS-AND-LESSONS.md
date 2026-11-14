# 01: Pain Point Analysis & Architectural Lessons from REA

---

## 1. The Anatomy of Oracle Middleware Pain

During the implementation of Track 1 (Forms 14c + WebLogic container testbed), we documented every friction point, linker failure, and configuration bottleneck required simply to compile and run a single test form with a database query:

### 1.1 The Installation & Storage Tax
- **Massive Storage Footprint**: Oracle Fusion Middleware 14c requires two stages: FMW Infrastructure (~2.5 GB installer) and Forms/Reports 14.1.2 (~2.8 GB installer). Unpacked and installed with WebLogic and JDK 17, the disk footprint exceeds **12 to 15 GB** for an installation whose actual execution engine is a few dozen megabytes.
- **Silent Installer Brittleness**: Silent response files (`forms14c_silent.rsp`) fail silently or abort with generic return codes if inventory directories (`oraInst.loc`) or group permissions (`oinstall`) are not perfectly aligned.

### 1.2 The Native C Linker & Shared Library Nightmare
- **Unlinked Shared Object Stubs**: Oracle ships Linux binaries with missing runtime links. When attempting to launch `frmweb`, the process immediately crashed with:
  ```text
  frmweb: error while loading shared libraries: libiplsn.so.0: cannot open shared object file
  ```
  Resolving this required installing `gcc` and `binutils` inside the container and running `/oracle_home_14c/bin/genshlib` for all **16 proprietary shared libraries**.
- **Undocumented WebLogic JVM Environment Properties (`FRM-93500`)**:
  Starting a form session initially threw:
  ```text
  FRM-93500: Failed to create forms session ...
  Caused by: java.lang.NullPointerException: Cannot invoke "java.util.Map.entrySet()" because "pEnv" is null
      at java.base/java.lang.ProcessBuilder$ProcessEnvironment.mergeEnv(ProcessBuilder.java:1208)
  ```
  The Oracle Forms servlet failed internally because the Forms Instance name was missing from WebLogic's environment. The fix required hacking `-Dforms.instance=forms1` into `setUserOverrides.sh`.

### 1.3 Obsolete Terminal & Resource Configurations
- **Terminal Resource Faults**: The forms listener threw `Error opening Oracle*Terminal file fmrweb`. Forms requires an archaic terminal resource file from the 1980s (`fmrweb.res`), requiring manual symlinks (`instances/forms1 -> ../forms1`) and rigid `ORACLE_PATH` environment routing.
- **Port Confusion**: WebLogic binds AdminServer to port 7001, but documents running forms on Managed Server `WLS_FORMS` port 9001. In standalone/development containers, managed servers abort with EOF errors unless nodemanager is orchestrated with complex Jython scripts.

### 1.4 Client Runtime Friction (Java FSAL & Applets)
- **Host Java Requirement**: Even with the container running, the developer must have Java installed on the client machine to launch `frmsal.jar`.
- **URL Encoding Fragment Bugs**: Passing database credentials like `userid=$DB_USER/$DB_PASSWORD@FREEPDB1` via URL causes Java to truncate the URL at `#` when the password contains one (URL-encode it as `%23`) (interpreting it as an HTTP fragment identifier), resulting in `FRM-40007: Invalid user identifier or password`.
- **Aesthetic Obsolescence**: The Java client renders with Swing/AWT components designed for 1024x768 CRT monitors from 1998, failing high-DPI modern displays and lacking responsive accessibility.

---

## 2. What Oracle Forms Actually Is Under the Hood

Stripped of its historical middleware baggage, an Oracle Form is simply:
```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ 1. Metadata Schema (AST)                                                    │
│    - Coordinates, Canvases, Windows, Blocks, Items, Triggers                │
├─────────────────────────────────────────────────────────────────────────────┤
│ 2. In-Memory Data Buffer (Multi-Record Manager)                             │
│    - Cursor pagination, row fetching, dirty-tracking (NEW / QUERY / CHANGED) │
├─────────────────────────────────────────────────────────────────────────────┤
│ 3. Database Gateway                                                         │
│    - Automatic SQL SELECT, INSERT, UPDATE, DELETE based on block metadata   │
├─────────────────────────────────────────────────────────────────────────────┤
│ 4. Navigation & Validation State Machine                                    │
│    - Tab traversal order, field formatting, required check, commit cycle     │
├─────────────────────────────────────────────────────────────────────────────┤
│ 5. Event Dispatcher & Built-ins                                             │
│    - EXECUTE_QUERY, NEXT_RECORD, GO_BLOCK, MESSAGE, SHOW_ALERT               │
└─────────────────────────────────────────────────────────────────────────────┘
```
**Every single one of these responsibilities can be implemented in a pure-native Rust binary of less than 25 MB that boots in 15 milliseconds, uses 18 MB of RAM, and runs directly in any modern browser!**

---

## 3. Architectural Lessons from REA (`temp/repos/rea`)

The `temp/repos/rea` repository provides powerful architectural patterns that directly inform our pure-Rust Forms runtime redesign:

### 3.1 Domain-Driven Design & Invariant Isolation (`src/domain/`)
- **Lesson**: REA strictly separates pure domain entities (which have zero dependencies on network, disk, or UI) from application orchestrators and external adapters.
- **Application to OF-Next**:
  - `oracle-forms-core` remains a pure, immutable domain AST model (definitions of blocks, items, geometries, triggers).
  - `oracle-forms-runtime` houses the mutable runtime session entities (Record Buffer, Cursor, Navigation Focus, Validation State) completely decoupled from HTTP, WebSockets, or database drivers.

### 3.2 Strongly-Typed Schema Contracts (`src/contracts/`)
- **Lesson**: REA uses explicit DTO contracts between the analysis core and external consumers (CLI, MCP, web visualizers).
- **Application to OF-Next**:
  - Define serializable Session DTOs (`RecordSetDto`, `FormEventDto`, `LovResultsDto`, `ValidationResultDto`).
  - The Axum web server and browser communicate over a typed JSON protocol (via REST or WebSocket), enabling either a web UI, a headless test runner, or an MCP agent tool to drive the exact same runtime engine.

### 3.3 Pluggable Adapter Pattern (`src/providers/` & `src/native/`)
- **Lesson**: REA isolates tool-specific bridges (Ghidra, Hopper, Cutter, Frida) behind unified abstract traits.
- **Application to OF-Next**:
  - Define a generic `DatabaseProvider` trait:
    ```rust
    #[async_trait]
    pub trait DatabaseProvider: Send + Sync {
        async fn execute_query(&self, sql: &str, binds: &[BindValue]) -> Result<RecordSet, DbError>;
        async fn insert_record(&self, table: &str, values: &[ColumnValue]) -> Result<RowId, DbError>;
        async fn update_record(&self, table: &str, row_id: &RowId, values: &[ColumnValue]) -> Result<(), DbError>;
        async fn delete_record(&self, table: &str, row_id: &RowId) -> Result<(), DbError>;
        async fn commit(&self) -> Result<(), DbError>;
        async fn rollback(&self) -> Result<(), DbError>;
    }
    ```
  - Implementations:
    1. `OracleWireProvider`: Native SQL*Net / TNS connection to Oracle Database 23ai/19c/12c.
    2. `PostgresProvider`: Connecting to PostgreSQL for migrated architectures.
    3. `SqliteMockProvider`: Offline local development and instant automated test execution.

### 3.4 Session & Context Isolation
- **Lesson**: REA encapsulates all state inside isolated session containers with clear lifecycles, timeouts, and resource budgets.
- **Application to OF-Next**:
  - Multi-user web viewer: Each browser tab instantiates an isolated `FormSession`.
  - Transaction isolation: Each session maintains its own database transaction, undo buffer, and dirty record set, mirroring Oracle Forms' authentic transactional model without cross-session pollution.
