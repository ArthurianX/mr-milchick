#!/usr/bin/env bash

set -euo pipefail

target="x86_64-unknown-linux-musl"
cargo_args=()
feature_args=()

while (($# > 0)); do
  case "$1" in
    --features)
      shift

      if (($# == 0)); then
        echo "error: --features requires at least one feature name" >&2
        exit 1
      fi

      while (($# > 0)) && [[ "$1" != --* ]]; do
        feature_args+=("$1")
        shift
      done
      ;;
    --features=*)
      feature_args+=("${1#--features=}")
      shift
      ;;
    *)
      cargo_args+=("$1")
      shift
      ;;
  esac
done

if ((${#feature_args[@]} > 0)); then
  # Cargo expects a single argument after --features; allow callers to pass
  # feature names as separate shell arguments for convenience.
  cargo_args+=(--features "${feature_args[*]}")
fi

if ! command -v rustup >/dev/null 2>&1; then
  echo "error: rustup is required to install the ${target} standard library" >&2
  exit 1
fi

if ! rustup target list --installed | grep -qx "${target}"; then
  echo "info: installing Rust target ${target}" >&2
  rustup target add "${target}"
fi

if command -v x86_64-linux-musl-gcc >/dev/null 2>&1 \
  && command -v x86_64-linux-musl-g++ >/dev/null 2>&1; then
  export CC_x86_64_unknown_linux_musl="${CC_x86_64_unknown_linux_musl:-x86_64-linux-musl-gcc}"
  export CXX_x86_64_unknown_linux_musl="${CXX_x86_64_unknown_linux_musl:-x86_64-linux-musl-g++}"
  cargo build --release --locked --target "${target}" "${cargo_args[@]}"
  exit 0
fi

if command -v x86_64-linux-musl-gcc >/dev/null 2>&1; then
  cat >&2 <<'EOF'
error: found x86_64-linux-musl-gcc but not x86_64-linux-musl-g++

This release path now includes native C++ code through llama.cpp, so a musl C
compiler alone is no longer enough. Install a musl toolchain that provides both
x86_64-linux-musl-gcc and x86_64-linux-musl-g++, or use cross / cargo-zigbuild.
EOF
fi

if command -v cross >/dev/null 2>&1; then
  cross build --release --locked --target "${target}" "${cargo_args[@]}"
  exit 0
fi

if command -v cargo-zigbuild >/dev/null 2>&1 && command -v zig >/dev/null 2>&1; then
  cargo zigbuild --release --locked --target "${target}" "${cargo_args[@]}"
  exit 0
fi

cat >&2 <<'EOF'
error: unable to build x86_64-unknown-linux-musl on this host yet

Milchick can cross-compile to Linux musl, but this host is missing a compatible C toolchain.
Install one of the following, then rerun this script:

- a musl cross-compiler that provides both x86_64-linux-musl-gcc and x86_64-linux-musl-g++
- cross
- cargo-zigbuild together with zig

Example:
  ./scripts/build_linux_release.sh --no-default-features --features gitlab slack-app llm-local
EOF

exit 1
