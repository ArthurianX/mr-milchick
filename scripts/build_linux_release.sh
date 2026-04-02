#!/usr/bin/env bash

set -euo pipefail

target="x86_64-unknown-linux-musl"
cargo_args=()
feature_args=()

pick_tool() {
  for candidate in "$@"; do
    if command -v "$candidate" >/dev/null 2>&1; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done

  return 1
}

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

if command -v rustup >/dev/null 2>&1; then
  if ! rustup target list --installed | grep -qx "${target}"; then
    echo "info: installing Rust target ${target}" >&2
    rustup target add "${target}"
  fi
else
  echo "info: rustup not found; assuming ${target} support is already present" >&2
fi

cc_tool="$(pick_tool x86_64-linux-musl-gcc musl-gcc || true)"
cxx_tool="$(pick_tool x86_64-linux-musl-g++ musl-g++ || true)"
ar_tool="$(pick_tool x86_64-linux-musl-ar musl-ar || true)"

if [[ -n "${cc_tool}" && -n "${cxx_tool}" ]]; then
  export CC_x86_64_unknown_linux_musl="${CC_x86_64_unknown_linux_musl:-$cc_tool}"
  export CXX_x86_64_unknown_linux_musl="${CXX_x86_64_unknown_linux_musl:-$cxx_tool}"
  export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER="${CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER:-$cc_tool}"
  if [[ -n "${ar_tool}" ]]; then
    export AR_x86_64_unknown_linux_musl="${AR_x86_64_unknown_linux_musl:-$ar_tool}"
  fi
  cargo build --release --locked --target "${target}" "${cargo_args[@]}"
  exit 0
fi

if [[ -n "${cc_tool}" ]]; then
  cat >&2 <<'EOF'
error: found a musl C compiler but not a musl C++ compiler

This release path now includes native C++ code through llama.cpp, so a musl C
compiler alone is no longer enough. Install a musl toolchain that provides both
x86_64-linux-musl-gcc and x86_64-linux-musl-g++ (or musl-gcc and musl-g++), or
use cross / cargo-zigbuild.
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

- a musl cross-compiler that provides both x86_64-linux-musl-gcc and x86_64-linux-musl-g++ (or musl-gcc and musl-g++)
- cross
- cargo-zigbuild together with zig

Example:
  ./scripts/build_linux_release.sh --no-default-features --features gitlab slack-app llm-local
EOF

exit 1
