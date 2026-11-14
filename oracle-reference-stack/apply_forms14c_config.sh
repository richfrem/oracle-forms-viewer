#!/usr/bin/env bash
# apply_forms14c_config.sh: applies every post-install adjustment the Forms 14c Oracle Home needs.
#
# Purpose:
#   After the Forms 14c install and `create_forms_domain.py`, the Oracle Home needs a set of fixes before a
#   form can launch. This script applies all of them, so a fresh Oracle Home needs no hand work. It is
#   idempotent: running it again changes nothing that is already correct.
#
#   1. Domain hooks:  bin/setUserOverrides.sh, boot.properties, the instances/forms1 symlink.
#   2. Database alias: the FREEPDB1 entry in config/fmwconfig/tnsnames.ora.
#   3. Terminal resources: fmrweb*.res copied into forms/admin/resource/US.
#   4. Runtime libraries: the 16 frmweb libraries (genshlib) and libfrmjapi.so.0 (Oracle's makefile).
#   5. Sample menu and form compiled into forms/ (.mmx and .fmx), unless --skip-samples.
#   6. Verification summary.
#
# Usage (from the repo root, inside WSL on Windows; same on macOS/Linux):
#   set -a; source .env; set +a
#   bash oracle-reference-stack/apply_forms14c_config.sh [--skip-samples] [--fix-perms]
#
# Options:
#   --skip-samples  do not compile the sample .fmx/.mmx
#   --fix-perms     also run `podman unshare chmod -R a+rX` on the Oracle Home (slow; several minutes)
#
# Requires: the Oracle Home at oracle-reference-stack/oracle_home_14c with the domain created, the
# forms-converter-14c image, and WLS_ADMIN_USER / WLS_ADMIN_PASSWORD in the environment (from .env).
# Nothing here starts, stops, or replaces a container. Library and compile steps use throwaway containers.
#
# STATUS: written from the steps verified by hand on Windows (WSL2). This script has NOT been run end to end
# on a clean Oracle Home yet. Treat the first run on a copy as its test.

set -euo pipefail

SKIP_SAMPLES=0
FIX_PERMS=0
for arg in "$@"; do
  case "$arg" in
    --skip-samples) SKIP_SAMPLES=1 ;;
    --fix-perms) FIX_PERMS=1 ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STACK="$REPO/oracle-reference-stack"
OH="$STACK/oracle_home_14c"
DOMAIN="$OH/user_projects/domains/base_domain"
CFG="$STACK/forms14c-config"
IMAGE="${OFW_CONVERTER_IMAGE:-forms-converter-14c:latest}"
FORMS_COMP="$DOMAIN/config/fmwconfig/components/FORMS"

if [ -d "/run/user/$(id -u)" ]; then export XDG_RUNTIME_DIR="/run/user/$(id -u)"; fi
[ -d "$DOMAIN" ] || { echo "ERROR: domain not found: $DOMAIN (run create_forms_domain.py first)" >&2; exit 1; }
: "${WLS_ADMIN_USER:?WLS_ADMIN_USER is not set (load .env: set -a; source .env; set +a)}"
: "${WLS_ADMIN_PASSWORD:?WLS_ADMIN_PASSWORD is not set (load .env)}"

step() { printf '\n== %s\n' "$*"; }

# Throwaway container with the Oracle Home and repo mounted; runs as the image's oracle user.
in_container() {
  podman run --rm --platform linux/amd64 \
    -v "$OH:/oracle_home_14c:Z" -v "$REPO:/work:Z" "$IMAGE" sh -c "$1"
}

if [ "$FIX_PERMS" -eq 1 ]; then
  step "0. Make container-created folders readable for the host user (slow)"
  (cd "$STACK" && podman unshare chmod -R a+rX oracle_home_14c)
fi

step "1. Domain hooks"
install -m 755 "$CFG/setUserOverrides.sh" "$DOMAIN/bin/setUserOverrides.sh"
mkdir -p "$DOMAIN/servers/AdminServer/security"
# boot.properties holds the WebLogic admin login. It is generated from .env and never committed.
printf 'username=%s\npassword=%s\n' "$WLS_ADMIN_USER" "$WLS_ADMIN_PASSWORD" \
  > "$DOMAIN/servers/AdminServer/security/boot.properties"
chmod -R a+rwX "$DOMAIN/servers/AdminServer/security"
mkdir -p "$FORMS_COMP/forms1" "$FORMS_COMP/instances"
ln -sfn ../forms1 "$FORMS_COMP/instances/forms1"
echo "setUserOverrides.sh, boot.properties, instances/forms1 -> ../forms1: done"

step "2. Database alias FREEPDB1 in tnsnames.ora"
TNS="$DOMAIN/config/fmwconfig/tnsnames.ora"
if [ -f "$TNS" ] && grep -q '^FREEPDB1' "$TNS"; then
  echo "FREEPDB1 entry already present"
else
  printf '\n' >> "$TNS"
  cat "$CFG/tnsnames-FREEPDB1.entry" >> "$TNS"
  echo "FREEPDB1 entry added"
fi

step "3. Terminal resources (fmrweb*.res)"
mkdir -p "$OH/forms/admin/resource/US"
cp -n "$OH"/forms/templates/resource/US/fmrweb*.res "$OH/forms/admin/resource/US/" 2>/dev/null || true
ls "$OH/forms/admin/resource/US" | grep -c '^fmrweb' | sed 's/^/fmrweb files present: /'

step "4a. Runtime libraries: the frmweb shared libraries (genshlib)"
in_container '
  export ORACLE_HOME=/oracle_home_14c LD_LIBRARY_PATH=/oracle_home_14c/lib:/oracle_home_14c/oracle_common/lib
  cd /oracle_home_14c/lib
  for n in iplsn ibfrmw iffw ifcw ijcw ipfw imffrmw iwfw iwcw itw icw sosdw ixw iicw iiiw iqw; do
    if [ -f "lib$n.so.0" ]; then echo "lib$n.so.0 present"; else echo "building lib$n.so.0"; /oracle_home_14c/bin/genshlib $n 0; fi
  done'

step "4b. Runtime library libfrmjapi.so.0 (Oracle makefile, not genshlib)"
in_container '
  export ORACLE_HOME=/oracle_home_14c LD_LIBRARY_PATH=/oracle_home_14c/lib:/oracle_home_14c/oracle_common/lib
  cd /oracle_home_14c/lib
  ln -sfn /usr/lib64/libaio.so.1 libaio.so;  ln -sfn /usr/lib64/libX11.so.6 libX11.so
  ln -sfn /usr/lib64/libXm.so.4 libXm.so;    ln -sfn /usr/lib64/libXt.so.6 libXt.so
  ln -sfn /usr/lib64/libXext.so.6 libXext.so; ln -sfn /usr/lib64/libXp.so.6 libXp.so
  ln -sfn /usr/lib64/libstdc++.so.6 libstdc++.so
  if [ -s libfrmjapi.so.0 ] && [ ! -L libfrmjapi.so.0 ]; then
    echo "libfrmjapi.so.0 present"
  else
    cd /oracle_home_14c/forms/lib
    make -B -f ins_forms.mk /oracle_home_14c/lib/libfrmjapi.so.0
  fi
  ln -sfn libfrmjapi.so.0 /oracle_home_14c/lib/libfrmjapi.so'

if [ "$SKIP_SAMPLES" -eq 0 ]; then
  step "5. Sample menu and form compiled into forms/ (.mmx, .fmx)"
  in_container '
    export ORACLE_HOME=/oracle_home_14c LD_LIBRARY_PATH=/oracle_home_14c/lib:/oracle_home_14c/oracle_common/lib ORACLE_TERM=vt220 TERM=vt220
    mkdir -p /tmp/job && cd /tmp/job
    export FORMS_PATH=/tmp/job:/work/samples:/oracle_home_14c/forms
    cp /work/samples/ofw_emp_menu.mmb /work/samples/ofw_demo_employees.fmb .
    /oracle_home_14c/bin/frmcmp_batch module=ofw_emp_menu.mmb module_type=menu logon=no compile_all=yes batch=yes
    /oracle_home_14c/bin/frmcmp_batch module=ofw_demo_employees.fmb module_type=form logon=no compile_all=yes batch=yes
    cp ofw_emp_menu.mmx ofw_demo_employees.fmx /oracle_home_14c/forms/'
fi

step "5b. Oracle Reports preference and printer config files (rwconverter)"
mkdir -p "$OH/tools/admin/US"
touch "$OH/tools/admin/prefs.ora"
if [ ! -f "$OH/guicommon/tk/admin/uiprint.txt" ]; then
  printf "dummy:PostScript:2:Dummy PostScript Printer:a_pnt518.ppd:\n" > "$OH/guicommon/tk/admin/uiprint.txt"
fi
if [ ! -f "$OH/guicommon/tk/admin/uiscreenprint.txt" ]; then
  printf "screenprinter:PostScript:2:ScreenPrinter:a_pnt518.ppd:\n" > "$OH/guicommon/tk/admin/uiscreenprint.txt"
fi

step "6. Verification"
FAIL=0
check() { if eval "$2"; then echo "OK   $1"; else echo "FAIL $1"; FAIL=1; fi; }
check "setUserOverrides.sh present" "[ -f '$DOMAIN/bin/setUserOverrides.sh' ]"
check "boot.properties present" "[ -f '$DOMAIN/servers/AdminServer/security/boot.properties' ]"
check "instances/forms1 symlink" "[ -L '$FORMS_COMP/instances/forms1' ]"
check "tnsnames FREEPDB1" "grep -q '^FREEPDB1' '$TNS'"
check "libfrmjapi.so.0 built" "[ -s '$OH/lib/libfrmjapi.so.0' ]"
check "Reports prefs.ora present" "[ -f '$OH/tools/admin/prefs.ora' ]"
check "Reports uiprint.txt present" "[ -f '$OH/guicommon/tk/admin/uiprint.txt' ]"
check "Reports uiscreenprint.txt present" "[ -f '$OH/guicommon/tk/admin/uiscreenprint.txt' ]"
MISSING="$(in_container 'export LD_LIBRARY_PATH=/oracle_home_14c/lib:/oracle_home_14c/oracle_common/lib; ldd /oracle_home_14c/bin/frmweb | grep -c "not found" || true' | tail -1)"
check "frmweb has no missing libraries (count=$MISSING)" "[ '${MISSING:-1}' = '0' ]"
[ "$FAIL" -eq 0 ] && echo "All adjustments applied." || { echo "Some checks failed."; exit 1; }
