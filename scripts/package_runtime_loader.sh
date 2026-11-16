#!/usr/bin/env bash
# Assemble a portable "drop-in" loader directory:
#   loader-<target>/
#     hmloader(.exe)
#     mods/
#     run.sh / run.cmd
#     README.txt
set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "usage: $0 <hmloader-binary> <target-triple> <output-dir>" >&2
  exit 2
fi

binary="$1"
target="$2"
out="$3"

if [ ! -f "$binary" ]; then
  echo "loader binary not found: $binary" >&2
  exit 1
fi

case "$target" in
  *windows*) exe="hmloader.exe" ;;
  *) exe="hmloader" ;;
esac

rm -rf "$out"
mkdir -p "$out/mods"

cp "$binary" "$out/$exe"
chmod +x "$out/$exe" 2>/dev/null || true

cat > "$out/run.sh" <<'EOF'
#!/usr/bin/env sh
set -eu
cd "$(dirname "$0")"
exec ./hmloader runtime mods "$@"
EOF
chmod +x "$out/run.sh"

cat > "$out/run.cmd" <<'EOF'
@echo off
setlocal
cd /d "%~dp0"
"%~dp0hmloader.exe" runtime "%~dp0mods" %*
EOF

cat > "$out/README.txt" <<'EOF'
Horizon .hmmod runtime loader
=============================

1. Put every .hmmod file in the mods folder next to this file.
2. Run run.sh (Linux/macOS) or run.cmd (Windows).

The loader installs every package in mods/, then starts a build that contains
all installed mods. A .hmmod is a complete prebuilt Horizon client, not a
small runtime plugin. If the packages were built separately, the loader stops
and prints the `hmloader combine` command. You can also run the binary
directly:

  ./hmloader runtime mods
  ./hmloader install-dir mods
  ./hmloader combine mods --out modpack.hmmod \
      --source <horizon> --mixins <horizon-mixins>

Full documentation: docs/RUNTIME_LOADER.md in the hmloader repository.
EOF

cat > "$out/mods/README.txt" <<'EOF'
Put .hmmod files in this folder, then run the loader from the parent folder.
EOF

printf 'packaged %s for %s\n' "$out" "$target"
