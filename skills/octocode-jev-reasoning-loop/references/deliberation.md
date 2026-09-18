# Decision brief

Load when genuine semantic uncertainty remains before a Jev route. Why: give the host a short decision checkpoint without serializing a private-reasoning transcript.

## Minimal checkpoint

Always answer briefly:

1. **OBSERVE** — What is directly known, and where is it anchored?
2. **UNCERTAINTY** — What unresolved semantic decision changes the next action?
3. **DIRECT-CHECK** — Can a cheap deterministic lookup or test answer it now?
4. **STOP** — Can Jev's answer actually change the next action?

Add only fields that clarify the selected route:

- **INFER / ASSUME / COUNTER** for an inference whose premises or alternatives matter.
- **PREDICT / FALSIFY / DISCRIMINATE** for an empirical hypothesis check. Before observing the result, state what each hypothesis predicts, what weakens it, and which outcome separates the deck.
- Omit predictions and falsifiers for classificatory or claim-status judgments when they would be artificial.

Normal use puts any needed fields under `reasoning` in `assets/run-loop-input.schema.json`; the runner derives minimal observations and uncertainty when omitted. Do not record hidden chain-of-thought, private scratch, or a transcript. For low-level debugging only, `assets/decision-brief.schema.json` and `scripts/build-decision-packet.mjs` expose the derived intermediate contract. Both paths reject direct checks and inert calls and emit exactly `model + state + questions`.

## Stop deliberating and act

Stop when a deterministic lookup resolves the uncertainty, one discriminating check is already clear, more thought cannot change the next action, evidence directly resolves the claim, or remaining uncertainty does not affect the user goal. Retrieve or execute instead of asking Jev to endorse an obvious action.

Next: select the route through `references/routing-policy.md`; after material new evidence use the reflection-delta route rather than repeating the same packet.
