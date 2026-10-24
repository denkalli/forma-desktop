#!/usr/bin/env bash
# Builds the Forma Flatpak and installs it for the current user.
#
# Needs, from Flathub (user installation):
#   org.flatpak.Builder, org.freedesktop.Sdk//26.08,
#   org.freedesktop.Sdk.Extension.rust-stable//26.08, org.freedesktop.Platform//26.08
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
desktop="$(cd "$here/../.." && pwd)"
# Both libforma and forma-desktop are sources, so the builder needs their parent folder.
root="$(cd "$desktop/.." && pwd)"
work="$desktop/target/flatpak"
mkdir -p "$work"

# The sandbox has no network: vendor the crates first (from the local Cargo cache when
# possible).
echo "Vendoring Rust dependencies..."
rm -rf "$work/vendor"
cargo --quiet vendor --locked --manifest-path "$desktop/Cargo.toml" "$work/vendor" > /dev/null

if command -v flatpak-builder > /dev/null; then
    builder=(flatpak-builder)
else
    builder=(flatpak run --filesystem="$root" org.flatpak.Builder)
fi

"${builder[@]}" \
    --user --install --force-clean \
    --state-dir="$work/state" \
    "$work/build" \
    "$here/io.github.denkalli.Forma.yml"

echo "Installed. Run it with: flatpak run io.github.denkalli.Forma"
