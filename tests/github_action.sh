#!/usr/bin/env bash
set -euo pipefail

# Shell regression harness for GitHub action wrapper
# Creates temporary fake action source, fake cargo, fake built binary and cleans up
# No network/build toolchain needed

TEST_DIR=$(mktemp -d)
trap 'rm -rf "$TEST_DIR"' EXIT

# Create fake action structure with spaces in path
FAKE_ACTION="$TEST_DIR/action dir"
mkdir -p "$FAKE_ACTION/scripts"
cp scripts/run_github_action.sh "$FAKE_ACTION/scripts/"
touch "$FAKE_ACTION/Cargo.toml"

# Fake cargo that records arguments and verifies token absence
FAKE_CARGO_BIN="$TEST_DIR/bin"
mkdir -p "$FAKE_CARGO_BIN"
cat > "$FAKE_CARGO_BIN/cargo" << 'EOF'
#!/usr/bin/env bash
# Verify no token variables are present in environment
if [ -n "${INPUT_TOKEN:-}" ]; then
  echo "FAIL: INPUT_TOKEN should not be set during cargo build" >&2
  exit 1
fi
if [ -n "${GITHUB_TOKEN:-}" ]; then
  echo "FAIL: GITHUB_TOKEN should not be set during cargo build" >&2
  exit 1
fi
if [ -n "${SAVED_TOKEN:-}" ]; then
  echo "FAIL: SAVED_TOKEN should not be exported to cargo build" >&2
  exit 1
fi
# Log each argument on separate line to verify spaces preserved
for arg in "$@"; do
  echo "$arg" >> "$CARGO_LOG"
done
exit 0
EOF
chmod +x "$FAKE_CARGO_BIN/cargo"

# Fake built binary that records env and args
FAKE_BINARY="$TEST_DIR/fake-binary"
cat > "$FAKE_BINARY" << 'EOF'
#!/usr/bin/env bash
if [ $# -ne 1 ]; then
  echo "FAIL: fake binary expects exactly 1 argument" >&2
  exit 1
fi
echo "BINARY_ARGS: $@" >> "$BINARY_LOG"
echo "BINARY_CWD: $(pwd)" >> "$BINARY_LOG"
echo "BINARY_TOKEN: ${GITHUB_TOKEN:-unset}" >> "$BINARY_LOG"
echo "BINARY_CONFIG: ${MR_MILCHICK_CONFIG_PATH:-unset}" >> "$BINARY_LOG"
exit 0
EOF
chmod +x "$FAKE_BINARY"

# Setup environment with spaces in paths
export GITHUB_ACTION_PATH="$FAKE_ACTION"
export RUNNER_TEMP="$TEST_DIR/temp dir"
mkdir -p "$RUNNER_TEMP"
export PATH="$FAKE_CARGO_BIN:$PATH"
export CARGO_LOG="$TEST_DIR/cargo.log"
export BINARY_LOG="$TEST_DIR/binary.log"

# Create consumer directory with spaces for cwd test
CONSUMER_DIR="$TEST_DIR/consumer dir"
mkdir -p "$CONSUMER_DIR"
cd "$CONSUMER_DIR"

# Test 1: Valid command observe
export INPUT_COMMAND="observe"
export INPUT_CONFIG_PATH="test config.toml"
export INPUT_TOKEN="secret-token-123"
export GITHUB_TOKEN='incoming-must-not-reach-build'
export SAVED_TOKEN='incoming-saved-must-not-reach-build'

# Create fake target dir structure to match expected binary path
mkdir -p "$RUNNER_TEMP/milchick-target/release"
cp "$FAKE_BINARY" "$RUNNER_TEMP/milchick-target/release/mr-milchick"

bash "$FAKE_ACTION/scripts/run_github_action.sh"

# Verify cargo args contain manifest, features, target-dir using grep --
grep -Fqx -- "--manifest-path" "$CARGO_LOG"
grep -Fqx -- "$GITHUB_ACTION_PATH/Cargo.toml" "$CARGO_LOG"
grep -Fqx -- "--features" "$CARGO_LOG"
grep -Fqx -- "github" "$CARGO_LOG"
grep -Fqx -- "--target-dir" "$CARGO_LOG"
grep -Fqx -- "$RUNNER_TEMP/milchick-target" "$CARGO_LOG"
grep -Fqx -- "--locked" "$CARGO_LOG"
grep -Fqx -- "--release" "$CARGO_LOG"
grep -Fqx -- "--no-default-features" "$CARGO_LOG"

# Verify binary received token and config with spaces intact
grep -Fqx -- "BINARY_TOKEN: secret-token-123" "$BINARY_LOG"
grep -Fqx -- "BINARY_CONFIG: test config.toml" "$BINARY_LOG"
grep -Fqx -- "BINARY_ARGS: observe" "$BINARY_LOG"

# Verify CWD equals consumer dir with spaces
grep -Fqx -- "BINARY_CWD: $CONSUMER_DIR" "$BINARY_LOG"

# Test 2: Invalid command rejected before build
rm -f "$CARGO_LOG" "$BINARY_LOG"
export INPUT_COMMAND="invalid-command"
if bash "$FAKE_ACTION/scripts/run_github_action.sh" 2>/dev/null; then
  echo "FAIL: invalid command should be rejected"
  exit 1
fi
# Verify no cargo build happened
if [ -f "$CARGO_LOG" ]; then
  echo "FAIL: cargo should not run for invalid command"
  exit 1
fi

# Test 3: Malicious command with semicolon rejected before build
rm -f "$CARGO_LOG" "$BINARY_LOG"
MARKER_FILE="$TEST_DIR/marker.txt"
export INPUT_COMMAND="refine; touch $MARKER_FILE"
if bash "$FAKE_ACTION/scripts/run_github_action.sh" 2>/dev/null; then
  echo "FAIL: malicious command should be rejected"
  exit 1
fi
# Verify marker file was not created
if [ -f "$MARKER_FILE" ]; then
  echo "FAIL: marker file should not exist after rejected command"
  exit 1
fi
# Verify no cargo build happened
if [ -f "$CARGO_LOG" ]; then
  echo "FAIL: cargo should not run for malicious command"
  exit 1
fi

# Test 4: Missing token fails before build
rm -f "$CARGO_LOG" "$BINARY_LOG"
export INPUT_COMMAND="refine"
unset INPUT_TOKEN
if bash "$FAKE_ACTION/scripts/run_github_action.sh" 2>/dev/null; then
  echo "FAIL: missing token should fail"
  exit 1
fi
# Verify no cargo build happened
if [ -f "$CARGO_LOG" ]; then
  echo "FAIL: cargo should not run when token is missing"
  exit 1
fi
export INPUT_TOKEN="secret-token-123"

# Test 5: Missing manifest fails before build
rm -f "$FAKE_ACTION/Cargo.toml"
rm -f "$CARGO_LOG" "$BINARY_LOG"
export INPUT_COMMAND="refine"
if bash "$FAKE_ACTION/scripts/run_github_action.sh" 2>/dev/null; then
  echo "FAIL: missing manifest should fail"
  exit 1
fi
# Verify no cargo build happened
if [ -f "$CARGO_LOG" ]; then
  echo "FAIL: cargo should not run when manifest is missing"
  exit 1
fi
touch "$FAKE_ACTION/Cargo.toml"

# Test 6: Exit status preserved (exit 7)
cat > "$RUNNER_TEMP/milchick-target/release/mr-milchick" << 'EOF'
#!/usr/bin/env bash
exit 7
EOF
chmod +x "$RUNNER_TEMP/milchick-target/release/mr-milchick"
export INPUT_COMMAND="refine"
ACTUAL_EXIT=0
if bash "$FAKE_ACTION/scripts/run_github_action.sh"; then
  echo "FAIL: exit status should be preserved"
  exit 1
else
  ACTUAL_EXIT=$?
fi
if [ "$ACTUAL_EXIT" -ne 7 ]; then
  echo "FAIL: expected exit 7, got $ACTUAL_EXIT"
  exit 1
fi

# Test 7: All supported command values work
for cmd in observe refine explain; do
  rm -f "$CARGO_LOG" "$BINARY_LOG"
  cp "$FAKE_BINARY" "$RUNNER_TEMP/milchick-target/release/mr-milchick"
  export INPUT_COMMAND="$cmd"
  bash "$FAKE_ACTION/scripts/run_github_action.sh"
  grep -Fqx -- "BINARY_ARGS: $cmd" "$BINARY_LOG"
done

echo "All shell tests passed"