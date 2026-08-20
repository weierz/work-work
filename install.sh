#!/bin/zsh

set -euo pipefail

REPOSITORY=weierz/work-work
SCRIPT_DIR=${0:A:h}
INSTALL_BIN_DIR=$HOME/.local/bin
TEMP_INSTALL_DIR=

cleanup() {
  if [[ -n $TEMP_INSTALL_DIR ]]; then
    rm -rf "$TEMP_INSTALL_DIR"
  fi
}
trap cleanup EXIT HUP INT TERM

if [[ ${0:t} == "install.sh" ]] && [[ -f "$SCRIPT_DIR/Cargo.toml" ]] && grep -q 'name = "work-work"' "$SCRIPT_DIR/Cargo.toml"; then
  if ! command -v cargo >/dev/null 2>&1; then
    echo "Error: a Rust toolchain is required when installing from source." >&2
    exit 1
  fi
  if [[ ${WW_INSTALL_DRY_RUN:-0} == 1 ]]; then
    cargo metadata --manifest-path "$SCRIPT_DIR/Cargo.toml" --no-deps --format-version 1 >/dev/null
    echo "Source installer validation succeeded."
    exit 0
  fi

  INSTALL_ROOT=${CARGO_INSTALL_ROOT:-${CARGO_HOME:-$HOME/.cargo}}
  echo "Building work-work from source..."
  cargo install --path "$SCRIPT_DIR" --root "$INSTALL_ROOT" --force
  WW_EXECUTABLE=$INSTALL_ROOT/bin/ww
else
  for command_name in curl tar uname; do
    if ! command -v "$command_name" >/dev/null 2>&1; then
      echo "Error: required command '$command_name' was not found." >&2
      exit 1
    fi
  done

  case $(uname -m) in
    arm64 | aarch64) TARGET=aarch64-apple-darwin ;;
    x86_64) TARGET=x86_64-apple-darwin ;;
    *)
      echo "Error: unsupported Mac architecture '$(uname -m)'." >&2
      exit 1
      ;;
  esac

  RELEASE_PATH=latest/download
  if [[ -n ${WW_VERSION:-} ]]; then
    RELEASE_PATH=download/$WW_VERSION
  fi
  ASSET_URL=https://github.com/$REPOSITORY/releases/$RELEASE_PATH/work-work-$TARGET.tar.gz

  if [[ ${WW_INSTALL_DRY_RUN:-0} == 1 ]]; then
    echo "Binary installer validation succeeded for $TARGET."
    echo "Asset: $ASSET_URL"
    exit 0
  fi

  TEMP_INSTALL_DIR=$(mktemp -d "${TMPDIR:-/tmp}/work-work.XXXXXX")
  echo "Downloading work-work for $TARGET..."
  curl -fsSL "$ASSET_URL" -o "$TEMP_INSTALL_DIR/work-work.tar.gz"
  tar -xzf "$TEMP_INSTALL_DIR/work-work.tar.gz" -C "$TEMP_INSTALL_DIR"
  if [[ ! -x "$TEMP_INSTALL_DIR/ww" ]]; then
    echo "Error: the release archive does not contain an executable ww binary." >&2
    exit 1
  fi

  mkdir -p "$INSTALL_BIN_DIR"
  /usr/bin/install -m 755 "$TEMP_INSTALL_DIR/ww" "$INSTALL_BIN_DIR/ww"
  WW_EXECUTABLE=$INSTALL_BIN_DIR/ww
fi

echo "Enabling automatic daily recording and reminders..."
"$WW_EXECUTABLE" install

echo
echo "work-work is installed and running automatically."
echo "Use 'ww status' to view today's record."
