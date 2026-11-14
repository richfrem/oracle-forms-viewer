#!/usr/bin/env bash
# provision_oracle_home.sh: create, copy, verify, and back up the Forms 14c Oracle Home on a chosen disk.
#
# WHY THIS EXISTS
#   The Oracle Home is 5+ GB of installed Oracle software plus the WebLogic domain and every adjustment that took
#   hours to get right. It cannot go in git. By default it sits in the repo folder, which on Windows is the slow
#   Windows drive (/mnt/c, a network-style 9p mount): WebLogic made 15 to 44 minutes of tiny file reads to boot, against
#   about 2 minutes on a Mac. Putting it on Ubuntu's own disk (ext4) avoids that "Windows tax". What lives in git is
#   this script and its inputs, so the Oracle Home can always be rebuilt or moved by one command, not by memory.
#   It is a host folder mounted into containers, so updating or recreating containers and images never touches it.
#
# THE PROTOCOL (what you run, in order; details in the setup skill's references/oracle-home-location.md)
#   1. --dry-run   print what would happen. Changes nothing.
#   2. --from-copy SRC   (or --from-installers)   build the Oracle Home at the target. Refuses a non-empty target.
#   3. --verify --against SRC   prove the copy: required files, library check, a tree fingerprint compared with the
#      source, and a timed file walk of both locations.
#   4. Point the stack at it: set OFW_ORACLE_HOME in .env, then recreate forms-server-14c with the new mount.
#      This script never stops, removes, or recreates a container; it prints the step for you to approve.
#   5. --backup DIR   write a .tar.gz you can restore from.
#
# Usage (from the repo root, in the Ubuntu shell; same on macOS/Linux):
#   set -a; source .env; set +a                                   # loads OFW_ORACLE_HOME, WLS_*, DB_*
#   bash oracle-reference-stack/provision_oracle_home.sh --dry-run --from-copy oracle-reference-stack/oracle_home_14c --target ~/oracle_home_14c
#   bash oracle-reference-stack/provision_oracle_home.sh --from-copy oracle-reference-stack/oracle_home_14c --target ~/oracle_home_14c
#   bash oracle-reference-stack/provision_oracle_home.sh --verify --target ~/oracle_home_14c --against oracle-reference-stack/oracle_home_14c
#   bash oracle-reference-stack/provision_oracle_home.sh --backup temp/backups --target ~/oracle_home_14c
#   bash oracle-reference-stack/provision_oracle_home.sh --from-installers --target ~/oracle_home_14c   # full rebuild (UNVERIFIED)
#
# Options:
#   --target PATH         Oracle Home to create or check. Default: $OFW_ORACLE_HOME, else oracle-reference-stack/oracle_home_14c
#   --from-copy SRC       copy an existing Oracle Home (preserves ownership, permissions, symlinks) into the target
#   --from-installers     run the two silent installs, create the domain, and apply the configuration (UNVERIFIED)
#   --verify              check the target (add --against SRC to compare it with a source)
#   --backup DIR          write DIR/oracle_home_14c-<timestamp>.tar.gz from the target and check the archive
#   --dry-run             print the plan only
#
# How the copy works: a throwaway container runs as root (which maps to your own user, so it can read files owned by
# the container's mapped user IDs) and streams `tar` from the source to the target, preserving numbers for owners.
# Plain `cp` from the host would lose that ownership. A copy is never a move: the source is mounted read-only.
#
# STATUS: written 2026-11-14. NOT YET RUN. The copy, verify, and backup modes use only commands that exist on a normal
# Podman install, but the first run is the test. --from-installers follows the documented steps and is unverified on
# the Ubuntu disk.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STACK="$REPO/oracle-reference-stack"
IMAGE="${OFW_CONVERTER_IMAGE:-forms-converter-14c:latest}"
[ -d "/run/user/$(id -u)" ] && export XDG_RUNTIME_DIR="/run/user/$(id -u)"

TARGET="${OFW_ORACLE_HOME:-$STACK/oracle_home_14c}"
MODE=""; SRC=""; AGAINST=""; BACKUP_DIR=""; DRY=0
while [ $# -gt 0 ]; do
  case "$1" in
    --target) TARGET="$2"; shift 2 ;;
    --from-copy) MODE="copy"; SRC="$2"; shift 2 ;;
    --from-installers) MODE="installers"; shift ;;
    --verify) MODE="verify"; shift ;;
    --against) AGAINST="$2"; shift 2 ;;
    --backup) MODE="backup"; BACKUP_DIR="$2"; shift 2 ;;
    --dry-run) DRY=1; shift ;;
    -h|--help) sed -n '2,45p' "$0"; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done
[ -n "$MODE" ] || { echo "Give one of --from-copy, --from-installers, --verify, --backup (or --help)." >&2; exit 2; }

abs() { local p="${1/#\~/$HOME}"; case "$p" in /*) echo "$p" ;; *) echo "$REPO/$p" ;; esac; }
TARGET="$(abs "$TARGET")"
[ -n "$SRC" ] && SRC="$(abs "$SRC")"
[ -n "$AGAINST" ] && AGAINST="$(abs "$AGAINST")"
[ -n "$BACKUP_DIR" ] && BACKUP_DIR="$(abs "$BACKUP_DIR")"

say() { printf '[%s] %s\n' "$(date +%H:%M:%S)" "$*"; }
run() { if [ "$DRY" -eq 1 ]; then say "DRY-RUN would run: $*"; else "$@"; fi; }
fail() { say "FAIL: $*"; exit 1; }

# Throwaway container as root. Usage: croot "<mounts...>" "<shell command>"
croot() {
  # shellcheck disable=SC2086
  podman run --rm --user root --platform linux/amd64 $1 "$IMAGE" sh -c "$2"
}

# One-line fingerprint of a tree: file count, link count, total bytes, and a hash of the sorted (path, type, size) list.
# Ownership-independent, so it matches between a Windows-drive copy and an ext4 copy.
fingerprint() {
  croot "-v $1:/d:ro,Z -v $REPO:/work:ro,Z" 'sh /work/oracle-reference-stack/forms14c-config/fingerprint.sh /d' | tail -1
}

# Time a walk of the whole tree (how slow is this disk for the many small reads WebLogic does).
timed_walk() {
  local start end n
  start=$(date +%s)
  n=$(croot "-v $1:/d:ro,Z" 'ls -AR /d | wc -l' | tail -1)    # the image has no `find`
  end=$(date +%s)
  echo "walked $n directory entries in $((end - start))s"
}

REQUIRED=(bin/frmcmp_batch bin/frmweb bin/rwconverter forms/java/frmsal.jar jlib/frmxmltools.jar lib/libfrmjapi.so.0
  user_projects/domains/base_domain/bin/startWebLogic.sh user_projects/domains/base_domain/bin/setUserOverrides.sh
  user_projects/domains/base_domain/servers/AdminServer/security/boot.properties
  user_projects/domains/base_domain/config/fmwconfig/tnsnames.ora)

do_verify() {
  say "Verifying $TARGET"
  [ -d "$TARGET" ] || fail "target folder not found: $TARGET"
  local bad=0
  for rel in "${REQUIRED[@]}"; do
    if croot "-v $TARGET:/d:ro,Z" "test -e /d/$rel"; then say "  OK   $rel"; else say "  MISSING $rel"; bad=1; fi
  done
  local missing
  missing="$(croot "-v $TARGET:/oracle_home_14c:ro,Z" 'export LD_LIBRARY_PATH=/oracle_home_14c/lib:/oracle_home_14c/oracle_common/lib; ldd /oracle_home_14c/bin/frmweb | grep -c "not found" || true' | tail -1)"
  if [ "${missing:-1}" = "0" ]; then say "  OK   frmweb has no missing libraries"; else say "  FAIL frmweb has $missing missing libraries"; bad=1; fi
  local fp_target; fp_target="$(fingerprint "$TARGET")"; say "target   : $fp_target"
  say "target   : $(timed_walk "$TARGET")"
  if [ -n "$AGAINST" ]; then
    local fp_src; fp_src="$(fingerprint "$AGAINST")"; say "source   : $fp_src"
    say "source   : $(timed_walk "$AGAINST")"
    if [ "$fp_src" = "$fp_target" ]; then say "  OK   tree fingerprints are IDENTICAL (same files, types, and sizes)"; else say "  FAIL fingerprints DIFFER (the copy is not the same as the source)"; bad=1; fi
  fi
  [ "$bad" -eq 0 ] && say "VERIFIED." || fail "verification failed (see above)"
  say "Not covered here: that WebLogic boots from this Oracle Home. Test that by recreating forms-server-14c on it and timing the boot (start_testbed.py)."
}

do_copy() {
  [ -d "$SRC" ] || fail "source not found: $SRC"
  if [ -d "$TARGET" ] && [ -n "$(ls -A "$TARGET" 2>/dev/null)" ]; then fail "target is not empty: $TARGET (refusing to overwrite; choose another --target or remove it yourself)"; fi
  local src_fp; src_fp="$(fingerprint "$SRC")"; say "source fingerprint: $src_fp"
  local need avail
  need="$(echo "$src_fp" | sed -n 's/.*bytes=\([0-9]*\).*/\1/p')"
  mkdir -p "$TARGET" 2>/dev/null || true
  avail="$(df -B1 --output=avail "$TARGET" 2>/dev/null | tail -1 | tr -d ' ')"
  say "needs ${need:-?} bytes; free at target: ${avail:-?} bytes"
  [ -n "$need" ] && [ -n "$avail" ] && [ "$avail" -lt "$((need + need / 10))" ] && fail "not enough free space at $TARGET"
  say "Copying $SRC -> $TARGET (a few minutes; source is mounted read-only)"
  run mkdir -p "$TARGET"
  run podman run --rm --user root --platform linux/amd64 -v "$SRC:/src:ro,Z" -v "$TARGET:/dst:Z" "$IMAGE" \
    sh -c 'cd /src && tar cf - . | (cd /dst && tar xpf -)'
  [ "$DRY" -eq 1 ] || { say "Copy finished. Now verifying against the source."; AGAINST="$SRC"; do_verify; }
}

do_installers() {
  say "FULL REBUILD from the installers into $TARGET (UNVERIFIED on the Ubuntu disk)"
  for f in fmw_14.1.2.0.0_infrastructure.jar fmw_14.1.2.0.0_fr_linux64.bin infra14c_silent.rsp forms14c_silent.rsp oraInst.loc; do
    [ -f "$STACK/installer/$f" ] || fail "missing $STACK/installer/$f (see the setup skill, 'Oracle installers')"
  done
  if [ -d "$TARGET" ] && [ -n "$(ls -A "$TARGET" 2>/dev/null)" ]; then fail "target is not empty: $TARGET"; fi
  run mkdir -p "$TARGET"; run chmod 777 "$TARGET"      # the container's mapped user needs to write here (documented step)
  chmod +x "$STACK/installer/fmw_14.1.2.0.0_fr_linux64.bin" 2>/dev/null || true
  say "Stage 1: FMW Infrastructure (about 25 minutes)"
  run podman run --rm --platform linux/amd64 -v "$STACK/installer:/installer:Z" -v "$TARGET:/oracle_home_14c:Z" "$IMAGE" \
    java -jar /installer/fmw_14.1.2.0.0_infrastructure.jar -silent -ignoreSysPrereqs -responseFile /installer/infra14c_silent.rsp -invPtrLoc /installer/oraInst.loc
  say "Stage 2: Forms and Reports (the installer may exit 139 after reporting success; the checks below decide)"
  run podman run --rm --platform linux/amd64 -v "$STACK/installer:/installer:Z" -v "$TARGET:/oracle_home_14c:Z" "$IMAGE" \
    /installer/fmw_14.1.2.0.0_fr_linux64.bin -silent -ignoreSysPrereqs -responseFile /installer/forms14c_silent.rsp -invPtrLoc /installer/oraInst.loc || say "Stage 2 exited non-zero; continuing to the checks"
  say "Creating the WebLogic domain (needs the oracle-net network and .env with WLS_ADMIN_PASSWORD)"
  run podman run --rm --platform linux/amd64 --network oracle-net --env-file "$REPO/.env" -v "$TARGET:/oracle_home_14c:Z" -v "$REPO:/work:Z" "$IMAGE" \
    /oracle_home_14c/oracle_common/common/bin/wlst.sh /work/plugins/oracle-forms/.apm/skills/setting-up-oracle-testbed/scripts/create_forms_domain.py
  say "Applying the post-install configuration"
  run env OFW_ORACLE_HOME="$TARGET" bash "$STACK/apply_forms14c_config.sh"
  [ "$DRY" -eq 1 ] || do_verify
}

do_backup() {
  [ -d "$TARGET" ] || fail "target not found: $TARGET"
  local out; out="oracle_home_14c-$(date +%Y%m%d-%H%M).tar.gz"
  run mkdir -p "$BACKUP_DIR"
  say "Writing $BACKUP_DIR/$out (several minutes)"
  run podman run --rm --user root --platform linux/amd64 -v "$TARGET:/src:ro,Z" -v "$BACKUP_DIR:/out:Z" "$IMAGE" \
    sh -c "cd /src && tar czpf /out/$out ."
  if [ "$DRY" -eq 0 ]; then
    local entries; entries="$(tar tzf "$BACKUP_DIR/$out" | wc -l)"
    say "Archive written: $(du -h "$BACKUP_DIR/$out" | cut -f1), $entries entries (listing it succeeded, so the archive is readable)"
  fi
}

say "Mode: $MODE   Target: $TARGET   Image: $IMAGE   Dry run: $DRY"
case "$MODE" in
  copy) do_copy ;;
  installers) do_installers ;;
  verify) do_verify ;;
  backup) do_backup ;;
esac
cat <<EOF

NEXT (not done by this script): to run the server from $TARGET, set OFW_ORACLE_HOME=$TARGET in .env, then recreate
forms-server-14c with the mount "-v $TARGET:/oracle_home_14c:Z". Its current settings are saved in
temp/image-backup/forms-server-14c.inspect.json (command, mounts, network oracle-net, ports 7001 and 9001). Removing or
recreating a container is your decision; nothing is lost, because the configuration lives in the Oracle Home folder.
EOF
