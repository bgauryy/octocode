#!/usr/bin/env bash
# Capture native stacks of one explicitly selected PID; never signal the target.
set -euo pipefail
usage() {
  echo 'Usage: bash native-capture.sh <pid> <seconds> <new-output-directory>'
  echo 'macOS: sample; Linux: perf record then perf report. Requires existing tools and attach permission.'
}
if [[ ${1:-} == --help ]]; then usage; exit 0; fi
if [[ $# != 3 || ! $1 =~ ^[1-9][0-9]*$ || ! $2 =~ ^[1-9][0-9]*$ ]]; then
  usage >&2; exit 2
fi
pid=$1
seconds=$2
out=$3
case $(uname -s) in
  Darwin) backend=sample ;;
  Linux) backend=perf ;;
  *) echo 'Unsupported platform; use its native profiler.' >&2; exit 2 ;;
esac
command -v "$backend" >/dev/null || { echo "Missing tool: $backend" >&2; exit 2; }
# Refuse reuse, avoiding accidental overwrites and mixed-run attribution.
umask 077
mkdir -- "$out"
ps -p "$pid" -o pid=,ppid=,rss=,%cpu=,etime=,comm= > "$out/process.txt" 2> "$out/profiler.log" || {
  echo "Cannot inspect PID $pid; see $out/profiler.log" >&2; exit 1
}
{
  printf 'backend=%s\npid=%s\ndurationSeconds=%s\n' "$backend" "$pid" "$seconds"
  printf 'startedAt='; date -u '+%Y-%m-%dT%H:%M:%SZ'
} > "$out/capture.txt"
if [[ $backend == sample ]]; then
  sample "$pid" "$seconds" -file "$out/stacks.txt" >> "$out/profiler.log" 2>&1
else
  perf record -p "$pid" --call-graph dwarf -o "$out/perf.data" -- sleep "$seconds" >> "$out/profiler.log" 2>&1
  perf report -i "$out/perf.data" --stdio --no-children > "$out/stacks.txt" 2>> "$out/profiler.log"
fi
printf 'Capture: %s\n' "$out"
