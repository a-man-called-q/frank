#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

# cargo-mutants' --output argument is a parent directory; the tool creates
# its canonical results in <parent>/mutants.out. Keep the parent and result
# paths separate so the gate reads the files produced by the current run.
output_parent="${FRANK_MUTANTS_OUTPUT:-$root/target/mutation}"
out="$output_parent/mutants.out"
timeout="${FRANK_MUTATION_TIMEOUT:-120}"
minimum=100
jobs="${FRANK_MUTATION_JOBS:-}"

command -v cargo-mutants >/dev/null 2>&1 || {
  echo 'cargo-mutants is required for the mutation gate' >&2
  exit 2
}

# cargo-mutants owns the output directory lifecycle (including retaining the
# previous run as mutants.out.old), so the gate never deletes a caller-owned
# path. Keep the output in a generated, ignored directory and let the tool
# write its canonical caught/missed/timeout/unviable files there.
mutation_scope=()
case "$(uname -s)" in
  Darwin|Linux)
    # The Windows backend is deliberately a placeholder until Windows CI
    # exists; mutating cfg(windows) source on Unix produces survivors that no
    # Unix test can execute.
    mutation_scope+=(--exclude '**/windows.rs')
    ;;
  MINGW*|MSYS*|CYGWIN*)
    mutation_scope+=(--exclude '**/unix.rs')
    ;;
esac

# crates/frank-gui is the platform shell -- tray/window lifecycle/
# single-instance glue that needs a real event loop and OS tray to exercise,
# covered by scripts/native-smoke.sh instead. Its own reducer/model logic
# lives in frank-gui-core and stays fully mutated below.
mutation_scope+=(--exclude 'crates/frank-gui/src/**')
# Widget layout/styling code (padding, spacing, which container wraps which)
# produces mutants no test can meaningfully distinguish from the original --
# see the plan's "Styling" note. reducer.rs/model.rs/message.rs/i18n.rs stay
# fully mutated.
mutation_scope+=(--exclude 'crates/frank-gui-core/src/pages/**')
mutation_scope+=(--exclude 'crates/frank-gui-core/src/view.rs')

mutation_command=(cargo mutants --workspace)
mutation_command+=("${mutation_scope[@]}")
if [[ -n "$jobs" ]]; then
  mutation_command+=(--jobs "$jobs")
fi

set +e
"${mutation_command[@]}" \
  --timeout "$timeout" --output "$output_parent"
mutants_status=$?
set -e

count_file() {
  local file="$1"
  [[ -f "$file" ]] || { echo 0; return; }
  awk 'NF { count++ } END { print count + 0 }' "$file"
}

caught="$(count_file "$out/caught.txt")"
missed="$(count_file "$out/missed.txt")"
timed_out="$(count_file "$out/timeout.txt")"
unviable="$(count_file "$out/unviable.txt")"
tested=$((caught + missed + timed_out))

if (( tested == 0 )); then
  echo 'mutation gate produced no testable mutants; refusing an empty result' >&2
  exit "${mutants_status:-1}"
fi

score=$((caught * 100 / tested))
printf 'mutation score: %d%% (%d caught, %d missed, %d timed out, %d unviable)\n' \
  "$score" "$caught" "$missed" "$timed_out" "$unviable"

if (( missed > 0 || timed_out > 0 )); then
  printf 'mutation gate requires zero missed and timed-out mutants\n' >&2
  for file in "$out/missed.txt" "$out/timeout.txt"; do
    [[ -f "$file" ]] || continue
    sed -n '1,20p' "$file" >&2
  done
  exit 1
fi

if (( score < minimum )); then
  printf 'mutation score %d%% is below required %d%%\n' "$score" "$minimum" >&2
  exit 1
fi

# A baseline/build failure must still fail the gate even if an old output file
# happened to be present. cargo-mutants returns 2 for missed mutants and 3 for
# timeouts; those statuses are rejected above. Other statuses still indicate
# an invalid run.
if (( mutants_status != 0 && mutants_status != 2 && mutants_status != 3 )); then
  printf 'cargo-mutants exited with status %d\n' "$mutants_status" >&2
  exit "$mutants_status"
fi

printf 'mutation gate: passed\n'
