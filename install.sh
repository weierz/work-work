#!/bin/zsh

set -euo pipefail

REPOSITORY_ARCHIVE=https://github.com/weierz/work-work/archive/refs/heads/main.tar.gz
SCRIPT_DIR=${0:A:h}
TEMP_PROJECT_DIR=

if [[ ${0:t} == "install.sh" ]] && [[ -f "$SCRIPT_DIR/Cargo.toml" ]] && grep -q 'name = "work-work"' "$SCRIPT_DIR/Cargo.toml"; then
  PROJECT_DIR=$SCRIPT_DIR
else
  for command_name in curl tar cargo; do
    if ! command -v "$command_name" >/dev/null 2>&1; then
      echo "Error: required command '$command_name' was not found." >&2
      exit 1
    fi
  done

  TEMP_PROJECT_DIR=$(mktemp -d "${TMPDIR:-/tmp}/work-work.XXXXXX")
  trap '[[ -n "$TEMP_PROJECT_DIR" ]] && rm -rf "$TEMP_PROJECT_DIR"' EXIT HUP INT TERM
  echo "Downloading work-work..."
  curl -fsSL "$REPOSITORY_ARCHIVE" | tar -xz -C "$TEMP_PROJECT_DIR" --strip-components=1
  PROJECT_DIR=$TEMP_PROJECT_DIR
fi

INSTALL_ROOT=${CARGO_INSTALL_ROOT:-${CARGO_HOME:-$HOME/.cargo}}
CARGO_BIN_DIR=$INSTALL_ROOT/bin

if [[ ${WW_INSTALL_DRY_RUN:-0} == 1 ]]; then
  cargo metadata --manifest-path "$PROJECT_DIR/Cargo.toml" --no-deps --format-version 1 >/dev/null
  echo "Installer download and project validation succeeded."
  exit 0
fi

echo "Installing work-work..."
cargo install --path "$PROJECT_DIR" --root "$INSTALL_ROOT" --force

echo "Enabling automatic daily recording and reminders..."
"$CARGO_BIN_DIR/ww" install

echo
echo "work-work is installed and running automatically."
echo "Use 'ww status' to view today's record."
