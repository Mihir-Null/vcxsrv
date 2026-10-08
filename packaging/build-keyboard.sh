#!/usr/bin/env bash
# Fresh keyboard-data preparation; invoked by the wrapper in an explicitly selected guest.
set -euo pipefail
# WSLENV may import the Windows build PATH. Bootstrap guest tools explicitly.
export PATH="/run/current-system/sw/bin:/nix/var/nix/profiles/default/bin:/usr/bin:/bin:$PATH"
root=$(realpath -- "$1")
revision=$2
nar_hash=$3
[[ $revision =~ ^[0-9a-f]{40}$ ]] || { echo 'Invalid pinned nixpkgs revision' >&2; exit 2; }
[[ $nar_hash =~ ^sha256-[A-Za-z0-9+/]{43}=$ ]] || { echo 'Invalid pinned nixpkgs narHash' >&2; exit 2; }
source_dir="$root/xorg-server/xkeyboard-config"
output="$root/xorg-server/xkbdata"
[[ ! -e "$output" ]] || { echo 'Keyboard preparation requires a fresh output directory' >&2; exit 2; }
[[ -f "$source_dir/meson.build" ]] || { echo 'Keyboard source is missing' >&2; exit 2; }
work=$(mktemp -d "$root/.keyboard-build.XXXXXX")
stage=$(mktemp -d "$root/.keyboard-data.XXXXXX")
export NIX_CONFIG='experimental-features = nix-command flakes fetch-tree'
expression="let source = builtins.fetchTree { type = \"github\"; owner = \"NixOS\"; repo = \"nixpkgs\"; rev = \"$revision\"; narHash = \"$nar_hash\"; }; pkgs = import source.outPath { system = \"x86_64-linux\"; }; in [ pkgs.meson pkgs.ninja pkgs.gettext pkgs.perl pkgs.python3 ]"
nix shell --expr "$expression" --command bash -s -- "$source_dir" "$work" "$stage" <<'KEYBOARD_WORKER'
set -euo pipefail
meson setup "$2/build" "$1" -Dxorg-rules-copy=true --prefix="$3"
meson compile -C "$2/build"
meson install -C "$2/build"
test -s "$3/rules/base"
test -s "$3/symbols/us"
KEYBOARD_WORKER
# No directory-presence success: all commands and actual essential assets must pass.
test -s "$stage/rules/base"
test -s "$stage/symbols/us"
[[ ! -e "$output" ]] || { echo 'Keyboard output appeared during preparation' >&2; exit 2; }
mv -- "$stage" "$output"
# PowerShell's stdin pipeline adds a final Windows newline; exit before that CR is parsed.
exit 0
