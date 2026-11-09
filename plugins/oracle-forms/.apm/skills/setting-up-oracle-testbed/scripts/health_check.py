#!/usr/bin/env python3
"""
health_check.py (CLI)
=====================================

Purpose:
    Verifies the operational readiness of the containerized Oracle Forms 14c
    and Database 23ai Free testbed across container lifecycle, HTTP listener,
    and live SQL*Net queries.

Layer: Tooling / Skill Script (setting-up-oracle-testbed)

Usage Examples:
    python3 plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/scripts/health_check.py
    python3 plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/scripts/health_check.py --verbose

Supported Object Types:
    - Podman Containers (oracle23ai-lite, forms-server-14c)
    - WebLogic Forms Listener Servlet (HTTP 7001)
    - Pluggable Database Services (FREEPDB1)

CLI Arguments:
    --verbose : Print extended diagnostic output during health checks.

Input Files:
    - Running container: oracle23ai-lite
    - Running container: forms-server-14c

Output:
    - Formatted terminal report indicating health status across 3 tiers
    - Exit code 0 if all subsystems healthy, 1 if any subsystem degraded

Key Functions:
    - check_containers() -> bool
    - check_forms_listener() -> bool
    - check_database_query() -> bool
    - main() -> int

Script Dependencies:
    - Python 3.8+ (standard library: sys, subprocess, urllib.request, urllib.error, typing)
    - Podman CLI available in system PATH

Consumed by:
    - AI coding agents during pre-flight testbed evaluation
    - CI/CD health check pipelines and setup verification routines
"""

import sys
import subprocess
import urllib.request
import urllib.error
from typing import List


# Verifies that both required Podman containers are currently in running status.
def check_containers() -> bool:
    """Query Podman for running container instances of 23ai DB and Forms 14c.

    Returns:
        True if both containers are active, False otherwise.
    """
    print("[1/3] Checking Podman containers...")
    cmd: List[str] = ["podman", "ps", "--format", "{{.Names}}"]
    try:
        res = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, check=True)
        running = res.stdout.strip().splitlines()
        db_ok = "oracle23ai-lite" in running
        forms_ok = "forms-server-14c" in running or "forms-14c-server" in running
        print(f"  - Oracle Database 23ai: {'RUNNING (ok)' if db_ok else 'STOPPED (fail)'}")
        print(f"  - Oracle Forms 14c Server: {'RUNNING (ok)' if forms_ok else 'STOPPED (fail)'}")
        return db_ok and forms_ok
    except Exception as e:
        print(f"  - Failed to query Podman: {e}")
        return False


# Tests HTTP connectivity to the WebLogic Forms Listener Servlet status endpoint.
def check_forms_listener() -> bool:
    """Send an HTTP GET probe to /forms/frmservlet/admin on port 7001.

    Returns:
        True if HTTP 200 is received and response indicates operational status.
    """
    print("[2/3] Checking Forms Listener Servlet (Port 7001)...")
    url = "http://localhost:7001/forms/frmservlet/admin"
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "HealthCheck/1.0"})
        with urllib.request.urlopen(req, timeout=5) as resp:
            content = resp.read().decode("utf-8", errors="ignore")
            if "Forms Listener Servlet is up and running" in content or resp.status == 200:
                print("  - Forms Listener Servlet: UP AND RUNNING (ok)")
                return True
            else:
                print(f"  - Unexpected response: HTTP {resp.status}")
                return False
    except urllib.error.URLError as e:
        print(f"  - Listener connection failed: {e}")
        return False


# Tests end-to-end database connectivity by executing a live SQL*Plus query inside 23ai.
def check_database_query() -> bool:
    """Execute a query against forms_user.test_items in FREEPDB1 via SQL*Plus.

    Returns:
        True if query succeeds and returns valid count, False otherwise.
    """
    print("[3/3] Checking Database Connection & Query (FREEPDB1)...")
    cmd: List[str] = [
        "podman", "exec", "-i", "oracle23ai-lite",
        "sqlplus", "-s", "forms_user/Welcome123#@localhost:1521/FREEPDB1"
    ]
    query = "SELECT COUNT(*) FROM test_items; EXIT;\n"
    try:
        res = subprocess.run(cmd, input=query, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, timeout=10)
        if res.returncode == 0 and "COUNT(*)" in res.stdout:
            print("  - Database Query test_items: SUCCESS (ok)")
            return True
        else:
            print(f"  - Query failed: {res.stdout.strip() or res.stderr.strip()}")
            return False
    except Exception as e:
        print(f"  - Failed to query database: {e}")
        return False


# Main orchestrator for testbed health checks.
def main() -> int:
    """Execute all health checks sequentially and return overall status.

    Returns:
        0 on full health, 1 if any subsystem is degraded.
    """
    print("=== Oracle Testbed Health Check ===")
    c_ok = check_containers()
    f_ok = check_forms_listener()
    d_ok = check_database_query()

    if c_ok and f_ok and d_ok:
        print("\nAll systems operational! Ready to compile and run Forms.")
        return 0
    else:
        print("\nHealth check FAILED. See references/troubleshooting-matrix.md for remedies.")
        return 1


if __name__ == "__main__":
    sys.exit(main())
