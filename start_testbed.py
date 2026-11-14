#!/usr/bin/env python3
"""
start_testbed.py: start the Oracle testbed containers and the Rust workbench server, and confirm each one.

Purpose:
    One command to bring the whole stack up (or back after a reboot or `wsl --shutdown`) and to verify it.
    It is idempotent: run it again at any time. Anything already running and healthy is left alone, and the
    output says what was skipped and why. Each component gets one result line and one row in the summary:

      1. Podman:   Windows/WSL: recreate the /run/user/<uid> runtime directory; macOS: start the Podman machine.
      2. Database: container oracle23ai-lite. Confirmed by: container running, the log line
                   "DATABASE IS READY TO USE" written since this container started, and port 1521 open.
      3. Forms:    container forms-server-14c (Fusion Middleware / WebLogic). Confirmed by: container running,
                   the log line "Server state changed to RUNNING" since this container started, and the Forms
                   servlet answering on port 7001. It takes 15 to 45 minutes to boot.
      4. Rust app: the workbench web server on port 7070. Built and started in the background unless already
                   running (it is never stopped unless you pass --restart-rust). Confirmed by: the home page and
                   the Oracle preview readiness API answering.
      5. Optionally (--launch FORM) open a deployed form in the Forms Standalone Launcher (FSAL).
      6. Print the summary: OK / STARTED / SKIP / WARN / FAIL per component. Exit code 0 only when every
         component that was requested is OK.

    Only log lines written since the container's current start count, so a "READY" or "RUNNING" line left over
    from an earlier boot is never mistaken for the current one.

Where to run it:
    - Windows 11 (recommended): from the Ubuntu WSL shell, with `python3 start_testbed.py`. The containers,
      Python, Java 21, and the Rust app all run inside WSL; the Rust app is built there with
      CARGO_TARGET_DIR=~/ofw-target (WSL's own disk, much faster than /mnt/c). No Visual Studio needed.
    - Windows PowerShell: `python start_testbed.py` runs Podman through `wsl.exe` and builds the Rust app
      as a native Windows program (a red herring: needs Visual Studio and was much worse than WSL; prefer
      the WSL shell above; see DEPENDENCIES.md 3.1).
    - macOS and Linux: `python3 start_testbed.py`.

Usage (what each flag skips is reported as SKIP in the summary):
    python3 start_testbed.py                 # database + Forms server + Rust server, all confirmed
    python3 start_testbed.py --no-forms      # database + Rust server only (fast); Forms is SKIP
    python3 start_testbed.py --no-wait-forms # start the Forms container but do not wait; Forms is STARTED, not confirmed
    python3 start_testbed.py --no-rust       # containers only; Rust is SKIP
    python3 start_testbed.py --no-containers # Rust server only; database and Forms are SKIP
    python3 start_testbed.py --restart-rust  # stop, rebuild, and restart the Rust server even if running
    python3 start_testbed.py --launch ofw_demo_employees.fmx   # open a deployed form in the Forms client (FSAL)
    python3 start_testbed.py --db-credentials  # let the UI use OFW_DB_USER / OFW_DB_PASSWORD from .env
    python3 start_testbed.py --stop-rust     # stop the Rust server and exit
    python3 start_testbed.py --stop-forms    # stop the Forms container (graceful, 60 s) and exit; never kills processes
    python3 start_testbed.py --status        # read-only check of everything, then exit

Notes:
    - Containers must already exist (created by the setup skill). This script starts them; it does not
      create or replace them.
    - Inside WSL, recreating /run/user/<uid> needs sudo and will prompt for the WSL password in this
      terminal. From Windows PowerShell the WSL distro is --distro (default: Ubuntu).
    - Credentials come from the git-ignored .env at the repo root. Values are never printed.
    - Rust server log: temp/oracle-forms-server.log (temp/ is git-ignored).
"""
import argparse
import glob
import os
import platform
import shutil
import shlex
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request
from datetime import datetime
from pathlib import Path
from typing import Dict, List, Optional, Tuple

REPO = Path(__file__).resolve().parent
IS_WINDOWS = platform.system() == "Windows"
# Running inside WSL: Podman is local, but the Rust app is built with the Windows toolchain.
IS_WSL = platform.system() == "Linux" and "microsoft" in platform.uname().release.lower()
DB_CONTAINER = "oracle23ai-lite"
FORMS_CONTAINER = "forms-server-14c"
NETWORK = "oracle-net"
DB_PORT = 1521
RUST_PORT = 7070
DB_READY_LINE = "DATABASE IS READY TO USE"
FORMS_RUNNING_LINE = "Server state changed to RUNNING"
# Inside WSL the build output lives on WSL's own disk (much faster than the Windows drive).
WSL_TARGET_DIR = Path.home() / "ofw-target"
if IS_WSL:
    RUST_BIN = WSL_TARGET_DIR / "debug" / "oracle-forms-server"
else:
    RUST_BIN = REPO / "target" / "debug" / ("oracle-forms-server.exe" if IS_WINDOWS else "oracle-forms-server")
RUST_LOG = REPO / "temp" / "oracle-forms-server.log"
FSAL_LAUNCHER = REPO / "plugins" / "oracle-forms" / ".apm" / "skills" / "compiling-and-running-oracle-forms" / "scripts" / "launch_fsal.py"
FORMS_SERVLET_URL = "http://127.0.0.1:7001/forms/frmservlet"
RUST_URL = f"http://127.0.0.1:{RUST_PORT}/"
RUST_READINESS_URL = f"http://127.0.0.1:{RUST_PORT}/api/forms/oracle-preview/readiness"

START_TIME = time.time()
STARTED_AT = datetime.now()
# One row per component for the summary: (component, status, detail, seconds taken or None, clock time finished).
RESULTS: List[Tuple[str, str, str, Optional[float], str]] = []
# When each component's work began (set by begin()), so its result can report how long it took.
TIMERS: Dict[str, float] = {}


# ----------------------------------------------------------------------------- output

def fmt_duration(seconds: float) -> str:
    """Human-readable duration: 7s, 1m 05s, 1h 02m 03s."""
    total = int(round(seconds))
    hours, rest = divmod(total, 3600)
    minutes, secs = divmod(rest, 60)
    if hours:
        return f"{hours}h {minutes:02d}m {secs:02d}s"
    if minutes:
        return f"{minutes}m {secs:02d}s"
    return f"{secs}s"


def say(message: str) -> None:
    """Print a line with the wall-clock time and the time elapsed since the script started."""
    print(f"[{datetime.now():%H:%M:%S} +{fmt_duration(time.time() - START_TIME):>8}] {message}", flush=True)


def begin(component: str) -> None:
    """Mark when a component's work starts, so record() can report how long it took."""
    TIMERS[component] = time.time()


def record(component: str, status: str, detail: str) -> bool:
    """Print and remember one component result with how long it took. Statuses: OK, STARTED, SKIP, WARN, FAIL.
    Returns True unless FAIL."""
    took = time.time() - TIMERS[component] if component in TIMERS else None
    RESULTS.append((component, status, detail, took, f"{datetime.now():%H:%M:%S}"))
    suffix = f" (took {fmt_duration(took)})" if took is not None and status in ("OK", "FAIL", "WARN", "STARTED") else ""
    say(f"{status:<7} {component}: {detail}{suffix}")
    return status != "FAIL"


def print_summary() -> int:
    finished = datetime.now()
    print("\n" + "=" * 78)
    print(" Testbed summary")
    print(f" Started  {STARTED_AT:%Y-%m-%d %H:%M:%S}   Finished {finished:%Y-%m-%d %H:%M:%S}   Total {fmt_duration(time.time() - START_TIME)}")
    print("=" * 78)
    print(f" {'Status':<8} {'Component':<10} {'Done at':<9} {'Took':>9}  Detail")
    for component, status, detail, took, clock in RESULTS:
        took_text = fmt_duration(took) if took is not None and status != "SKIP" else "-"
        print(f" {status:<8} {component:<10} {clock:<9} {took_text:>9}  {detail}")
    failed = [r for r in RESULTS if r[1] == "FAIL"]
    unconfirmed = [r for r in RESULTS if r[1] in ("STARTED", "WARN")]
    print("-" * 78)
    if failed:
        print(f" {len(failed)} component(s) FAILED.")
    elif unconfirmed:
        print(f" No failures, but {len(unconfirmed)} component(s) are not confirmed (STARTED or WARN).")
    else:
        print(" Everything requested is up and confirmed.")
    return 1 if failed else 0


# ----------------------------------------------------------------------------- helpers

def read_dotenv() -> Dict[str, str]:
    """Read KEY=VALUE lines from .env at the repo root. Missing file means no values."""
    values: Dict[str, str] = {}
    path = REPO / ".env"
    if path.is_file():
        for raw in path.read_text(encoding="utf-8").splitlines():
            line = raw.strip()
            if line and not line.startswith("#") and "=" in line:
                key, value = line.split("=", 1)
                values[key.strip()] = value.strip().strip('"').strip("'")
    return values


def run_shell(command: str, distro: str, interactive: bool = False, timeout: Optional[int] = None) -> subprocess.CompletedProcess:
    """Run a shell command where Podman lives: inside WSL from Windows, locally in WSL, locally elsewhere."""
    if IS_WINDOWS:
        full = ["wsl", "-d", distro, "--", "bash", "-lc", "export XDG_RUNTIME_DIR=/run/user/$(id -u); " + command]
    elif IS_WSL:
        full = ["bash", "-lc", "export XDG_RUNTIME_DIR=/run/user/$(id -u); " + command]
    else:
        full = ["bash", "-lc", command]
    if interactive:
        return subprocess.run(full)
    return subprocess.run(full, capture_output=True, text=True, timeout=timeout)


def podman(args: List[str], distro: str, timeout: Optional[int] = None) -> subprocess.CompletedProcess:
    return run_shell("podman " + " ".join(shlex.quote(a) for a in args), distro, timeout=timeout)


def output(result: subprocess.CompletedProcess) -> str:
    return (result.stdout or "") + (result.stderr or "")


def container_exists(name: str, distro: str) -> bool:
    return podman(["container", "exists", name], distro).returncode == 0


def container_state(name: str, distro: str) -> str:
    result = podman(["ps", "-a", "--filter", f"name=^{name}$", "--format", "{{.Status}}"], distro)
    return (result.stdout or "").strip() or "not found"


def container_running(name: str, distro: str) -> bool:
    result = podman(["inspect", "-f", "{{.State.Running}}", name], distro)
    return (result.stdout or "").strip() == "true"


def container_started_unix(name: str, distro: str) -> Optional[int]:
    """Unix time of the container's current start, or None if it cannot be read."""
    result = podman(["inspect", "-f", "{{.State.StartedAt.Unix}}", name], distro)
    text = (result.stdout or "").strip()
    return int(text) if result.returncode == 0 and text.isdigit() and int(text) > 0 else None


def log_has_line(name: str, needle: str, distro: str, since: Optional[int]) -> bool:
    """True when the container log has the needle. With `since`, only lines written since that Unix time count,
    so a line left over from an earlier boot is ignored. Without it, the last 400 lines are searched."""
    args = ["logs", "--since", str(since), name] if since else ["logs", "--tail", "400", name]
    result = podman(args, distro)
    if result.returncode != 0 and since:
        result = podman(["logs", "--tail", "400", name], distro)  # `--since` not supported: fall back
    return needle in output(result)


def wait_for_log(name: str, needle: str, distro: str, timeout: int, since: Optional[int], label: str) -> bool:
    """Poll the container log until the needle appears or the timeout (seconds) passes, with progress lines."""
    began = time.time()
    next_note = began + 30
    while time.time() - began < timeout:
        if log_has_line(name, needle, distro, since):
            return True
        if time.time() >= next_note:
            say(f"        still waiting for {label} ({int(time.time() - began)}s of {timeout}s)")
            next_note += 30
        time.sleep(5)
    return False


def http_status(url: str) -> int:
    try:
        with urllib.request.urlopen(url, timeout=3) as response:
            return response.status
    except urllib.error.HTTPError as error:
        return error.code
    except Exception:
        return 0


def tcp_open(host: str, port: int) -> bool:
    try:
        with socket.create_connection((host, port), timeout=2):
            return True
    except OSError:
        return False


def servlet_up() -> bool:
    return http_status(FORMS_SERVLET_URL) in (200, 301, 302)


# ----------------------------------------------------------------------------- Podman and containers

def ensure_podman_runtime(distro: str) -> bool:
    """Windows/WSL: recreate /run/user/<uid>. macOS: start the Podman machine if needed."""
    begin("Podman")
    if IS_WINDOWS or IS_WSL:
        check = run_shell("test -d /run/user/$(id -u)/containers/networks", distro)
        if check.returncode == 0:
            return record("Podman", "OK", "runtime directory /run/user/<uid> present (nothing to do)")
        say("Recreating the Podman runtime directory in WSL (sudo will ask for your WSL password)...")
        fix = (
            "sudo mkdir -p /run/user/$(id -u) && sudo chown $(id -un): /run/user/$(id -u) "
            "&& chmod 700 /run/user/$(id -u) && mkdir -p /run/user/$(id -u)/containers/networks"
        )
        if run_shell(fix, distro, interactive=True).returncode != 0:
            return record("Podman", "FAIL", "could not create the runtime directory; run the sudo commands from HOW-TO-RUN.md section 6 by hand")
        return record("Podman", "OK", "runtime directory recreated")
    if podman(["info"], distro).returncode != 0:
        say("Starting the Podman machine...")
        subprocess.run(["podman", "machine", "start"])
    if podman(["info"], distro).returncode == 0:
        return record("Podman", "OK", "Podman is responding")
    return record("Podman", "FAIL", "Podman is not responding (macOS: run `podman machine start`)")


def ensure_network(distro: str) -> None:
    if podman(["network", "exists", NETWORK], distro).returncode != 0:
        say(f"Creating the {NETWORK} network...")
        podman(["network", "create", NETWORK], distro)
    else:
        say(f"        network {NETWORK} already exists (nothing to do)")


def start_database(distro: str, timeout: int) -> bool:
    """Start oracle23ai-lite if needed and confirm it: running, ready line since this start, port 1521 open."""
    begin("Database")
    if not container_exists(DB_CONTAINER, distro):
        return record("Database", "FAIL", f"container {DB_CONTAINER} does not exist; create it with the setup skill first")
    if container_running(DB_CONTAINER, distro):
        say(f"        {DB_CONTAINER} is already running (not restarted)")
    else:
        say(f"Starting {DB_CONTAINER} (state before: {container_state(DB_CONTAINER, distro)})...")
        started = podman(["start", DB_CONTAINER], distro)
        if started.returncode != 0:
            return record("Database", "FAIL", f"podman start failed: {output(started).strip()[:200]}")
    since = container_started_unix(DB_CONTAINER, distro)
    say(f"Waiting for the database (log line '{DB_READY_LINE}' since this start, then port {DB_PORT})...")
    if not wait_for_log(DB_CONTAINER, DB_READY_LINE, distro, timeout, since, "the database"):
        return record("Database", "FAIL", f"not ready after {timeout}s; check: podman logs {DB_CONTAINER}")
    if not tcp_open("127.0.0.1", DB_PORT):
        time.sleep(3)
    if not tcp_open("127.0.0.1", DB_PORT):
        return record("Database", "FAIL", f"log says ready but port {DB_PORT} is not accepting connections")
    return record("Database", "OK", f"container up, '{DB_READY_LINE}' logged since start, port {DB_PORT} open")


def start_forms_container(distro: str) -> bool:
    """Start forms-server-14c if needed. Confirmation is separate (confirm_forms) because boot is slow.
    The Forms timer starts here, so its "took" is the real time from starting the container to confirmed."""
    begin("Forms")
    if not container_exists(FORMS_CONTAINER, distro):
        return record("Forms", "FAIL", f"container {FORMS_CONTAINER} does not exist; create it with the setup skill first")
    if container_running(FORMS_CONTAINER, distro):
        say(f"        {FORMS_CONTAINER} is already running (not restarted)")
        return True
    say(f"Starting {FORMS_CONTAINER} (state before: {container_state(FORMS_CONTAINER, distro)}; WebLogic takes 15 to 45 minutes to reach RUNNING)...")
    started = podman(["start", FORMS_CONTAINER], distro)
    if started.returncode != 0:
        return record("Forms", "FAIL", f"podman start failed: {output(started).strip()[:200]}")
    return True


def confirm_forms(distro: str, timeout: int) -> bool:
    """Confirm the Forms server: container running, RUNNING line since this start, servlet answering on 7001."""
    if not container_running(FORMS_CONTAINER, distro):
        return record("Forms", "FAIL", f"container {FORMS_CONTAINER} is not running; check: podman logs {FORMS_CONTAINER}")
    since = container_started_unix(FORMS_CONTAINER, distro)
    if servlet_up() and log_has_line(FORMS_CONTAINER, FORMS_RUNNING_LINE, distro, since):
        return record("Forms", "OK", "already RUNNING and the Forms servlet answers on port 7001 (nothing to wait for)")
    say(f"Waiting for the Forms server (up to {timeout // 60} min; log line '{FORMS_RUNNING_LINE}' since this start)...")
    if not wait_for_log(FORMS_CONTAINER, FORMS_RUNNING_LINE, distro, timeout, since, "WebLogic"):
        return record("Forms", "FAIL", f"not RUNNING after {timeout}s; check: podman logs {FORMS_CONTAINER}")
    say("        WebLogic RUNNING. Waiting for the Forms servlet on port 7001...")
    deadline = time.time() + 120  # the web application needs a little longer to finish deploying
    while time.time() < deadline:
        if servlet_up():
            return record("Forms", "OK", f"RUNNING and the Forms servlet answers: {FORMS_SERVLET_URL}")
        time.sleep(5)
    return record("Forms", "FAIL", f"RUNNING, but the servlet did not answer on port 7001; check: curl -I {FORMS_SERVLET_URL}")


# ----------------------------------------------------------------------------- Rust server

def windows_build_env() -> Dict[str, str]:
    """Find the MSVC and Windows SDK folders so cargo can link and compile C code. Empty if not found."""
    if os.environ.get("LIB") and os.environ.get("INCLUDE"):
        return {}
    msvc = sorted(glob.glob(r"C:\Program Files\Microsoft Visual Studio\*\*\VC\Tools\MSVC\*"))
    sdk_root = Path(r"C:\Program Files (x86)\Windows Kits\10")
    sdk_versions = sorted(
        p.name for p in (sdk_root / "Lib").glob("10.*")
        if (p / "ucrt" / "x64").is_dir() and (sdk_root / "Include" / p.name / "ucrt").is_dir()
    ) if (sdk_root / "Lib").is_dir() else []
    if not msvc or not sdk_versions:
        say("WARNING: Visual Studio or the Windows SDK was not found; run this from a Developer Command Prompt if the build fails.")
        return {}
    vct, ver = Path(msvc[-1]), sdk_versions[-1]
    lib = [vct / "lib" / "onecore" / "x64", sdk_root / "Lib" / ver / "ucrt" / "x64", sdk_root / "Lib" / ver / "um" / "x64"]
    inc = [vct / "include", sdk_root / "Include" / ver / "ucrt", sdk_root / "Include" / ver / "um", sdk_root / "Include" / ver / "shared"]
    return {"LIB": ";".join(map(str, lib)), "INCLUDE": ";".join(map(str, inc))}


def run_on_windows(extra_args: List[str]) -> int:
    """From WSL, run this script's Rust steps with Windows Python, which has the Windows toolchain."""
    python_exe = shutil.which("python.exe") or shutil.which("py.exe")
    if not python_exe:
        say("ERROR: Windows Python (python.exe) is not reachable from WSL. Run this script from PowerShell instead.")
        return 1
    say("WSL detected: running the Rust step with Windows Python...")
    # WSL starts Windows programs directly and maps the /mnt/c working directory to its Windows path.
    return subprocess.run([python_exe, "start_testbed.py", "--no-containers", *extra_args], cwd=REPO).returncode


def stop_rust() -> None:
    if IS_WINDOWS:
        subprocess.run(["taskkill", "/IM", RUST_BIN.name, "/F"], capture_output=True)
    else:
        subprocess.run(["pkill", "-f", "oracle-forms-server"], capture_output=True)
    time.sleep(1)


def stop_forms(distro: str, grace_seconds: int = 60) -> bool:
    """Ask Podman to stop forms-server-14c and wait. Never kills processes itself.
    If the container will not stop, prints the manual steps and returns False."""
    if not container_exists(FORMS_CONTAINER, distro):
        say(f"{FORMS_CONTAINER} does not exist. Nothing to stop.")
        return True
    if not container_running(FORMS_CONTAINER, distro):
        say(f"{FORMS_CONTAINER} is not running. Nothing to stop.")
        return True
    say(f"Stopping {FORMS_CONTAINER} (waiting up to {grace_seconds} seconds)...")
    podman(["stop", "-t", str(grace_seconds)], distro, timeout=grace_seconds + 60)
    if container_running(FORMS_CONTAINER, distro):
        say(f"{FORMS_CONTAINER} did not stop: Podman could not end its main process.")
        say("Manual steps (nothing is deleted by these):")
        say(f"  1. Find its process:  podman inspect -f '{{{{.State.Pid}}}}' {FORMS_CONTAINER}")
        say("  2. Check its state:   ps -o pid,stat,etime,cmd --ppid <that pid>")
        say("  3. Ending those processes with kill -9 is your decision; this script does not do it.")
        return False
    say(f"{FORMS_CONTAINER} stopped. Its Oracle Home is a mounted folder; nothing in it was deleted.")
    return True


def find_cargo() -> Optional[str]:
    """Path to cargo (on PATH, or in ~/.cargo/bin, which a non-login shell may not have on PATH)."""
    found = shutil.which("cargo")
    if found:
        return found
    candidate = Path.home() / ".cargo" / "bin" / "cargo"
    return str(candidate) if candidate.is_file() else None


def process_running(name: str) -> bool:
    """True when a process with this image name is running (Windows, WSL, or Unix)."""
    if IS_WINDOWS or IS_WSL:
        exe = name if name.lower().endswith(".exe") else name + ".exe"
        listing = subprocess.run(["tasklist.exe" if IS_WSL else "tasklist", "/FI", f"IMAGENAME eq {exe}"], capture_output=True, text=True)
        return exe.lower() in (listing.stdout or "").lower()
    return subprocess.run(["pgrep", "-f", name], capture_output=True).returncode == 0


def rust_confirmed() -> Optional[str]:
    """Confirmation detail when the Rust app answers (home page and readiness API), else None."""
    if http_status(RUST_URL) != 200:
        return None
    readiness = http_status(RUST_READINESS_URL)
    return f"home page answers on port {RUST_PORT}; readiness API {'answers' if readiness == 200 else f'returned {readiness}'}"


def start_rust(db_credentials: bool, build_timeout: int, restart: bool = False) -> bool:
    """Start the Rust app unless it is already running, and confirm it. Never stops a running app unless restart."""
    begin("Rust app")
    if not restart:
        detail = rust_confirmed()
        if detail:
            return record("Rust app", "SKIP", f"already running, left as is ({detail}); use --restart-rust to rebuild and restart")
    cargo = find_cargo()
    if IS_WSL and not cargo:
        # No Rust inside WSL: fall back to the Windows build. That route needs Visual Studio and was a red
        # herring; install Rust in WSL instead (see DEPENDENCIES.md 3.1).
        extra = ["--rust-timeout", str(build_timeout)] + (["--db-credentials"] if db_credentials else [])
        ok = run_on_windows(extra + (["--restart-rust"] if restart else [])) == 0
        return record("Rust app", "OK" if ok else "FAIL", "started through the Windows build (cargo was not found in WSL; install Rust in WSL instead)")
    if not cargo:
        return record("Rust app", "FAIL", "cargo (Rust) was not found. Install Rust from https://rustup.rs and rerun, or use --no-rust")
    if not restart and (process_running(RUST_BIN.name) or process_running("cargo")):
        say("        A Rust server or build is already starting. Waiting for it; it is not being stopped...")
        deadline = time.time() + build_timeout
        while time.time() < deadline:
            detail = rust_confirmed()
            if detail:
                return record("Rust app", "OK", f"was already starting; now up ({detail})")
            time.sleep(2)
        return record("Rust app", "WARN", f"a build or server is running but did not answer on port {RUST_PORT} yet; nothing was stopped")
    env = os.environ.copy()
    env.update(read_dotenv())
    if IS_WSL:
        env["CARGO_TARGET_DIR"] = str(WSL_TARGET_DIR)
        env["XDG_RUNTIME_DIR"] = f"/run/user/{os.getuid()}"
    if IS_WINDOWS:
        env.update(windows_build_env())
    if db_credentials:
        env["OFW_SERVER_ENV_CREDENTIALS"] = "1"

    if restart:
        say("Stopping the running Rust server (--restart-rust)...")
        stop_rust()
    build_log = REPO / "temp" / "cargo-build.log"
    build_log.parent.mkdir(parents=True, exist_ok=True)
    say(f"Building the Rust server (cargo build -p oracle-forms-server); the first build takes a few minutes. Full cargo output: {build_log}")
    with open(build_log, "w", encoding="utf-8") as out:
        build = subprocess.run([cargo, "build", "-p", "oracle-forms-server", "--color", "never"],
                               cwd=REPO, env=env, stdout=out, stderr=subprocess.STDOUT)
    build_text = build_log.read_text(encoding="utf-8", errors="replace")
    warning_count = sum(1 for line in build_text.splitlines() if line.startswith("warning: ") and " generated " not in line)
    say(f"        build finished: exit {build.returncode}, {warning_count} warning(s) (details in the log above)")
    if build.returncode != 0 or not RUST_BIN.is_file():
        for line in build_text.splitlines()[-30:]:
            print("    " + line, flush=True)
        return record("Rust app", "FAIL", f"cargo build failed (last lines above; full log: {build_log}); see DEPENDENCIES.md section 3.1")

    RUST_LOG.parent.mkdir(parents=True, exist_ok=True)
    log = open(RUST_LOG, "ab")
    kwargs = {}
    if IS_WINDOWS:
        kwargs["creationflags"] = 0x00000008 | 0x00000200  # DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
    else:
        kwargs["start_new_session"] = True
    say(f"Starting {RUST_BIN.name} in the background (log: {RUST_LOG})...")
    subprocess.Popen([str(RUST_BIN)], cwd=REPO, env=env, stdout=log, stderr=subprocess.STDOUT, **kwargs)

    deadline = time.time() + build_timeout
    while time.time() < deadline:
        detail = rust_confirmed()
        if detail:
            return record("Rust app", "OK", f"built and started ({detail})")
        time.sleep(1)
    return record("Rust app", "FAIL", f"did not answer on port {RUST_PORT}; see {RUST_LOG}")


# ----------------------------------------------------------------------------- FSAL launch

def launch_form(form: str) -> bool:
    """Open a deployed form in the Forms Standalone Launcher (FSAL), with the native logon dialog."""
    begin("FSAL")
    name = Path(form).name
    if not name.lower().endswith(".fmx"):
        return record("FSAL", "FAIL", "--launch takes a deployed form file name such as ofw_demo_employees.fmx")
    if not servlet_up():
        return record("FSAL", "FAIL", f"the Forms servlet is not answering on port 7001 ({FORMS_SERVLET_URL}); run: python3 start_testbed.py")
    if not FSAL_LAUNCHER.is_file():
        return record("FSAL", "FAIL", f"launcher script not found: {FSAL_LAUNCHER}")
    kwargs = {}
    if IS_WINDOWS:
        kwargs["creationflags"] = 0x00000008 | 0x00000200  # DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
    else:
        kwargs["start_new_session"] = True
    subprocess.Popen([sys.executable, str(FSAL_LAUNCHER), name, "--interactive"], cwd=REPO, **kwargs)
    return record("FSAL", "STARTED", f"launching {name}; the logon dialog opens in its own window")


# ----------------------------------------------------------------------------- status and main

def show_status(distro: str) -> int:
    """Read-only check of everything; nothing is started or stopped. Exit 0 when all four checks are up."""
    runtime_ok = True
    if IS_WINDOWS or IS_WSL:
        runtime_ok = run_shell("test -d /run/user/$(id -u)", distro).returncode == 0
    say("Containers:")
    up = 0
    for name in (DB_CONTAINER, FORMS_CONTAINER):
        if not runtime_ok:
            say(f"  {name}: Podman runtime directory missing (reboot or wsl --shutdown); run python3 start_testbed.py")
            continue
        state = container_state(name, distro)
        running = container_running(name, distro) if state != "not found" else False
        up += 1 if running else 0
        say(f"  {name}: {state}")
    db_port = tcp_open("127.0.0.1", DB_PORT)
    rust = rust_confirmed()
    servlet = servlet_up()
    say(f"Database port {DB_PORT}: " + ("open" if db_port else "closed"))
    say(f"Rust server (port {RUST_PORT}): " + (f"up ({rust})" if rust else "down"))
    say("Forms servlet (port 7001): " + ("up" if servlet else "down"))
    return 0 if (up == 2 and db_port and rust and servlet) else 1


def main() -> int:
    parser = argparse.ArgumentParser(description="Start the Oracle testbed containers and the Rust workbench server, and confirm each one")
    parser.add_argument("--no-forms", action="store_true", help="skip the Forms 14c server container (database and Rust server only)")
    parser.add_argument("--no-wait-forms", action="store_true", help="start the Forms server but do not wait for it to reach RUNNING (reported as STARTED, not confirmed)")
    parser.add_argument("--no-rust", action="store_true", help="do not start the Rust server")
    parser.add_argument("--no-containers", action="store_true", help="do not touch the containers")
    parser.add_argument("--db-credentials", action="store_true", help="start the Rust server with OFW_SERVER_ENV_CREDENTIALS=1 (UI uses OFW_DB_USER/OFW_DB_PASSWORD from .env)")
    parser.add_argument("--launch", metavar="FORM.fmx", help="open this deployed form in the Forms desktop client (FSAL) once the Forms servlet is up")
    parser.add_argument("--restart-rust", action="store_true", help="stop, rebuild, and restart the Rust server even if it is already running")
    parser.add_argument("--stop-rust", action="store_true", help="stop the Rust server and exit")
    parser.add_argument("--stop-forms", action="store_true", help="stop the forms-server-14c container (graceful, 60 seconds) and exit; reports manual steps if it will not stop")
    parser.add_argument("--status", action="store_true", help="read-only check of everything, then exit")
    parser.add_argument("--distro", default="Ubuntu", help="WSL distribution that runs Podman (Windows only; default: Ubuntu)")
    parser.add_argument("--db-timeout", type=int, default=300, help="seconds to wait for the database (default: 300)")
    parser.add_argument("--forms-timeout", type=int, default=1800, help="seconds to wait for the Forms server (default: 1800)")
    parser.add_argument("--rust-timeout", type=int, default=120, help="seconds to wait for the Rust server (default: 120)")
    args = parser.parse_args()

    if args.status:
        return show_status(args.distro)
    if args.stop_rust:
        if IS_WSL and not find_cargo():
            # No Rust in WSL: the app, if any, is the Windows build.
            return run_on_windows(["--stop-rust"])
        stop_rust()
        say("Rust server stopped.")
        return 0
    if args.stop_forms:
        return 0 if stop_forms(args.distro) else 1

    with_forms = not args.no_forms
    say(f"Starting the testbed at {STARTED_AT:%Y-%m-%d %H:%M:%S} (idempotent: anything already running and healthy is left alone).")
    ok = True

    # 1. Podman and the containers.
    if args.no_containers:
        record("Podman", "SKIP", "not checked (--no-containers)")
        record("Database", "SKIP", "not checked (--no-containers)")
        record("Forms", "SKIP", "not checked (--no-containers)")
    else:
        podman_ok = ensure_podman_runtime(args.distro)
        ok = podman_ok and ok
        forms_started = False
        if podman_ok:
            ensure_network(args.distro)
            ok = start_database(args.distro, args.db_timeout) and ok
            if with_forms:
                forms_started = start_forms_container(args.distro)
                ok = forms_started and ok
        else:
            record("Database", "FAIL", "skipped because Podman is not usable")
            if with_forms:
                record("Forms", "FAIL", "skipped because Podman is not usable")
        if not with_forms:
            record("Forms", "SKIP", "not started (--no-forms)")

    # 2. The Rust app (starts while WebLogic boots).
    if args.no_rust:
        record("Rust app", "SKIP", "not started (--no-rust)")
    else:
        ok = start_rust(args.db_credentials, args.rust_timeout, args.restart_rust) and ok

    # 3. Confirm the Forms server (slow), unless skipped or told not to wait.
    if with_forms and not args.no_containers and RESULTS and not any(row[0] == "Forms" for row in RESULTS):
        if args.no_wait_forms:
            record("Forms", "STARTED", "container started, WebLogic is booting (15 to 45 minutes); not confirmed (--no-wait-forms). Check later: python3 start_testbed.py --status")
        else:
            ok = confirm_forms(args.distro, args.forms_timeout) and ok

    if args.launch:
        ok = launch_form(args.launch) and ok
    code = print_summary()
    return code if code else (0 if ok else 1)


if __name__ == "__main__":
    sys.exit(main())
