#!/usr/bin/env python3
"""
validate_forms_xml.py (CLI)
=====================================

Purpose:
    Validates Oracle Forms XML against Oracle's own schema BEFORE converting it to a binary module with
    XML2Forms. Catches schema errors (wrong element or attribute names, bad version, wrong data types) in
    seconds, instead of after a slow convert and compile. You choose which Forms version's schema to use.

Layer: Tooling / Skill Script (compiling-and-running-oracle-forms)

Usage Examples:
    python3 plugins/oracle-forms/.apm/skills/compiling-and-running-oracle-forms/scripts/validate_forms_xml.py samples/ofw_demo_employees_fmb.xml
    python3 .../validate_forms_xml.py samples/*_fmb.xml samples/*_mmb.xml          # many files
    python3 .../validate_forms_xml.py samples/orders_summary_report.xml            # report (DTD)
    python3 .../validate_forms_xml.py form.xml --version 14c                       # schema version (default 14c)
    python3 .../validate_forms_xml.py form.xml --version auto                      # pick it from <Module version=...>
    python3 .../validate_forms_xml.py form.xml --schema path/to/forms.xsd          # any schema file
    python3 .../validate_forms_xml.py --list-versions                              # which versions are available
    python3 .../validate_forms_xml.py form.xml --backend java                      # force the container backend

What it validates:
    - Forms, menus, and object libraries: root <Module xmlns="http://xmlns.oracle.com/Forms"> against
      assets/forms-<version>.xsd (Oracle Forms Forms XML Schema). Available now: 14c (14.1.2).
    - Reports: root <report> against assets/reports.dtd (Oracle Reports DTD; not versioned here).
    - PL/SQL libraries (.pll) have no XML form, so there is nothing to validate.

Versions (--version):
    14c (default)   assets/forms-14c.xsd. The target for new XML. The schema only requires the Module version to be
                    a number (xs:long); Oracle 14c writes 141020000, but a 12c number such as 122010400 also passes.
    auto            Reads <Module version="...">: 1410xxxxx -> 14c; 12.2.1.19.0 or 122010400 -> 12c. If that
                    version's schema is not in assets/, it falls back to 14c and says so in the output (many files
                    declare 12c yet conform to the 14c schema). An explicit --version with no schema stops with exit 2.
    <other>         Any name with a matching assets/forms-<name>.xsd. Add one by extracting the schema from that
                    version's Oracle Home (converting-oracle-artifacts/scripts/extract_forms_schema.py --version).
    The 12c schema is NOT included yet. A dotted version (12.2.1.19.0) fails the 14c schema's number rule.

Backends (--backend auto picks the first available):
    xmllint (host)  -> lxml (host, Python package) -> java (a throwaway container with the JDK 17 in the
    forms-converter-14c image; needs Podman running). Install xmllint: Ubuntu `sudo apt-get install -y
    libxml2-utils`; macOS includes it.

Output / exit codes:
    One PASS or FAIL line per file with up to 10 errors (line number and message). Exit 0 = all valid,
    1 = at least one invalid or malformed, 2 = no backend, no such schema version, or a usage problem.

Script Dependencies:
    - Python 3.8+ standard library only. Optional: xmllint, lxml, Podman (for the java backend).
    - assets/forms-<version>.xsd, assets/reports.dtd, scripts/ValidateXml.java (same skill folder).

Consumed by:
    - Developers and agents after writing or editing a form, menu, library, or report XML.
    - The conversion workflow before XML -> FMB (see references/xml-validation.md).
"""

import argparse
import os
import platform
import re
import shutil
import subprocess
import sys
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import Dict, List, Optional, Tuple

SCRIPTS = Path(__file__).resolve().parent
SKILL = SCRIPTS.parent
ASSETS = SKILL / "assets"
REPO = Path(__file__).resolve().parents[6]
REPORTS_DTD = ASSETS / "reports.dtd"
FORMS_NS = "http://xmlns.oracle.com/Forms"
DEFAULT_VERSION = "14c"
IMAGE = os.environ.get("OFW_CONVERTER_IMAGE", "forms-converter-14c:latest")
IS_WSL = platform.system() == "Linux" and "microsoft" in platform.uname().release.lower()
MAX_SHOWN = 10

Finding = Tuple[int, str]  # (line, message)


def available_versions() -> List[str]:
    """Schema versions present in assets/ (files named forms-<version>.xsd)."""
    return sorted(p.stem[len("forms-"):] for p in ASSETS.glob("forms-*.xsd"))


def version_from_module(module_version: str) -> Optional[str]:
    """Map <Module version="..."> to a schema version name: 1410xxxxx -> 14c, dotted 12.x -> 12c."""
    if re.fullmatch(r"1410\d+", module_version or ""):
        return "14c"
    if re.fullmatch(r"12(\.\d+)+", module_version or "") or re.fullmatch(r"12\d{7}", module_version or ""):
        return "12c"  # dotted (12.2.1.19.0) or numeric (122010400, which is 12.2.1.4.0)
    return None


def resolve_xsd(version: str, module_version: str, schema_path: Optional[str]) -> Tuple[Optional[Path], str, Optional[str]]:
    """Return (xsd file, label, error). --schema wins; otherwise --version (auto reads the module version)."""
    if schema_path:
        path = Path(schema_path)
        return (path, f"schema {path.name}", None) if path.is_file() else (None, "", f"schema file not found: {path}")
    chosen = version
    if version == "auto":
        chosen = version_from_module(module_version) or ""
        if not chosen:
            return None, "", (f"cannot choose a schema version from Module version '{module_version}'; "
                              f"use --version {'|'.join(available_versions()) or '<name>'} or --schema FILE")
    path = ASSETS / f"forms-{chosen}.xsd"
    if version == "auto" and not path.is_file() and (ASSETS / f"forms-{DEFAULT_VERSION}.xsd").is_file():
        # The declared Module version has no schema here. Many files declare 12c yet conform to the 14c schema
        # (it only needs a number), so validate against the default and say so.
        return (ASSETS / f"forms-{DEFAULT_VERSION}.xsd",
                f"forms {DEFAULT_VERSION} XSD; file declares {chosen} (version {module_version}) and no {chosen} schema is available", None)
    if not path.is_file():
        have = ", ".join(available_versions()) or "none"
        return None, "", (f"no schema for version '{chosen}' (assets/forms-{chosen}.xsd is missing; available: {have}). "
                          f"Extract it from that version's Oracle Home with converting-oracle-artifacts/scripts/extract_forms_schema.py")
    return path, f"forms {chosen} XSD", None


def detect_kind(path: Path) -> Tuple[str, str, Optional[Finding]]:
    """Return (kind, module_version, problem). kind is "forms" or "report"; problem is a (line, message) when the
    file is not well-formed XML or is neither a Forms module nor a report."""
    try:
        for _event, element in ET.iterparse(str(path), events=("start",)):
            tag = element.tag
            if tag == f"{{{FORMS_NS}}}Module":
                return "forms", element.get("version", ""), None
            if tag == "report":
                return "report", "", None
            return "", "", (1, f"root element <{tag}> is not a Forms <Module> (namespace {FORMS_NS}) or a Reports <report>")
    except ET.ParseError as err:
        line = err.position[0] if getattr(err, "position", None) else 0
        return "", "", (line, f"not well-formed XML: {err}")
    except OSError as err:
        return "", "", (0, f"cannot read file: {err}")
    return "", "", (0, "empty file")


def run_xmllint(kind: str, path: Path, xsd: Optional[Path]) -> List[Finding]:
    """Validate one file with xmllint; return findings (empty list means valid)."""
    if kind == "forms":
        cmd = ["xmllint", "--noout", "--schema", str(xsd), str(path)]
    else:
        # --nonet and a copy of the DTD in a temp-free way: --dtdvalid overrides the DOCTYPE, but xmllint would
        # still try to load the DOCTYPE's own "reports.dtd" relative to the file, so that load is allowed to fail.
        cmd = ["xmllint", "--noout", "--nonet", "--dtdvalid", str(REPORTS_DTD), str(path)]
    result = subprocess.run(cmd, capture_output=True, text=True)
    findings: List[Finding] = []
    for raw in (result.stderr or "").splitlines():
        if not raw.strip() or raw.strip().endswith("validates") or "fails to validate" in raw:
            continue
        if kind == "report" and ("failed to load external entity" in raw or raw.lstrip().startswith(("<!DOCTYPE", "^"))):
            continue  # the DOCTYPE's own reports.dtd is not next to the file; we validate against our copy
        match = re.match(r".*?:(\d+):\s*(.*)", raw)
        findings.append((int(match.group(1)), match.group(2)) if match else (0, raw.strip()))
    if result.returncode != 0 and not findings:
        findings.append((0, f"xmllint exit {result.returncode}"))
    return findings


def run_lxml(kind: str, path: Path, xsd: Optional[Path]) -> List[Finding]:
    """Validate one file with lxml; return findings (empty list means valid)."""
    from lxml import etree  # type: ignore

    document = etree.parse(str(path))
    if kind == "forms":
        validator = etree.XMLSchema(etree.parse(str(xsd)))
    else:
        validator = etree.DTD(str(REPORTS_DTD))
    if validator.validate(document):
        return []
    return [(entry.line, entry.message) for entry in validator.error_log]


def podman_cmd(args: List[str]) -> List[str]:
    """Podman command for macOS, Linux, and WSL (the native Windows route is not supported)."""
    if IS_WSL:
        return ["env", f"XDG_RUNTIME_DIR=/run/user/{os.getuid()}", "podman", *args]
    return ["podman", *args]


def container_path(path: Path) -> str:
    """Path of a host file as seen inside the validation container (/skill = this skill, /work = the repo)."""
    resolved = path.resolve()
    for root, mount in ((SKILL, "/skill"), (REPO, "/work")):
        try:
            return f"{mount}/{resolved.relative_to(root).as_posix()}"
        except ValueError:
            continue
    raise RuntimeError(f"the java backend can only read files inside the repo ({REPO}): {path}")


def run_java(items: List[Tuple[str, Path, Optional[Path]]]) -> Dict[Path, List[Finding]]:
    """Validate files in a throwaway Forms 14c container (JDK 17); return findings per file."""
    out: Dict[Path, List[Finding]] = {}
    groups: Dict[Tuple[str, str], List[Path]] = {}
    for kind, path, xsd in items:
        key = ("xsd", container_path(xsd)) if kind == "forms" else ("dtd", container_path(REPORTS_DTD))
        groups.setdefault(key, []).append(path)
    for (mode, schema_in_container), group in groups.items():
        in_container = [container_path(path) for path in group]
        cmd = podman_cmd([
            "run", "--rm", "--platform", "linux/amd64",
            "-v", f"{SKILL.as_posix()}:/skill:ro,Z", "-v", f"{REPO.as_posix()}:/work:ro,Z",
            IMAGE, "java", "/skill/scripts/ValidateXml.java", mode, schema_in_container, *in_container,
        ])
        result = subprocess.run(cmd, capture_output=True, text=True)
        if result.returncode not in (0, 1):
            raise RuntimeError((result.stderr or result.stdout or "container run failed").strip().splitlines()[-1])
        findings: Dict[str, List[Finding]] = {c: [] for c in in_container}
        for line in result.stdout.splitlines():
            parts = line.split("\t")
            if len(parts) >= 4 and parts[1] == "ERROR":
                findings.setdefault(parts[0], []).append((int(parts[2] or 0), parts[3]))
        for path, in_path in zip(group, in_container):
            out[path] = findings.get(in_path, [])
    return out


def choose_backend(requested: str) -> Optional[str]:
    """Pick a backend: the requested one, or the first available (xmllint, lxml, java)."""
    if requested != "auto":
        return requested
    if shutil.which("xmllint"):
        return "xmllint"
    try:
        import lxml  # type: ignore  # noqa: F401
        return "lxml"
    except ImportError:
        pass
    if shutil.which("podman") and platform.system() != "Windows":
        return "java"
    return None


def parse_cli_args(argv: List[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Validate Oracle Forms and Reports XML against Oracle's schema")
    parser.add_argument("files", nargs="*", help="XML files to validate")
    parser.add_argument("--version", default=DEFAULT_VERSION,
                        help=f"Forms schema version: a name with assets/forms-<name>.xsd (available: "
                             f"{', '.join(available_versions()) or 'none'}), or 'auto' to read it from <Module version=...> "
                             f"(default: {DEFAULT_VERSION})")
    parser.add_argument("--schema", help="validate Forms XML against this XSD file instead (overrides --version)")
    parser.add_argument("--list-versions", action="store_true", help="list the schema versions in assets/ and exit")
    parser.add_argument("--backend", choices=["auto", "xmllint", "lxml", "java"], default="auto",
                        help="validator to use (default: first available of xmllint, lxml, java)")
    args = parser.parse_args(argv)
    if not args.files and not args.list_versions:
        parser.error("give at least one XML file (or --list-versions)")
    return args


def main() -> int:
    args = parse_cli_args(sys.argv[1:])
    if args.list_versions:
        for name in available_versions():
            print(f"{name}\t{ASSETS / ('forms-' + name + '.xsd')}")
        print(f"(default: {DEFAULT_VERSION}; reports: {REPORTS_DTD.name})")
        return 0
    if not REPORTS_DTD.is_file():
        print(f"ERROR: missing asset {REPORTS_DTD}", file=sys.stderr)
        return 2
    backend = choose_backend(args.backend)
    if backend is None:
        print("ERROR: no validator available. Install xmllint (Ubuntu: sudo apt-get install -y libxml2-utils), "
              "or lxml, or start Podman for the java backend.", file=sys.stderr)
        return 2

    results: Dict[Path, List[Finding]] = {}
    labels: Dict[Path, str] = {}
    pending: List[Tuple[str, Path, Optional[Path]]] = []
    for name in args.files:
        path = Path(name)
        kind, module_version, problem = detect_kind(path)
        if problem:
            results[path] = [problem]
            continue
        if kind == "report":
            labels[path] = "reports DTD"
            pending.append((kind, path, None))
            continue
        xsd, label, error = resolve_xsd(args.version, module_version, args.schema)
        if error:
            if args.version != "auto" or args.schema:
                print(f"ERROR: {error}", file=sys.stderr)
                return 2
            results[path] = [(0, error)]
            continue
        labels[path] = label
        pending.append((kind, path, xsd))

    try:
        if backend == "java":
            if pending:
                results.update(run_java(pending))
        else:
            runner = run_xmllint if backend == "xmllint" else run_lxml
            for kind, path, xsd in pending:
                results[path] = runner(kind, path, xsd)
    except (RuntimeError, OSError, ImportError) as err:
        print(f"ERROR: {backend} backend failed: {err}", file=sys.stderr)
        return 2

    failures = 0
    for name in args.files:
        path = Path(name)
        findings = results.get(path, [])
        label = labels.get(path, "not validated")
        if not findings:
            print(f"PASS  {name}  ({label}, {backend})")
            continue
        failures += 1
        print(f"FAIL  {name}  ({label}, {backend}): {len(findings)} error(s)")
        for line, message in findings[:MAX_SHOWN]:
            print(f"      line {line}: {message}")
        if len(findings) > MAX_SHOWN:
            print(f"      ... {len(findings) - MAX_SHOWN} more")
    print(f"\n{len(args.files) - failures} passed, {failures} failed (backend: {backend})")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
