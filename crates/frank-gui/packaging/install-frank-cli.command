#!/bin/sh
set -eu

# cargo-packager (M-6) puts this script in Contents/Resources/ and every
# binary -- frank-gui and frank alike -- in the sibling Contents/MacOS/,
# the standard macOS bundle layout. That is a real change from the Tauri
# packaging this script was originally written against, which put its
# sidecar CLI resource in Contents/Resources/ next to the script itself
# (hence the two legacy fallback locations kept below for any bundle still
# built that way).
resource_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
source="$resource_dir/../MacOS/frank"
if [ ! -f "$source" ]; then
  source="$resource_dir/frank"
fi
if [ ! -f "$source" ]; then
  source="$resource_dir/frank-cli/frank"
fi
destination="${HOME:?}/.local/bin/frank"
destination_dir=$(dirname -- "$destination")

if [ -L "$source" ] || [ ! -f "$source" ] || [ ! -x "$source" ]; then
  printf '%s\n' 'Frank CLI is not present in this desktop bundle.' >&2
  exit 1
fi

for parent in "$HOME/.local" "$destination_dir"; do
  if [ -L "$parent" ]; then
    printf 'Refusing to traverse symlinked install directory: %s\n' "$parent" >&2
    exit 1
  fi
done
mkdir -p "$destination_dir"
if [ -L "$HOME/.local" ] || [ -L "$destination_dir" ] || [ -L "$destination" ]; then
  printf 'Refusing to replace symlink: %s\n' "$destination" >&2
  exit 1
fi

temporary=$(mktemp "$destination_dir/.frank.XXXXXX")
trap 'rm -f "$temporary"' EXIT HUP INT TERM
install -m 0755 "$source" "$temporary"
# A final destination check prevents a user-created symlink from becoming an
# implicit redirect between the initial check and the atomic rename.
if [ -L "$destination" ] || [ -L "$destination_dir" ] || [ -L "$HOME/.local" ]; then
  printf 'Refusing to replace symlink: %s\n' "$destination" >&2
  exit 1
fi
mv -f "$temporary" "$destination"
trap - EXIT HUP INT TERM
printf 'Installed Frank CLI at %s\n' "$destination"
