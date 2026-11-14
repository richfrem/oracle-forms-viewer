# Installing and running the Rust workbench (App 1)

The Rust workbench is a web app, `oracle-forms-server`, on port 7070. It needs **no Oracle software, no JVM, and no Node**. It is separate from the Oracle stack (Podman containers); it calls the stack only for **Preview in Oracle**. Read `architecture.md` "The two apps and how they fit together" first.

| | macOS (and Linux) | Windows 11 |
|---|---|---|
| Where it runs | On the host | **Inside the Ubuntu WSL2 distro** (the same one that runs Podman). The browser stays on Windows. Do **not** build it natively on Windows: that route (Visual Studio and C++) was a red herring; see [windows-podman-setup.md](windows-podman-setup.md) section 12 |
| Install Rust | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` | The same command, in Ubuntu: `... \| sh -s -- -y --profile minimal` |
| Extra packages | A C compiler (Xcode command line tools) | `sudo apt-get update && sudo apt-get install -y build-essential` (the TLS dependency `aws-lc-sys` compiles C) |
| Build output | `target/` in the repo | `export CARGO_TARGET_DIR=$HOME/ofw-target` (Ubuntu's disk; the Windows drive, `/mnt/c`, is slow) |
| Verified | Steps from `HOW-TO-RUN.md`; **not re-run in the Windows sessions** | `rustc` and `cargo` 1.100.0, gcc 13.3; `cargo check` and `cargo test` (120 passed); the app serving and Preview in Oracle working |

Minimum Rust: 1.75 (`DEPENDENCIES.md`); 1.80 or newer recommended (`HOW-TO-RUN.md`). Rust 1.100 prints 8 manifest lint warnings (a redundant `homepage` and an unused `tracing` dependency in some `Cargo.toml` files); they are not code warnings.

## Run it

From the repo root (Windows: in the Ubuntu shell, after `. "$HOME/.cargo/env"` and the `CARGO_TARGET_DIR` export):

```bash
cargo run -p oracle-forms-server          # prints "Listening on http://127.0.0.1:7070"
```

Open `http://127.0.0.1:7070`. `Ctrl + C` stops it. On Windows also `export XDG_RUNTIME_DIR=/run/user/$(id -u)` so Preview in Oracle can reach Podman.

To start the containers and the app together, and to confirm each one, use `python3 start_testbed.py` (Windows: from the Ubuntu shell). It builds with the right settings, starts the app in the background (log `temp/oracle-forms-server.log`, cargo output `temp/cargo-build.log`), never stops a running app unless `--restart-rust`, and ends with a summary table. `python3 start_testbed.py --status` is read-only.

## Settings

| Variable | Default | Effect |
|---|---|---|
| `PORT` | `7070` | Listening port (`PORT=8080 cargo run -p oracle-forms-server`) |
| `OFW_HOST` | `127.0.0.1` | Listening address. A non-loopback value (for example `0.0.0.0`) exposes the server to your network and prints a warning |
| `OFW_ALLOWED_HOSTS` | none | Extra comma-separated Host names to accept (needed with a non-loopback `OFW_HOST`) |
| `OFW_SERVER_ENV_CREDENTIALS` | off | When `1`, database requests from the UI that carry no credentials use `OFW_DB_USER` and `OFW_DB_PASSWORD`. `start_testbed.py --db-credentials` sets it and loads `.env` |

By default the server listens on `127.0.0.1` only, answers only to `localhost`, `127.0.0.1`, and `[::1]`, and refuses API calls from other websites. There are no built-in database credentials, and the server never falls back to mock data: a missing database or credentials is an error. To query Oracle from the UI, set `OFW_DB_USER` and `OFW_DB_PASSWORD` (in `.env`) and start with `--db-credentials`, or enter them in the UI's login dialog.

## Check it

```bash
cargo check && cargo test                 # the repo gate: all tests pass, no code warnings
cargo run -p oracle-forms-cli -- inspect samples/orders_fmb.xml                           # CLI without the web app
cargo run -p oracle-forms-cli -- query samples/test_db_fmb.xml --backend oracle-thin     # live query; needs OFW_DB_USER / OFW_DB_PASSWORD
```

Port 7070 busy (`Address already in use`): find the process with `lsof -i :7070`, stop it, and restart. Never kill the owner's running app without being asked.

## Preview in Oracle needs the containers

Preview in Oracle sends the saved form XML to `run_form.py`, which converts and compiles it in throwaway containers and opens the form through the Forms servlet on port 7001 with FSAL. The full list of what must be true first is in [windows-podman-status-log.md](windows-podman-status-log.md) and [forms-and-reports-testing-guide.md](forms-and-reports-testing-guide.md); the readiness API `GET http://127.0.0.1:7070/api/forms/oracle-preview/readiness` reports the same checks. Do not use **Save & launch in Oracle** to test: each click rewrites the form's XML file.

## Background run and credentials from `.env`

To run the app in the background in WSL: `setsid nohup cargo run -p oracle-forms-server > temp/oracle-forms-server.wsl.log 2>&1 &` (or let `python3 start_testbed.py` do it). To let the UI use the credentials from `.env`: `set -a; source .env; set +a` first, then add `OFW_SERVER_ENV_CREDENTIALS=1` before `cargo run`. Run it from the repo root: from another folder the samples and UI files are not found. Do not run two copies; both use port 7070.

## How the Oracle stack feeds the workbench

- **Certified XML:** `convert_fmb.py` produces `*_fmb.xml` files that `oracle-forms-xml` reads and `oracle-forms-server` shows on port 7070.
- **PL/SQL text:** `convert_pll.py` produces `.pld` files the native PLD parser reads.
- **Sibling discovery:** opening a `.fmb`, the server detects a companion `<stem>_fmb.xml` and renders the canonical layout.
- **Visual ground truth:** running a form with `run_form.py` (FSAL) next to the same form in the workbench shows whether the native SVG canvas reproduces Oracle's coordinate scaling and widget layout.
- **Version differences:** comparing 12c Forms2XML exports with 14c exports isolates Oracle syntax deprecations and additions (the 12c and 14c schemas, `validate_forms_xml.py --version`, are the tools for this once the 12c schema is wired).
