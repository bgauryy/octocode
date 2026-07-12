# Results — rtk+gh vs Octocode CLI

**Latest (v2, 10Q, live):** [`rtk-gh-vs-octocode-flows-20260712T091818Z`](../../../output/rtk-gh-vs-octocode-flows-20260712T091818Z/)
— first full run of the v2 question set. rtk-gh sweeps 30/30 correctness;
octocode drops to 29/30 (1 FP on Q5, 1 thin-evidence 0.5 on Q8). Quality is a
near-wash (4.13 octocode vs 4.10 rtk-gh). Cost ranking **flips** vs. the v1
(6Q) run: octocode is now ~1.9x cheaper on tokens; rtk-gh wins wall-clock time
~3.3x — see that run's `results.md`/`reflection.md` for why (mainly a Q6 token
blowout on 2/3 rtk-gh agents plus a sandbox egress restriction on Q8).

Prior (v1, 6Q, superseded): [`rtk-gh-vs-octocode-flows-20260712T075326Z`](../../../output/rtk-gh-vs-octocode-flows-20260712T075326Z/) — not comparable to v2, kept for the question-freeze/fairness-incident case study in its `reflection.md`.
