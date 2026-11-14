# Testing both engines, and how they compare

Detail behind the `testing-forms-parity` skill: how to open the same form in Oracle Fusion Middleware 14c and in the native Rust workbench, and what differs.

## Contents

- [Comparison matrix](#comparison-matrix)
- [Engine A: Fusion Middleware 14c (desktop FSAL)](#engine-a-fusion-middleware-14c-desktop-fsal)
- [Engine B: native Rust workbench (browser)](#engine-b-native-rust-workbench-browser)

## Comparison matrix

| Capability | Oracle Fusion Middleware 14c | Native Rust Oracle Forms Workbench |
| :--- | :--- | :--- |
| **Runtime Architecture** | Java EE / WebLogic AdminServer 14.1.2 | Standalone native Rust binary (`oracle-forms-server`) |
| **Client Requirement** | JRE + Forms Standalone Launcher (`frmsal.jar`) | Zero-install web browser (`http://127.0.0.1:7070`) |
| **Startup / Query Latency** | ~3 to 5 seconds (JVM + applet init) | 2 to 10 ms direct async TCP queries |
| **Client Protocol** | Oracle Forms socket protocol | HTTP/REST + JSON canvas stream |
| **Database Connectivity** | Oracle Net / OCI client libraries | Pure-Rust `oracle-thin` (async Tokio TCP wire protocol) |
| **Dependency Footprint** | ~5 GB container / Oracle Home | Single ~25 MB native compiled binary |

## Engine A: Fusion Middleware 14c (desktop FSAL)

Needs the testbed running (`python3 start_testbed.py --status`) and a host Java 17 or 21 (WSL's OpenJDK 21 on Windows). Run the Forms Standalone Launcher against the servlet on port 7001:

```bash
java -jar oracle-reference-stack/oracle_home_14c/forms/java/frmsal.jar \
  -url "http://localhost:7001/forms/frmservlet?config=standaloneapp&form=ofw_demo_employees.fmx"
java -jar oracle-reference-stack/oracle_home_14c/forms/java/frmsal.jar \
  -url "http://localhost:7001/forms/frmservlet?config=standaloneapp&form=ofw_sales.fmx"
```

Or use `compiling-and-running-oracle-forms/scripts/launch_fsal.py <form>.fmx --interactive`, which builds the URL from `.env`. If the Oracle Home is not in the repo folder (`OFW_ORACLE_HOME`), use that path to `frmsal.jar`.

**Logon:** `DB_USER` and `DB_PASSWORD` from `.env`, service `localhost:1521/FREEPDB1` from the host (the server container uses the `FREEPDB1` alias in its `tnsnames.ora`). Do not use a container's numeric IP address.

## Engine B: native Rust workbench (browser)

1. Start it: `cargo run -p oracle-forms-server` (or `python3 start_testbed.py`).
2. Open `http://127.0.0.1:7070`.
3. Select `ofw_demo_employees_fmb.xml` or `ofw_sales_fmb.xml` from the module selector.
4. Click **Run Form** for a live session with real database queries, block navigation, and commit processing.
