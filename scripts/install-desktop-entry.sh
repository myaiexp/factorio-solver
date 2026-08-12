#!/usr/bin/env bash
# Install (or refresh) the desktop entry that launches this checkout.
set -euo pipefail

self=$(readlink -f "${BASH_SOURCE[0]}")
repo=$(dirname "$(dirname "$self")")
apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
entry="$apps/factorio-solver.desktop"

mkdir -p "$apps"
chmod +x "$repo/scripts/launch.sh"

# StartupWMClass matches the app_id set in crates/ui/src/main.rs, which is what
# lets a Wayland compositor pair the running window with this entry's icon.
cat >"$entry" <<EOF
[Desktop Entry]
Type=Application
Version=1.0
Name=Factorio Solver
GenericName=Blueprint Generator
Comment=Turn a production goal into a pasteable Factorio blueprint
Exec=$repo/scripts/launch.sh
Icon=$repo/assets/icon.svg
Terminal=false
StartupNotify=true
StartupWMClass=factorio-solver
Categories=Game;Utility;
Keywords=factorio;blueprint;layout;solver;production;
EOF

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$apps" 2>/dev/null || true
fi

echo "Installed $entry"
echo "  launches $repo/scripts/launch.sh"
