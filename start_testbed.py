#!/usr/bin/env python3
"""
start_testbed.py: start the Oracle testbed containers and the Rust workbench server.

Purpose:
    One command to bring the whole stack back after a reboot or `wsl --shutdown`:
      1. Make sure Podman can run (WSL: the /run/user/<uid> runtime directory; macOS: the Podman machine).
      2. Start the database container and wait until it is ready.
      3. Start the Forms 14c (Fusion Middleware / WebLogic) server container. It is slow to boot:
         15 to 45 minutes to reach RUNNING, so it starts while the next step runs.
      4. Build and start the Rust server in the background, unless it is already running. A running
         server is never stopped unless you pass --restart-rust.
      5. Wait for the Forms server to reach RUNNING and for its servlet to answer on port 7001.
      6. With --launch FORM, open that deployed form in the Forms Standalone Launcher (FSAL) with the
         native logon dialog.
      7. Print the status.

Where to run it:
    - Windows 11 (recommended): from the Ubuntu WSL shell, with `python3 start_testbed.py`. The containers,
      Python, Java 21, and the Rust app all run inside WSL; the Rust app is built there with
      CARGO_TARGET_DIR=~/ofw-target (WSL's own disk, much faster than /mnt/c). No Visual Studio needed.
    - Windows PowerShell: `python start_testbed.py` runs Podman through `wsl.exe` and builds the Rust app
      as a native Windows program (alternative; needs Visual Studio, see docs/DEPENDENCIES.md 3.1).
    - macOS and Linux: `python3 start_testbed.py`.

Usage:
    python3 start_testbed.py                 # database + Forms server + Rust server (everything)
    python3 start_testbed.py --no-forms      # database + Rust server only (fast)
    python3 start_testbed.py --no-wait-forms # start everything but do not wait for the Forms server
    python3 start_testbed.py --no-rust       # containers only
    python3 start_testbed.py --no-containers # Rust server only
    python3 start_testbed.py --restart-rust  # stop, rebuild, and restart the Rust server even if running
    python3 start_testbed.py --launch ofw_demo_employees.fmx   # open a deployed form in the Forms client (FSAL)
    python3 start_testbed.py --db-credentials  # let the UI use OFW_DB_USER / OFW_DB_PASSWORD from .env
    python3 start_testbed.py --stop-rust     # stop the Rust server and exit
    python3 start_testbed.py --status        # show what is running and exit

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
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Dict, List, Optional

REPO = Path(__file__).resolve().parent
IS_WINDOWS = platform.system() == "Windows"
# Running inside WSL: Podman is local here, but the Rust app is built with the Windows toolchain.
IS_WSL = platform.system() == "Linux" and "microsoft" in platform.uname().release.lower()
DB_CONTAINER = "oracle23ai-lite"
FORMS_CONTAINER = "forms-server-14c"
NETWORK = "oracle-net"
RUST_PORT = 7070
# Inside WSL the build output lives on WSL's own disk (much faster than the Windows drive).
WSL_TARGET_DIR = Path.home() / "ofw-target"
if IS_WSL:
    RUST_BIN = WSL_TARGET_DIR / "debug" / "oracle-forms-server"
else:
    RUST_BIN = REPO / "target" / "debug" / ("oracle-forms-server.exe" if IS_WINDOWS else "oracle-forms-server")
RUST_LOG = REPO / "temp" / "oracle-forms-server.log"
FSAL_LAUNCHER = REPO / "plugins" / "oracle-forms" / ".apm" / "skills" / "compiling-and-running-oracle-forms" / "scripts" / "launch_fsal.py"


# ----------------------------------------------------------------------------- helpers

def say(message: str) -> None:
    print(message, flush=True)


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


def wait_for_log(name: str, needle: str, distro: str, timeout: int) -> bool:
    """Poll the container log until the needle appears or the timeout (seconds) passes."""
    deadline = time.time() + timeout
    while time.time() < deadline:
        if needle in output(podman(["logs", "--tail", "400", name], distro)):
            return True
        time.sleep(10)
    return False


def http_status(url: str) -> int:
    try:
        with urllib.request.urlopen(url, timeout=3) as response:
            return response.status
    except urllib.error.HTTPError as error:
        return error.code
    except Exception:
        return 0


# ----------------------------------------------------------------------------- containers

def ensure_podman_runtime(distro: str) -> bool:
    """Windows/WSL: recreate /run/user/<uid>. macOS: start the Podman machine if needed."""
    if IS_WINDOWS or IS_WSL:
        check = run_shell("test -d /run/user/$(id -u)/containers/networks", distro)
        if check.returncode == 0:
            return True
        say("Recreating the Podman runtime directory in WSL (sudo will ask for your WSL password)...")
        fix = (
            "sudo mkdir -p /run/user/$(id -u) && sudo chown $(id -un): /run/user/$(id -u) "
            "&& chmod 700 /run/user/$(id -u) && mkdir -p /run/user/$(id -u)/containers/networks"
        )
        if run_shell(fix, distro, interactive=True).returncode != 0:
            say("ERROR: could not create the runtime directory. Run the sudo commands from HOW-TO-RUN.md section 6 by hand.")
            return False
        return True
    if podman(["info"], distro).returncode != 0:
        say("Starting the Podman machine...")
        subprocess.run(["podman", "machine", "start"])
    return podman(["info"], distro).returncode == 0


def start_containers(with_forms: bool, distro: str, db_timeout: int) -> bool:
    if not ensure_podman_runtime(distro):
        return False
    if podman(["network", "exists", NETWORK], distro).returncode != 0:
        say(f"Creating the {NETWORK} network...")
        podman(["network", "create", NETWORK], distro)

    if not container_exists(DB_CONTAINER, distro):
        say(f"ERROR: container {DB_CONTAINER} does not exist. Create it with the setup skill first.")
        return False
    say(f"Starting {DB_CONTAINER}...")
    podman(["start", DB_CONTAINER], distro)
    say("Waiting for the database (log line 'DATABASE IS READY TO USE')...")
    if wait_for_log(DB_CONTAINER, "DATABASE IS READY TO USE", distro, db_timeout):
        say("  database ready")
    else:
        say(f"  WARNING: not ready after {db_timeout}s. Check: podman logs {DB_CONTAINER}")
        return False

    if with_forms:
        if not container_exists(FORMS_CONTAINER, distro):
            say(f"WARNING: container {FORMS_CONTAINER} does not exist; skipping. Create it with the setup skill.")
            return True
        say(f"Starting {FORMS_CONTAINER} (WebLogic takes 15 to 45 minutes to reach RUNNING)...")
        podman(["start", FORMS_CONTAINER], distro)
    return True


FORMS_SERVLET_URL = "http://127.0.0.1:7001/forms/frmservlet"


def servlet_up() -> bool:
    return http_status(FORMS_SERVLET_URL) in (200, 301, 302)


def wait_for_forms(distro: str, timeout: int) -> bool:
    """Wait for WebLogic to log RUNNING, then for the Forms servlet to answer on port 7001.

    The servlet is what the workbench's Preview in Oracle dialog and FSAL actually need, so the log line alone is not enough.
    """
    if not container_exists(FORMS_CONTAINER, distro):
        return False
    deadline = time.time() + timeout
    say(f"Waiting for the Forms server (up to {timeout // 60} min; log line 'Server state changed to RUNNING')...")
    if not wait_for_log(FORMS_CONTAINER, "Server state changed to RUNNING", distro, timeout):
        say(f"  WARNING: not RUNNING after {timeout}s. Check: podman logs {FORMS_CONTAINER}")
        return False
    say("  WebLogic RUNNING. Waiting for the Forms servlet on port 7001...")
    while time.time() < deadline + 120:  # allow two extra minutes for the web app to finish deploying
        if servlet_up():
            say(f"  Forms servlet up: {FORMS_SERVLET_URL}")
            return True
        time.sleep(5)
    say(f"  WARNING: the Forms servlet did not answer on port 7001. Check: curl -I {FORMS_SERVLET_URL}")
    return False

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


def process_running(name: str) -> bool:
    """True when a process with this image name is running (Windows, WSL, or Unix)."""
    if IS_WINDOWS or IS_WSL:
        exe = name if name.lower().endswith(".exe") else name + ".exe"
        listing = subprocess.run(["tasklist.exe" if IS_WSL else "tasklist", "/FI", f"IMAGENAME eq {exe}"], capture_output=True, text=True)
        return exe.lower() in (listing.stdout or "").lower()
    return subprocess.run(["pgrep", "-f", name], capture_output=True).returncode == 0


def start_rust(db_credentials: bool, build_timeout: int, restart: bool = False) -> bool:
    if not restart and http_status(f"http://127.0.0.1:{RUST_PORT}/") == 200:
        say(f"  Rust server already running at http://127.0.0.1:{RUST_PORT}/ (left as is; use --restart-rust to rebuild and restart it)")
        return True
    cargo = shutil.which("cargo") or (str(Path.home() / ".cargo" / "bin" / "cargo") if (Path.home() / ".cargo" / "bin" / "cargo").is_file() else None)
    if IS_WSL and not cargo:
        # No Rust inside WSL: fall back to the Windows build (needs Visual Studio; see docs/DEPENDENCIES.md).
        extra = ["--rust-timeout", str(build_timeout)] + (["--db-credentials"] if db_credentials else [])
        return run_on_windows(extra + (["--restart-rust"] if restart else [])) == 0
    if not cargo:
        say("ERROR: cargo (Rust) was not found. Install Rust from https://rustup.rs and rerun, or use --no-rust.")
        return False
    if not restart and (process_running(RUST_BIN.name) or process_running("cargo")):
        say("  A Rust server or build is already starting. Waiting for it; it is not being stopped...")
        deadline = time.time() + build_timeout
        while time.time() < deadline:
            if http_status(f"http://127.0.0.1:{RUST_PORT}/") == 200:
                say(f"  Rust server up: http://127.0.0.1:{RUST_PORT}/")
                return True
            time.sleep(2)
        say(f"  WARNING: it did not answer on port {RUST_PORT} yet. Nothing was stopped.")
        return False
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
        stop_rust()
    say("Building the Rust server (cargo build -p oracle-forms-server)...")
    build = subprocess.run([cargo, "build", "-p", "oracle-forms-server"], cwd=REPO, env=env)
    if build.returncode != 0 or not RUST_BIN.is_file():
        say("ERROR: cargo build failed. See docs/DEPENDENCIES.md section 3.1 for the Windows toolchain.")
        return False

    RUST_LOG.parent.mkdir(parents=True, exist_ok=True)
    log = open(RUST_LOG, "ab")
    kwargs = {}
    if IS_WINDOWS:
        kwargs["creationflags"] = 0x00000008 | 0x00000200  # DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
    else:
        kwargs["start_new_session"] = True
    subprocess.Popen([str(RUST_BIN)], cwd=REPO, env=env, stdout=log, stderr=subprocess.STDOUT, **kwargs)

    deadline = time.time() + build_timeout
    while time.time() < deadline:
        if http_status(f"http://127.0.0.1:{RUST_PORT}/") == 200:
            say(f"  Rust server up: http://127.0.0.1:{RUST_PORT}/")
            return True
        time.sleep(1)
    say(f"  WARNING: the Rust server did not answer on port {RUST_PORT}. See {RUST_LOG}")
    return False


# ----------------------------------------------------------------------------- FSAL launch

def launch_form(form: str) -> bool:
    """Open a deployed form in the Forms Standalone Launcher (FSAL), with the native logon dialog."""
    name = Path(form).name
    if not name.lower().endswith(".fmx"):
        say("ERROR: --launch takes a deployed form file name such as ofw_demo_employees.fmx")
        return False
    if not servlet_up():
        say(f"ERROR: the Forms servlet is not answering on port 7001 ({FORMS_SERVLET_URL}). Run: python start_testbed.py")
        return False
    if not FSAL_LAUNCHER.is_file():
        say(f"ERROR: launcher script not found: {FSAL_LAUNCHER}")
        return False
    kwargs = {}
    if IS_WINDOWS:
        kwargs["creationflags"] = 0x00000008 | 0x00000200  # DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
    else:
        kwargs["start_new_session"] = True
    subprocess.Popen([sys.executable, str(FSAL_LAUNCHER), name, "--interactive"], cwd=REPO, **kwargs)
    say(f"Launching {name} in the Forms desktop client. The logon dialog opens in its own window.")
    return True


# ----------------------------------------------------------------------------- status and main

def show_status(distro: str) -> None:
    say("Containers:")
    for name in (DB_CONTAINER, FORMS_CONTAINER):
        state = container_state(name, distro) if (not (IS_WINDOWS or IS_WSL) or run_shell("test -d /run/user/$(id -u)", distro).returncode == 0) else "Podman runtime directory missing (reboot or wsl --shutdown)"
        say(f"  {name}: {state}")
    say(f"Rust server (port {RUST_PORT}): " + ("up" if http_status(f"http://127.0.0.1:{RUST_PORT}/") == 200 else "down"))
    say("Forms servlet (port 7001): " + ("up" if servlet_up() else "down"))


def main() -> int:
    parser = argparse.ArgumentParser(description="Start the Oracle testbed containers and the Rust workbench server")
    parser.add_argument("--no-forms", action="store_true", help="skip the Forms 14c server container (database and Rust server only)")
    parser.add_argument("--no-wait-forms", action="store_true", help="start the Forms server but do not wait for it to reach RUNNING")
    parser.add_argument("--no-rust", action="store_true", help="do not start the Rust server")
    parser.add_argument("--no-containers", action="store_true", help="do not touch the containers")
    parser.add_argument("--db-credentials", action="store_true", help="start the Rust server with OFW_SERVER_ENV_CREDENTIALS=1 (UI uses OFW_DB_USER/OFW_DB_PASSWORD from .env)")
    parser.add_argument("--launch", metavar="FORM.fmx", help="open this deployed form in the Forms desktop client (FSAL) once the Forms servlet is up")
    parser.add_argument("--restart-rust", action="store_true", help="stop, rebuild, and restart the Rust server even if it is already running")
    parser.add_argument("--stop-rust", action="store_true", help="stop the Rust server and exit")
    parser.add_argument("--status", action="store_true", help="show what is running and exit")
    parser.add_argument("--distro", default="Ubuntu", help="WSL distribution that runs Podman (Windows only; default: Ubuntu)")
    parser.add_argument("--db-timeout", type=int, default=300, help="seconds to wait for the database (default: 300)")
    parser.add_argument("--forms-timeout", type=int, default=1800, help="seconds to wait for the Forms server (default: 1800)")
    parser.add_argument("--rust-timeout", type=int, default=120, help="seconds to wait for the Rust server (default: 120)")
    args = parser.parse_args()

    if args.status:
        show_status(args.distro)
        return 0
    if args.stop_rust:
        if IS_WSL:
            return run_on_windows(["--stop-rust"])
        stop_rust()
        say("Rust server stopped.")
        return 0

    with_forms = not args.no_forms
    ok = True
    if not args.no_containers:
        ok = start_containers(with_forms, args.distro, args.db_timeout)
    if not args.no_rust:
        ok = start_rust(args.db_credentials, args.rust_timeout, args.restart_rust) and ok
    if with_forms and not args.no_containers and not args.no_wait_forms:
        ok = wait_for_forms(args.distro, args.forms_timeout) and ok
    if args.launch:
        ok = launch_form(args.launch) and ok
    say("")
    show_status(args.distro)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
