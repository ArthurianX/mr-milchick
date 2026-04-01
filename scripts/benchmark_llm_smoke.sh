#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/benchmark_llm_smoke.sh [options]

Benchmark each local GGUF model under test-models against the ignored
llm-local smoke tests and emit a repeatable scoreboard.

Options:
  --models-dir DIR         Directory to scan recursively for *.gguf files.
                           Default: test-models
  --output-dir DIR         Directory for logs and reports.
                           Default: /tmp/mr-milchick-llm-bench-<timestamp>
  --smoke-timeout-ms MS    Override MR_MILCHICK_LLM_SMOKE_TIMEOUT_MS.
                           Default: 120000
  --patch-budget BYTES     Override MR_MILCHICK_LLM_SMOKE_MAX_PATCH_BYTES.
  --tmpdir DIR             TMPDIR for cargo/llama.cpp runs. Default: $TMPDIR or /tmp
  -h, --help               Show this help text.

Outputs:
  <output-dir>/summary.csv
  <output-dir>/summary.md
  <output-dir>/logs/<model>/<case>.log

Reaction-case logs include `--nocapture`, so passing runs preserve the
reported summary and recommendations for qualitative comparison.
EOF
}

models_dir="test-models"
output_dir=""
smoke_timeout_ms="${MR_MILCHICK_LLM_SMOKE_TIMEOUT_MS:-120000}"
patch_budget="${MR_MILCHICK_LLM_SMOKE_MAX_PATCH_BYTES:-}"
tmpdir="${TMPDIR:-/tmp}"
field_delim=$'\034'

while [[ $# -gt 0 ]]; do
  case "$1" in
    --models-dir)
      models_dir="$2"
      shift 2
      ;;
    --output-dir)
      output_dir="$2"
      shift 2
      ;;
    --smoke-timeout-ms)
      smoke_timeout_ms="$2"
      shift 2
      ;;
    --patch-budget)
      patch_budget="$2"
      shift 2
      ;;
    --tmpdir)
      tmpdir="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ -z "$output_dir" ]]; then
  output_dir="${tmpdir%/}/mr-milchick-llm-bench-$(date +%Y%m%d-%H%M%S)"
fi

if [[ ! -d "$models_dir" ]]; then
  echo "Models directory does not exist: $models_dir" >&2
  exit 1
fi

mkdir -p "$output_dir/logs"

models=()
while IFS= read -r model_path; do
  models+=("$model_path")
done < <(find "$models_dir" -type f -name '*.gguf' | sort)

if [[ ${#models[@]} -eq 0 ]]; then
  echo "No GGUF files found under $models_dir" >&2
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required to run the benchmark" >&2
  exit 1
fi

sanitize_slug() {
  printf '%s' "$1" | tr '/ ' '__' | tr -cs 'A-Za-z0-9._-' '_'
}

csv_escape() {
  local value=${1//$'\n'/ }
  value=${value//\"/\"\"}
  printf '"%s"' "$value"
}

extract_real_seconds() {
  local logfile=$1
  awk '/^real / { gsub(/,/, ".", $2); print $2 }' "$logfile" | tail -n 1
}

extract_test_result() {
  local logfile=$1
  local test_name=$2
  local result
  result=$(awk -v test_name="$test_name" '
    index($0, test_name) && $0 ~ / \.\.\. (ok|FAILED)$/ {
      print $NF
    }
  ' "$logfile" | tail -n 1)

  if [[ -n "$result" ]]; then
    printf '%s\n' "$result"
    return
  fi

  if awk '/^test result: ok\./ { found = 1 } END { exit(found ? 0 : 1) }' "$logfile"; then
    printf 'ok\n'
  elif awk '/^test result: FAILED\./ { found = 1 } END { exit(found ? 0 : 1) }' "$logfile"; then
    printf 'FAILED\n'
  fi
}

extract_case_status() {
  local logfile=$1
  local case_name=$2
  awk -v case_name="$case_name" '
    index($0, "case=" case_name " status=") {
      split($0, parts, "status=")
      print parts[2]
    }
  ' "$logfile" | tail -n 1
}

extract_case_detail() {
  local logfile=$1
  local case_name=$2
  awk -v case_name="$case_name" '
    index($0, "case=" case_name " detail=") {
      sub(/^.* detail=/, "", $0)
      print $0
    }
  ' "$logfile" | tail -n 1
}

extract_case_summary() {
  local logfile=$1
  local case_name=$2
  awk -v case_name="$case_name" '
    index($0, "case=" case_name " summary=") {
      sub(/^.* summary=/, "", $0)
      print $0
    }
  ' "$logfile" | tail -n 1
}

extract_case_recommendations() {
  local logfile=$1
  local case_name=$2
  awk -v case_name="$case_name" '
    index($0, "case=" case_name " recommendation[") {
      sub(/^.* recommendation\[[^]]+\]=/, "", $0)
      recommendations[++count] = $0
    }
    END {
      for (idx = 1; idx <= count; idx++) {
        if (idx > 1) {
          printf " || "
        }
        printf "%s", recommendations[idx]
      }
      if (count > 0) {
        printf "\n"
      }
    }
  ' "$logfile" | tail -n 1
}

has_duplicate_recommendations() {
  local recommendations=$1
  [[ -z "${recommendations//[[:space:]]/}" ]] && return 1

  awk -v input="$recommendations" '
    BEGIN {
      count = split(input, parts, / \|\| /)
      for (idx = 1; idx <= count; idx++) {
        normalized = tolower(parts[idx])
        gsub(/[^[:alnum:]]/, "", normalized)
        if (normalized == "") {
          continue
        }
        if (seen[normalized]++) {
          found = 1
        }
      }
      exit(found ? 0 : 1)
    }
  '
}

count_recommendations() {
  local recommendations=$1
  [[ -z "${recommendations//[[:space:]]/}" ]] && {
    printf '0\n'
    return
  }

  awk -v input="$recommendations" '
    BEGIN {
      count = split(input, parts, / \|\| /)
      for (idx = 1; idx <= count; idx++) {
        if (parts[idx] ~ /[[:alnum:]]/) {
          total++
        }
      }
      print total + 0
    }
  '
}

summary_repeats_recommendation() {
  local summary=$1
  local recommendations=$2
  [[ -z "${summary//[[:space:]]/}" || -z "${recommendations//[[:space:]]/}" ]] && return 1

  awk -v summary="$summary" -v recommendations="$recommendations" '
    function normalize(value) {
      value = tolower(value)
      gsub(/[^[:alnum:]]/, "", value)
      return value
    }
    BEGIN {
      normalized_summary = normalize(summary)
      if (normalized_summary == "") {
        exit 1
      }

      count = split(recommendations, parts, / \|\| /)
      for (idx = 1; idx <= count; idx++) {
        normalized_recommendation = normalize(parts[idx])
        if (normalized_recommendation == "") {
          continue
        }
        if (normalized_summary == normalized_recommendation) {
          exit 0
        }
      }

      exit 1
    }
  '
}

contains_generic_filler() {
  local summary=$1
  local recommendations=$2
  local lower_text
  lower_text=$(printf '%s\n%s\n' "$summary" "$recommendations" | tr '[:upper:]' '[:lower:]')

  if printf '%s' "$lower_text" | grep -Fq "may pose security risks" \
    || printf '%s' "$lower_text" | grep -Fq "could lead to regressions" \
    || printf '%s' "$lower_text" | grep -Fq "suggests that the route is no longer being tested" \
    || printf '%s' "$lower_text" | grep -Fq "indicate a risk" \
    || printf '%s' "$lower_text" | grep -Fq "as per diff"; then
    return 0
  fi

  return 1
}

collect_case_quality_flags() {
  local result=$1
  local status=$2
  local detail=$3
  local summary=$4
  local recommendations=$5
  local flags=()
  local lower_text
  local recommendation_count

  lower_text=$(printf '%s\n%s\n%s\n' "$detail" "$summary" "$recommendations" | tr '[:upper:]' '[:lower:]')

  if [[ -z "${summary//[[:space:]]/}" ]]; then
    flags+=("empty-summary")
  fi

  if [[ -z "${recommendations//[[:space:]]/}" ]]; then
    flags+=("empty-recommendations")
  fi

  if printf '%s' "$lower_text" | grep -Fq "potential review cues from the diff" \
    || printf '%s' "$lower_text" | grep -Fq "the first character of your reply must be" \
    || printf '%s' "$lower_text" | grep -Fq "if the snapshot includes deleted tests"; then
    flags+=("prompt-parroting")
  fi

  if has_duplicate_recommendations "$recommendations"; then
    flags+=("duplicate-recommendations")
  fi

  recommendation_count=$(count_recommendations "$recommendations")
  if (( recommendation_count > 0 && recommendation_count < 2 )); then
    flags+=("too-few-recommendations")
  fi

  if summary_repeats_recommendation "$summary" "$recommendations"; then
    flags+=("summary-repeats-recommendation")
  fi

  if contains_generic_filler "$summary" "$recommendations"; then
    flags+=("generic-wording")
  fi

  if [[ "${result:-}" != "ok" && "${status:-}" == "Ready" ]]; then
    flags+=("structured-but-failed")
  fi

  if printf '%s' "$lower_text" | grep -Fq "did not return a json object"; then
    flags+=("protocol-failure")
  fi

  if [[ ${#flags[@]} -eq 0 ]]; then
    printf '\n'
  else
    local joined=""
    local flag
    for flag in "${flags[@]}"; do
      if [[ -n "$joined" ]]; then
        joined="$joined,$flag"
      else
        joined="$flag"
      fi
    done
    printf '%s\n' "$joined"
  fi
}

count_quality_flags() {
  local flags=$1
  [[ -z "${flags//[[:space:]]/}" ]] && {
    printf '0\n'
    return
  }

  awk -v input="$flags" '
    BEGIN {
      count = split(input, parts, /,/)
      for (idx = 1; idx <= count; idx++) {
        if (length(parts[idx]) > 0) {
          total++
        }
      }
      print total + 0
    }
  '
}

prefix_quality_flags() {
  local prefix=$1
  local flags=$2
  [[ -z "${flags//[[:space:]]/}" ]] && {
    printf '\n'
    return
  }

  awk -v prefix="$prefix" -v input="$flags" '
    BEGIN {
      count = split(input, parts, /,/)
      for (idx = 1; idx <= count; idx++) {
        if (length(parts[idx]) == 0) {
          continue
        }
        if (printed++ > 0) {
          printf "; "
        }
        printf "%s:%s", prefix, parts[idx]
      }
      if (printed > 0) {
        printf "\n"
      }
    }
  '
}

combine_quality_flags() {
  local js_flags=$1
  local ts_flags=$2
  local combined=""
  local prefixed

  prefixed=$(prefix_quality_flags "js" "$js_flags")
  if [[ -n "$prefixed" ]]; then
    combined="$prefixed"
  fi

  prefixed=$(prefix_quality_flags "ts" "$ts_flags")
  if [[ -n "$prefixed" ]]; then
    if [[ -n "$combined" ]]; then
      combined="$combined; $prefixed"
    else
      combined="$prefixed"
    fi
  fi

  printf '%s\n' "$combined"
}

run_case() {
  local model_path=$1
  local filter_name=$2
  local logfile=$3
  local capture_output=${4:-false}

  local -a env_args=(
    "TMPDIR=$tmpdir"
    "MR_MILCHICK_LLM_MODEL_PATH=$model_path"
    "MR_MILCHICK_LLM_SMOKE_TIMEOUT_MS=$smoke_timeout_ms"
  )

  if [[ -n "$patch_budget" ]]; then
    env_args+=("MR_MILCHICK_LLM_SMOKE_MAX_PATCH_BYTES=$patch_budget")
  fi

  set +e
  /usr/bin/time -p \
    env "${env_args[@]}" \
    cargo test \
      --features llm-local \
      --test llm_local_smoke \
      "$filter_name" \
      -- \
      --ignored \
      --test-threads=1 \
      $( [[ "$capture_output" == "true" ]] && printf '%s' "--nocapture" ) \
      >"$logfile" 2>&1
  local rc=$?
  set -e
  return "$rc"
}

results_tsv="$output_dir/results.tsv"
: >"$results_tsv"

for model_path in "${models[@]}"; do
  rel_model=${model_path#./}
  model_slug=$(sanitize_slug "$rel_model")
  model_log_dir="$output_dir/logs/$model_slug"
  mkdir -p "$model_log_dir"

  echo "Benchmarking $rel_model"

  backend_log="$model_log_dir/backend_load_and_context_probe.log"
  js_log="$model_log_dir/reacts_to_javascript_backend_changes.log"
  ts_log="$model_log_dir/reacts_to_typescript_frontend_changes.log"

  run_case "$model_path" "backend_load_and_context_probe" "$backend_log" false || true
  run_case "$model_path" "reacts_to_javascript_backend_changes" "$js_log" true || true
  run_case "$model_path" "reacts_to_typescript_frontend_changes" "$ts_log" true || true

  backend_result=$(extract_test_result "$backend_log" "test llm_local_smoke::backend_load_and_context_probe")
  js_result=$(extract_test_result "$js_log" "test llm_local_smoke::reacts_to_javascript_backend_changes")
  ts_result=$(extract_test_result "$ts_log" "test llm_local_smoke::reacts_to_typescript_frontend_changes")

  backend_seconds=$(extract_real_seconds "$backend_log")
  js_seconds=$(extract_real_seconds "$js_log")
  ts_seconds=$(extract_real_seconds "$ts_log")

  if [[ "$js_result" == "ok" ]]; then
    js_status="Ready"
  else
    js_status=$(extract_case_status "$js_log" "javascript-backend")
  fi

  if [[ "$ts_result" == "ok" ]]; then
    ts_status="Ready"
  else
    ts_status=$(extract_case_status "$ts_log" "typescript-frontend")
  fi

  js_detail=$(extract_case_detail "$js_log" "javascript-backend")
  ts_detail=$(extract_case_detail "$ts_log" "typescript-frontend")
  js_summary=$(extract_case_summary "$js_log" "javascript-backend")
  ts_summary=$(extract_case_summary "$ts_log" "typescript-frontend")
  js_recommendations=$(extract_case_recommendations "$js_log" "javascript-backend")
  ts_recommendations=$(extract_case_recommendations "$ts_log" "typescript-frontend")

  js_quality_flags=$(collect_case_quality_flags "$js_result" "${js_status:-}" "${js_detail:-}" "${js_summary:-}" "${js_recommendations:-}")
  ts_quality_flags=$(collect_case_quality_flags "$ts_result" "${ts_status:-}" "${ts_detail:-}" "${ts_summary:-}" "${ts_recommendations:-}")
  quality_penalty=$(( $(count_quality_flags "$js_quality_flags") + $(count_quality_flags "$ts_quality_flags") ))
  quality_flags=$(combine_quality_flags "$js_quality_flags" "$ts_quality_flags")

  reaction_score=0
  [[ "$js_result" == "ok" ]] && reaction_score=$((reaction_score + 1))
  [[ "$ts_result" == "ok" ]] && reaction_score=$((reaction_score + 1))

  total_score=0
  [[ "$backend_result" == "ok" ]] && total_score=$((total_score + 1))
  total_score=$((total_score + reaction_score))

  total_seconds=$(awk -v a="${backend_seconds:-0}" -v b="${js_seconds:-0}" -v c="${ts_seconds:-0}" 'BEGIN { printf "%.2f", a + b + c }')

  {
    printf '%s%s' "$reaction_score" "$field_delim"
    printf '%s%s' "$total_score" "$field_delim"
    printf '%s%s' "$quality_penalty" "$field_delim"
    printf '%s%s' "$total_seconds" "$field_delim"
    printf '%s%s' "$rel_model" "$field_delim"
    printf '%s%s' "${backend_result:-n/a}" "$field_delim"
    printf '%s%s' "${backend_seconds:-n/a}" "$field_delim"
    printf '%s%s' "${js_result:-n/a}" "$field_delim"
    printf '%s%s' "${js_status:-n/a}" "$field_delim"
    printf '%s%s' "${js_seconds:-n/a}" "$field_delim"
    printf '%s%s' "${js_detail:-}" "$field_delim"
    printf '%s%s' "${js_summary:-}" "$field_delim"
    printf '%s%s' "${js_recommendations:-}" "$field_delim"
    printf '%s%s' "${ts_result:-n/a}" "$field_delim"
    printf '%s%s' "${ts_status:-n/a}" "$field_delim"
    printf '%s%s' "${ts_seconds:-n/a}" "$field_delim"
    printf '%s%s' "${ts_detail:-}" "$field_delim"
    printf '%s%s' "${ts_summary:-}" "$field_delim"
    printf '%s%s' "${ts_recommendations:-}" "$field_delim"
    printf '%s%s' "${quality_flags:-}" "$field_delim"
    printf '%s\n' "$model_log_dir"
  } >>"$results_tsv"
done

sorted_tsv="$output_dir/results.sorted.tsv"
sort -t "$field_delim" -k1,1nr -k2,2nr -k3,3n -k4,4g -k5,5 "$results_tsv" >"$sorted_tsv"

csv_file="$output_dir/summary.csv"
md_file="$output_dir/summary.md"

{
  printf 'model_path,reaction_score,total_score,quality_penalty,total_seconds,backend_result,backend_seconds,javascript_result,javascript_status,javascript_seconds,javascript_detail,javascript_summary,javascript_recommendations,typescript_result,typescript_status,typescript_seconds,typescript_detail,typescript_summary,typescript_recommendations,quality_flags,log_dir\n'
  while IFS="$field_delim" read -r reaction_score total_score quality_penalty total_seconds model_path backend_result backend_seconds js_result js_status js_seconds js_detail js_summary js_recommendations ts_result ts_status ts_seconds ts_detail ts_summary ts_recommendations quality_flags log_dir; do
    csv_escape "$model_path"; printf ','
    csv_escape "$reaction_score"; printf ','
    csv_escape "$total_score"; printf ','
    csv_escape "$quality_penalty"; printf ','
    csv_escape "$total_seconds"; printf ','
    csv_escape "$backend_result"; printf ','
    csv_escape "$backend_seconds"; printf ','
    csv_escape "$js_result"; printf ','
    csv_escape "$js_status"; printf ','
    csv_escape "$js_seconds"; printf ','
    csv_escape "$js_detail"; printf ','
    csv_escape "$js_summary"; printf ','
    csv_escape "$js_recommendations"; printf ','
    csv_escape "$ts_result"; printf ','
    csv_escape "$ts_status"; printf ','
    csv_escape "$ts_seconds"; printf ','
    csv_escape "$ts_detail"; printf ','
    csv_escape "$ts_summary"; printf ','
    csv_escape "$ts_recommendations"; printf ','
    csv_escape "$quality_flags"; printf ','
    csv_escape "$log_dir"
    printf '\n'
  done <"$sorted_tsv"
} >"$csv_file"

{
  printf '# LLM Local Smoke Benchmark\n\n'
  printf 'Generated at `%s`.\n\n' "$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  printf 'Command shape: one ignored smoke test per model, sequentially, with `--test-threads=1` for repeatability. Ranking prefers higher pass counts, then lower quality penalties, then lower total runtime. Quality penalties now apply to structured failures too, not just passing cases.\n\n'
  printf '| Rank | Model | Reaction Score | Total Score | Quality Penalty | Load | Load s | JS | JS status | JS s | TS | TS status | TS s | Total s |\n'
  printf '| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |\n'
  rank=1
  while IFS="$field_delim" read -r reaction_score total_score quality_penalty total_seconds model_path backend_result backend_seconds js_result js_status js_seconds js_detail js_summary js_recommendations ts_result ts_status ts_seconds ts_detail ts_summary ts_recommendations quality_flags log_dir; do
    printf '| %s | `%s` | %s/2 | %s/3 | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s |\n' \
      "$rank" \
      "$model_path" \
      "$reaction_score" \
      "$total_score" \
      "$quality_penalty" \
      "${backend_result:-n/a}" \
      "${backend_seconds:-n/a}" \
      "${js_result:-n/a}" \
      "${js_status:-n/a}" \
      "${js_seconds:-n/a}" \
      "${ts_result:-n/a}" \
      "${ts_status:-n/a}" \
      "${ts_seconds:-n/a}" \
      "$total_seconds"

    if [[ -n "$js_summary" || -n "$js_recommendations" || -n "$ts_summary" || -n "$ts_recommendations" || -n "$js_detail" || -n "$ts_detail" || -n "$quality_flags" ]]; then
      printf '|  | Notes |  |  | Flags: `%s` |  |  | `%s` |  |  | `%s` |  |  | Logs: `%s` |\n' \
        "${quality_flags:-none}" \
        "${js_detail:-}" \
        "${ts_detail:-}" \
        "$log_dir"
      printf '|  | JS output |  |  |  |  |  | Summary: `%s` |  |  | Recs: `%s` |  |  |  |\n' \
        "${js_summary:-}" \
        "${js_recommendations:-}"
      printf '|  | TS output |  |  |  |  |  | Summary: `%s` |  |  | Recs: `%s` |  |  |  |\n' \
        "${ts_summary:-}" \
        "${ts_recommendations:-}"
    else
      printf '|  | Logs |  |  |  |  |  |  |  |  |  |  |  | `%s` |\n' "$log_dir"
    fi

    rank=$((rank + 1))
  done <"$sorted_tsv"
} >"$md_file"

echo
echo "Benchmark complete."
echo "Markdown summary: $md_file"
echo "CSV summary:      $csv_file"
