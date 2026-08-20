#!/bin/zsh

set -euo pipefail

PROJECT_DIR=${0:A:h}
INSTALL_ROOT=${CARGO_INSTALL_ROOT:-${CARGO_HOME:-$HOME/.cargo}}
CARGO_BIN_DIR=$INSTALL_ROOT/bin

echo "Installing wake-clock..."
cargo install --path "$PROJECT_DIR" --root "$INSTALL_ROOT" --force

echo "Enabling automatic daily recording and reminders..."
"$CARGO_BIN_DIR/wake-clock" install

echo
echo "wake-clock is installed and running automatically."
echo "Use 'wake-clock status' to view today's record."
