#!/bin/sh
set -eu

cd "$(dirname "$0")"
cargo install --path .

binary="${CARGO_HOME:-$HOME/.cargo}/bin/lexpeek"
data="${XDG_DATA_HOME:-$HOME/.local/share}"

# GNOME launches desktop entries without ~/.cargo/bin in PATH, so the binary path is written in full.
mkdir -p "$data/applications"
sed "s|^Exec=lexpeek|Exec=$binary|" data/io.github.nihilmagnum.Lexpeek.desktop \
    > "$data/applications/io.github.nihilmagnum.Lexpeek.desktop"
install -Dm644 data/io.github.nihilmagnum.Lexpeek.svg "$data/icons/hicolor/scalable/apps/io.github.nihilmagnum.Lexpeek.svg"
gtk4-update-icon-cache -q -t "$data/icons/hicolor" 2>/dev/null || true
