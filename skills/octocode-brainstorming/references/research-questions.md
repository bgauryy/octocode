Load when writing a research question for yourself or a worker, or a plan row for `research-tree.mjs`.

# Research questions and the research plan

## Research question

A research question is useful when its answer can change the recommendation and the searcher can tell when it is answered. Fill every slot, whether you search yourself or brief a worker; a worker sees nothing else of the conversation.

| Slot | Content | Example |
|---|---|---|
| Decision | The choice this answer feeds | Queue for a 5k msg/s job pipeline |
| Claim under test | One falsifiable sentence | Redis Streams keeps p99 under 50 ms at 5k msg/s |
| Confirms / refutes | The observation that settles it each way | A benchmark at 5k msg/s or more reporting p99; an incident report of lost or delayed messages |
| Context | Constraints, versions, scale, date window, out of scope | Redis 7.2+, one region, since 2024-01-01; managed services out of scope |
| Already known | Sources read and options ruled out; do not repeat them | Redis docs on consumer groups; Kafka ruled out for ops cost |
| Sources wanted | Primary types to prefer | Official docs, benchmarks that state their method, incident reports |
| Queries | At least one supporting and one disconfirming query ([wording](research.md#write-queries)) | `redis streams p99 latency benchmark`; `redis streams message loss postmortem` |
| Output | What to return | Ledger path, or findings with exact URL and verbatim quote |

- One claim per question: split "is X fast and cheap" into two.
- Ask for an observation ("what p99 did X measure at 5k msg/s?"), not an opinion ("is X good?").
- Merge questions one source settles; drop a question whose answer would leave the recommendation unchanged.

## Research plan

The plan is the research tree as rows: each row is one claim, a leaf of decision → sub-question → claim → queries, with the slots above. `node <skill-folder>/scripts/research-tree.mjs --help` lists the fields; one row:

```text
{"id":"C1","question":"Does Redis Streams meet our latency target?","claim":"Redis Streams keeps p99 under 50 ms at 5k msg/s","rival":"Kafka keeps p99 lower at the same rate","refutedBy":"A benchmark at 5k msg/s or more reporting p99 over 50 ms","worker":"W1","queries":[{"direction":"supports","surface":"web","text":"redis streams p99 latency benchmark"},{"direction":"refutes","surface":"github","text":"redis streams latency regression"}]}
```

A worker gets its rows (`workers[]` in the script's output) as its research questions.
