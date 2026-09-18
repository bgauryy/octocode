# Build, evaluate, verify, and apply

Load after deterministic routing selects a Jev route. Why: bind structured state to a validated request and turn advice into one concrete provisional action.

## Prepare
Retrieve exact evidence before construction: `localSearch` locates candidates, `localFetch` reopens excerpts, `astSearch` supplies structure, and `lspSearch` confirms symbol identity. External evidence uses pinned GitHub refs or dated URLs. Search snippets are locators, not deciding anchors.

Create builder input with `route`, `model`, `decisionBrief`, and route state. Use `assets/decision-brief.schema.json`; prefer stdin so the brief is ephemeral. Keep the generic brief minimal. On `hypothesis_triage`, every testable explanation needs predictions and `weakenedBy`, and every proposed check needs cost plus at least two outcomes whose effects distinguish named hypotheses. Omit prediction/falsifier fields on classificatory and claim-status routes when they add no information.
```sh
node scripts/build-decision-packet.mjs --input - > request.json
node scripts/validate-decision-packet.mjs --route <route> --input request.json
node scripts/jev.mjs evaluate --input request.json --dry-run
```
The builder rejects direct checks, inert calls, unknown IDs, missing precommitments on testable hypothesis routes, non-discriminating outcomes, missing `none`, invalid bases, empty grounding evidence, and deterministic scope mismatch. Output is exactly `model`, `state`, and `questions`.

## Evaluate and verify
For hunch, triage, review, reflection, and gate:
```sh
node scripts/jev.mjs evaluate --input request.json --retries 0 --timeout-ms 10000 > response.json
```
For disputed claims, pin a versioned model and use the bound guard:
```sh
node scripts/research.mjs --input request.json --retries 0 --timeout-ms 10000 > envelope.json
node scripts/check-research.mjs --request request.json --response envelope.json
```
Nonzero exit means no usable advice. Check resolved model, answer IDs, complete probabilities, uncertainty, and source scope. Reopen every selected original anchor. A selected hypothesis is worth testing; a selected basis is worth independently verifying. Neither is established fact.

## Apply
Create `actions.json`, mapping every question ID to a concrete caller-owned action, then run:
```sh
node scripts/apply-response.mjs --request request.json --response response.json \
  --actions actions.json --net-action "Run C1 against src/cache.ts and record both branches"
```
Exit 4 means policy blocked action: `none`, a non-check soft tie, ambiguous Noul, low grounding, or scope mismatch. Do not override it with prose. For close next-check probabilities, the record names a cheaper `policy_preference` instead of treating check cost as model truth; cost never changes Jev’s semantic selection. Keep `provisional: true`, selected ID, probability, resolved model, and exact next action.

## Route outcomes
- Hunch survives → name a competing explanation and triage; otherwise drop it.
- Triage → freeze the selected check's prediction, then execute one branchable check.
- Decision review → retrieve or redesign when viability fails, evidence is needed, or a supplied risk blocks.
- Reflection → compare the precommitment with the real observation, then update, abandon, or replace the deck; never repeat-vote on unchanged evidence.
- Claim check → inspect every evidence ID in the selected basis; retrieve for insufficient/conflicting.
- Hallucination gate → block on low grounding or `none`; narrow on scope mismatch; otherwise cite anchor and scope.

Next: use `references/context.md` for compaction, `references/protocol.md` for wire semantics, and `references/benchmark.md` before claiming outcome improvement.
