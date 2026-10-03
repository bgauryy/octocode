#!/bin/sh
# follow.sh <task> <tool> <startStep> : replays data.next.nextPage.query verbatim from the last output until none
task=$1; tool=$2; step=$3
dir=$(cd "$(dirname "$0")" && pwd)
while :; do
  prev="$dir/out/$task.oc.$step.txt"
  q=$(sed '/^--- STDERR ---$/,$d' "$prev" | jq -c '.results[0].data.next.nextPage.query // empty')
  [ -z "$q" ] && break
  step=$((step+1))
  payload=$(jq -cn --argjson q "$q" '{queries:[$q]}')
  printf '%s' "$payload" > "$dir/out/$task.oc.$step.query.json"
  SHOW=0 node "$dir/run.mjs" "$task" oc "$step" "octocode $tool \"\$(cat $dir/out/$task.oc.$step.query.json)\""
done
