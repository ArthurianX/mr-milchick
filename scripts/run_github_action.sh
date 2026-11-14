#!/usr/bin/env bash
set -euo pipefail

# Validate command input before any build
COMMAND="${INPUT_COMMAND:-refine}"
case "$COMMAND" in
  observe|refine|explain)
    ;;
  *)
    echo "Error: Invalid command '$COMMAND'. Must be one of: observe, refine, explain" >&2
    exit 1
    ;;
esac

CONFIG_PATH="${INPUT_CONFIG_PATH:-mr-milchick.github.toml}"
TOKEN="${INPUT_TOKEN:-}"

if [ -z "$TOKEN" ]; then
  echo "Error: token input is required" >&2
  exit 1
fi

# Validate action path and manifest exist
ACTION_PATH="${GITHUB_ACTION_PATH:-}"
if [ -z "$ACTION_PATH" ]; then
  echo "Error: GITHUB_ACTION_PATH not set" >&2
  exit 1
fi

MANIFEST="$ACTION_PATH/Cargo.toml"
if [ ! -f "$MANIFEST" ]; then
  echo "Error: Action manifest not found at $MANIFEST" >&2
  exit 1
fi

# Save token to non-exported variable for CLI invocation only
SAVED_TOKEN="$TOKEN"
export -n SAVED_TOKEN
unset TOKEN
unset INPUT_TOKEN
unset GITHUB_TOKEN

# Set target dir under RUNNER_TEMP with fallback
TARGET_DIR="${RUNNER_TEMP:-/tmp}/milchick-target"
mkdir -p "$TARGET_DIR"

# Build from action source, not consumer source
# Do not leak token into build subprocess
# Run cargo inside ACTION_PATH to ensure correct toolchain/config discovery
(
  cd "$ACTION_PATH"
  cargo build --locked --release --no-default-features --features github \
    --manifest-path "$MANIFEST" \
    --target-dir "$TARGET_DIR"
)

# Execute binary with explicit env for CLI only (in original consumer cwd)
MR_MILCHICK_CONFIG_PATH="$CONFIG_PATH" \
GITHUB_TOKEN="$SAVED_TOKEN" \
"$TARGET_DIR/release/mr-milchick" "$COMMAND"
