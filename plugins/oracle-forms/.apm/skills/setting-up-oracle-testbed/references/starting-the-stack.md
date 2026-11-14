# Starting, restarting, and checking the stack: `start_testbed.py`

One command brings up everything and confirms it. It is the same on macOS and Windows; on Windows run it from the **Ubuntu WSL shell** with `python3` (not PowerShell). It never creates or replaces containers: it starts the existing ones (`oracle23ai-lite`, `forms-server-14c`) and the Rust app.

## Contents

- [What it does](#what-it-does)
- [Options](#options)
- [Reading the output](#reading-the-output)
- [After a reboot or wsl --shutdown](#after-a-reboot-or-wsl---shutdown)
- [Idempotent and safe](#idempotent-and-safe)

## What it does

In order, each step recorded as one line and one row in the summary:

1. **Podman.** Windows/WSL: recreates `/run/user/<uid>` if it is missing (needs `sudo`, so it prompts for your WSL password in the terminal; an AI agent cannot type it, so the user runs the script). macOS: starts the Podman machine if it is stopped.
2. **Database** (`oracle23ai-lite`). Confirmed by three things: the container is running, the log line `DATABASE IS READY TO USE` was written **since this container started**, and port 1521 accepts connections.
3. **Forms** (`forms-server-14c`). Started, then confirmed by: running, the log line `Server state changed to RUNNING` since this start, and the Forms servlet answering on port 7001. WebLogic takes 15 to 45 minutes to boot on Windows (Oracle Home on the Windows drive); about 2 minutes on a Mac.
4. **Rust app** (port 7070). Built and started in the background if it is not already running. Confirmed by the home page and the Oracle preview readiness API. Cargo's output goes to `temp/cargo-build.log` (the terminal shows a warning count); the app's log is `temp/oracle-forms-server.log`.
5. Optionally opens a deployed form in FSAL (`--launch`).

Only log lines written since the container's current start count, so a "READY" or "RUNNING" line left from an earlier boot is never mistaken for the current one. Every output line has a clock time and the time elapsed since the start; each result line says how long that component took.

## Options

| Command | Effect |
|---|---|
| `python3 start_testbed.py` | Everything, confirmed |
| `--status` | Read-only check; changes nothing; exit 0 only if everything is up |
| `--no-forms` | Database and Rust app only (fast); Forms shows SKIP |
| `--no-wait-forms` | Start the Forms container but do not wait; Forms shows STARTED (not confirmed) |
| `--no-rust` / `--no-containers` | Containers only / Rust app only |
| `--restart-rust` / `--stop-rust` | Rebuild and restart, or stop, the Rust app (it is never stopped otherwise) |
| --stop-forms | Stop the orms-server-14c container (graceful, 60 seconds) and exit. It never kills processes. If the container will not stop, it prints the manual steps (find the PID with podman inspect, check its children with ps; ending them is your decision) |
| `--db-credentials` | Let the UI use `OFW_DB_USER` and `OFW_DB_PASSWORD` from `.env` |
| `--launch <form>.fmx` | Open a deployed form in FSAL once the servlet is up |
| `--db-timeout`, `--forms-timeout`, `--rust-timeout` | Seconds to wait (defaults 300, 1800, 120) |

## Reading the output

The summary has one row per component with a status, the clock time it finished, how long it took, and a detail line.

| Status | Meaning |
|---|---|
| OK | Started or already running, and every check passed |
| STARTED | Started but not confirmed (`--no-wait-forms`, or FSAL launched) |
| SKIP | Not requested (a flag) or already running and left alone; the detail says which |
| WARN | Something is running but a check did not pass yet |
| FAIL | A check failed; the detail and the log path say why |

The exit code is 0 only when nothing failed. `Rust app: readiness API returned 0` means the readiness call timed out (it probes the containers and can be slow while WebLogic boots), not that the app is down.

## After a reboot or wsl --shutdown

Podman's runtime directory `/run/user/<uid>` is gone, and `--status` reports "Podman runtime directory missing". Run `python3 start_testbed.py`: it recreates the directory and asks for the sudo password once.

## Idempotent and safe

Run it again at any time. Anything already running and healthy is left alone and reported as such; a running Rust app is never stopped unless `--restart-rust`. It never removes containers or images.

Not verified: the confirmation checks rely on `podman logs --since` and on reading the container's start time with `podman inspect`; if either is unavailable the script falls back to searching the last 400 log lines.
