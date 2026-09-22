#!/bin/sh
set -eu

cd "$(dirname "$0")"
cargo install --path .

binary="${CARGO_HOME:-$HOME/.cargo}/bin/lexpeek"
data="${XDG_DATA_HOME:-$HOME/.local/share}"

# GNOME запускает ярлыки без ~/.cargo/bin в PATH, поэтому путь к бинарнику прописывается целиком.
mkdir -p "$data/applications"
sed "s|@BINARY@|$binary|" data/io.github.lexpeek.Lexpeek.desktop > "$data/applications/io.github.lexpeek.Lexpeek.desktop"
install -Dm644 data/io.github.lexpeek.Lexpeek.svg "$data/icons/hicolor/scalable/apps/io.github.lexpeek.Lexpeek.svg"
gtk4-update-icon-cache -q -t "$data/icons/hicolor" 2>/dev/null || true

# Фоновому приложению GNOME не показывает диалог разрешения скриншотов, поэтому оно выдаётся при установке.
gdbus call --session \
    --dest org.freedesktop.impl.portal.PermissionStore \
    --object-path /org/freedesktop/impl/portal/PermissionStore \
    --method org.freedesktop.impl.portal.PermissionStore.SetPermission \
    screenshot true screenshot io.github.lexpeek.Lexpeek "['yes']" > /dev/null
