#!/usr/bin/env bash
# restore_oracle_home_attributes.sh: repair a hand-copied Oracle Home so it matches the original.
#
# WHY
#   Copying the Oracle Home with Windows Explorer (or any Windows tool) into the Ubuntu disk loses what Linux needs:
#   the execute permission on the programs, the 404 symbolic links, and the file owner the container expects. It may also
#   leave files missing. This script takes the ORIGINAL folder as the reference and fixes the copy to match, in one pass.
#
# WHAT IT DOES (in order)
#   1. rsync from the original to the copy: copies any file that is missing or has a different size, restores the
#      permissions, and recreates the symbolic links. Files that already have the right size are not copied again
#      (their contents are not compared, only their size).
#   2. Sets the owner of everything to the container's `oracle` user (UID/GID 1000), so WebLogic can write its logs.
#      This uses `podman unshare`, which maps that ID to the right owner on the host.
#   Both steps run inside `podman unshare`, so every file in the original is readable even when its owner is a
#   container-mapped user ID.
#
# USAGE (from the repo root, in the Ubuntu shell). Always do the dry run first.
#   bash oracle-reference-stack/restore_oracle_home_attributes.sh --dry-run --from oracle-reference-stack/oracle_home_14c --target ~/oracle_home_14c
#   bash oracle-reference-stack/restore_oracle_home_attributes.sh            --from oracle-reference-stack/oracle_home_14c --target ~/oracle_home_14c
#   Then prove the result:
#   bash oracle-reference-stack/provision_oracle_home.sh --verify --target ~/oracle_home_14c --against oracle-reference-stack/oracle_home_14c
#
# SAFETY
#   Never changes the original (it is only read). Never deletes anything in the copy (no --delete). Stop the Forms
#   server first if you want the original to stop changing (podman stop forms-server-14c); that is the owner's choice.
#
# STATUS: written 2026-11-14. NOT YET RUN. Same-size files whose contents differ are not detected (size-only compare).
set -euo pipefail

SRC=""; DST=""; DRY=0
while [ $# -gt 0 ]; do
  case "$1" in
    --from) SRC="$2"; shift 2 ;;
    --target) DST="$2"; shift 2 ;;
    --dry-run) DRY=1; shift ;;
    -h|--help) sed -n '2,32p' "$0"; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done
[ -n "$SRC" ] && [ -n "$DST" ] || { echo "Give --from <original folder> and --target <the copy>. See --help." >&2; exit 2; }

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
abs() { local p="${1/#\~/$HOME}"; case "$p" in /*) echo "$p" ;; *) echo "$REPO/$p" ;; esac; }
SRC="$(abs "$SRC")"; DST="$(abs "$DST")"
[ -d "$SRC" ] || { echo "original not found: $SRC" >&2; exit 1; }
[ -d "$DST" ] || { echo "target not found: $DST" >&2; exit 1; }
command -v rsync >/dev/null || { echo "rsync is not installed (sudo apt-get install -y rsync)" >&2; exit 1; }
[ -d "/run/user/$(id -u)" ] && export XDG_RUNTIME_DIR="/run/user/$(id -u)"

say() { printf '[%s] %s\n' "$(date +%H:%M:%S)" "$*"; }
say "Original: $SRC"
say "Copy:     $DST"
say "Dry run:  $DRY"

# 1. Missing files, permissions, symbolic links
RSYNC_OPTS=(-rlpt --size-only --no-owner --no-group --stats)
[ "$DRY" -eq 1 ] && RSYNC_OPTS+=(-n -i)
say "Step 1: rsync (adds missing files, restores permissions and symbolic links). This reads the original over the slow drive, a few minutes."
podman unshare rsync "${RSYNC_OPTS[@]}" "$SRC/" "$DST/" | tail -25

if [ "$DRY" -eq 1 ]; then
  say "DRY RUN finished: nothing was changed. The lines above show what a real run would add or change."
  exit 0
fi

# 2. Owner: the container's oracle user (UID/GID 1000)
say "Step 2: setting the owner to the container's oracle user (1000:1000)"
podman unshare chown -R 1000:1000 "$DST"

# Quick checks
say "Checks:"
for f in bin/frmcmp_batch bin/frmweb bin/rwconverter user_projects/domains/base_domain/bin/startWebLogic.sh; do
  if [ -e "$DST/$f" ]; then say "  $f: $(podman unshare ls -l "$DST/$f" | cut -c1-10)"; else say "  $f: MISSING"; fi
done
say "  symbolic links in the copy: $(podman unshare find "$DST" -type l | wc -l)  (the original has 404)"
say "Done. Now run provision_oracle_home.sh --verify --target ... --against ... to prove the copy matches."
