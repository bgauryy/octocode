Load when executing one package benchmark matchup after selecting and pinning its arms.

## Quickstart (copy-paste, one matchup, one pass)

```bash
cd packages/octocode-benchmark
export HR_PY="$HOME/.local/share/uv/tools/headroom-ai/bin/python"   # Headroom arm only

# Phase 0 — preflight (non-zero exit = fix before running)
bash skills/octocode-benchmark/scripts/check-prereqs.sh            # add a spec, e.g. octocode@18.2.2, to also check the published pin

# set up a campaign dir
CAMP="../../.octocode/octocode-benchmark/$(date +%F)-comparison/tmp/campaign"; mkdir -p "$CAMP/answers" "$CAMP/judge"

# Phase 1 — answer (spawn one isolated agent per question, arm and pass).
# Each research call sets its per-question log, e.g. octocode Q4:
OCTO_LOG="$CAMP/octocode-p1-Q4.jsonl" ./compare/bin/octoc ghGetFileContent \
  '{"queries":[{"owner":"axios","repo":"axios","path":"lib/adapters/http.js","matchString":["follow-redirects"]}]}'
# baseline (gh+RTK) Q4:
RTK_LOG="$CAMP/rtk-p1-Q4.jsonl" ./compare/bin/rtkm search code --repo axios/axios follow-redirects --limit 20
# log the final answer, then append a "## Q4" section (Answer + Research steps) to answers/<arm>-p1.md
python3 compare/bin/record_answer.py --log "$CAMP/octocode-p1-Q4.jsonl" --question Q4 --file answer.txt

# Phase 2 — judge: build the blind packet, then one reasoning-first verdict per question
python3 compare/bin/build_blind_packet.py --help          # X/Y randomized, tool identity redacted

# Phase 3 — validate + aggregate + report
python3 compare/bin/sumlog.py --strict "$CAMP/octocode-p1-Q4.jsonl"
python3 compare/bin/validate_campaign.py "$CAMP" --question-count 30
# Aggregate this campaign per aggregation-and-stats.md.
# per_question_summary.py regenerates fixed historical campaigns, not this new run.
```

Runner/judge briefing packets, dispatch scaling (fresh contexts per question within each arm), and output
layout: [run with agents](run-with-agents.md) → [preflight](run-preflight.md) + [phases](run-phases.md).

