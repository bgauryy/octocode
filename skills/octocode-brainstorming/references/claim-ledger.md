Load when writing or verifying a claim ledger, or marking a deciding claim's status on its rows.

# Claim ledger

Every claim the recommendation rests on goes in a ledger, one JSON object per line at the assigned path:

```json
{"claim":"<what the quote supports>","url":"<exact opened source URL>","quote":"<verbatim source text>"}
```

Copy `quote` from `node <skill-folder>/scripts/read-page.mjs --url <url> --grep "<phrase>"`: it prints the page text the verifier reads, so the quote matches. The verifier requires quotes of at least 20 characters; separate non-adjacent passages with `...`. Keep inference out of `claim`. Optional fields the verifier passes through and counts:

| Field | Values | Use |
|---|---|---|
| `id` | any | Cross-review references |
| `decides` | the deciding claim's or option's id, e.g. `O3` | Groups rows: the summary's `decidingClaims` gives per id the rows, verified rows, `distinctSources`, `supports`, `refutes` and statuses |
| `direction` | `supports`, `refutes` | Which way this row's quote cuts on that deciding claim |
| `claimStatus` | `proven`, `contradicted`, `unresolved` | The status of the deciding claim (not of the row) by the [proof bar](proof-bar.md#contradict-and-prove); give every row of one `decides` id the same value, and `conflictingStatus` flags when they differ. A refuting row on a claim the counterevidence defeats carries `contradicted` |
| `primary` | `true`, `false` | Original source or not; the summary counts it |
| `stake` | text | Who gains if the claim is believed; passed through, not counted |
| `hypothesis` | text | When that context changes interpretation |

Asset and manual-verification fields are in [verification](verification.md).

Run `node <skill-folder>/scripts/verify-claims.mjs --ledger <path> --out <path>.verified.jsonl [--since YYYY-MM-DD]`; it adds `verification` per row ([reruns, statuses and several ledgers](verification.md#run-the-verifier)). A nonzero exit leaves unresolved rows to inspect, not evidence to discard silently. The verifier proves the quote is on the page, not that it supports the claim: check that yourself ([validated sources](verification.md#validated-sources)).
