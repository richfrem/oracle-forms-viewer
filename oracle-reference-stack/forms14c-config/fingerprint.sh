#!/bin/sh
# fingerprint.sh DIR: print a one-line fingerprint of a folder tree, for comparing two copies of the Oracle Home.
#
# Output:  files=<n> links=<n> bytes=<n> tree=<16 hex chars>
# The tree value hashes the sorted list of (relative path, type, size) for every file and symlink, so it does not
# depend on owners, permissions, or timestamps, and matches between a Windows-drive copy and an Ubuntu-disk copy.
# Run inside a container by provision_oracle_home.sh (repo mounted at /work, the folder at /d).
#
# The slim Oracle Linux image has no `find` or `xargs` (found by a dry run on 2026-11-14), so this uses `ls -lAR`
# and `awk`, which it does have. Folder sizes are left out on purpose: they differ between file systems.
cd "$1" || exit 2
ls -lAR --time-style=+%s . 2>/dev/null | awk '
  /^\.(\/.*)?:$/ { d = substr($0, 1, length($0) - 1); next }
  /^[-l]/ { t = substr($1, 1, 1); n = $0; sub(/^([^ ]+ +){6}/, "", n); print d "/" n "\t" t "\t" $5 }
' | LC_ALL=C sort > /tmp/oh-list
awk -F'\t' '$2=="-"{f++} $2=="l"{l++} {b+=$3} END{printf "files=%d links=%d bytes=%d", f, l, b}' /tmp/oh-list
printf ' tree=%s\n' "$(sha256sum /tmp/oh-list | cut -c1-16)"
