#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

out="${FRANK_MUTANTS_OUTPUT:-$root/mutants.out}"
timeout="${FRANK_MUTATION_TIMEOUT:-120}"
minimum="${FRANK_MUTATION_MIN_SCORE:-85}"
allowlist="$root/mutants-equivalent.allowlist"

command -v cargo-mutants >/dev/null 2>&1 || {
  echo 'cargo-mutants is required for the mutation gate' >&2
  exit 2
}

# cargo-mutants owns the output directory lifecycle (including retaining the
# previous run as mutants.out.old), so the gate never deletes a caller-owned
# path. Keep the output in a generated, ignored directory and let the tool
# write its canonical caught/missed/timeout/unviable files there.
set +e
cargo mutants --workspace --timeout "$timeout" --output "$out"
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

# A missed or timed-out mutant is acceptable only when its exact rendered
# description is present in the reviewed allowlist. The allowlist is line
# based on purpose: a broad regex or package-wide exemption would hide new
# survivors and violate the reviewed-equivalent-mutant contract.
unreviewed=0
for file in "$out/missed.txt" "$out/timeout.txt"; do
  [[ -f "$file" ]] || continue
  while IFS= read -r mutant; do
    [[ -n "${mutant//[[:space:]]/}" ]] || continue
    if ! grep -Fqx -- "$mutant" "$allowlist"; then
      if (( unreviewed < 20 )); then
        printf 'unreviewed surviving mutant: %s\n' "$mutant" >&2
      fi
      unreviewed=$((unreviewed + 1))
    fi
  done < "$file"
done

if (( unreviewed > 0 )); then
  printf '%d surviving mutant(s) require a regression test or a specific allowlist entry\n' "$unreviewed" >&2
  exit 1
fi

if (( score < minimum )); then
  printf 'mutation score %d%% is below required %d%%\n' "$score" "$minimum" >&2
  exit 1
fi

# A baseline/build failure must still fail the gate even if an old output file
# happened to be present. Successful mutation runs return zero; the explicit
# check keeps a partial interrupted run from looking green.
if (( mutants_status != 0 )); then
  printf 'cargo-mutants exited with status %d\n' "$mutants_status" >&2
  exit "$mutants_status"
fi

printf 'mutation gate: passed\n'
