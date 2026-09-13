# Frozen agent prompt integrity

Load when multiple tasks, workers, agents, or requests must share an unchanged base prompt. Why: cache-friendly text can still drift semantically, and a claimed freeze needs an external invariant.

**Freeze application-owned base bytes; append task overlays.** A frozen base can change only through a new reviewed version, never through a per-agent rewrite.

## Release contract

For every released base, record:

- `base_prompt_id` and immutable version;
- canonical serialized bytes and cryptographic digest;
- authority/source revisions included in the base;
- model and provider configuration that can affect rendered instructions;
- ordered tool-catalog/schema and output-contract versions;
- evaluation suite/result and release timestamp.

Exclude timestamps, user data, retrieved evidence, task text, per-worker status, and secrets from the base. Put them in a typed overlay after the frozen prefix. A role specialization is a versioned overlay or a different base ID, not an in-place edit.

## Dispatch gate

1. Load the released base by ID/version; do not reconstruct it from prose fragments.
2. Canonicalize with the same serializer used at release and recompute its digest.
3. Compare the digest, ordered tool catalog, output schema, and behavior-affecting provider settings with the release manifest.
4. Append the task overlay without editing, summarizing, interpolating into, or reordering the base. The overlay may specialize allowed behavior but must not contradict or weaken higher-authority rules.
5. On any mismatch, stop dispatch and rebuild from the released artifact or open a new-version review. Do not ask the target agent to attest that its own prompt was unchanged.
6. Log base version/digest and overlay digest with the task result so workers remain comparable and incidents are reproducible.

Platform-injected hidden instructions cannot be hashed by the application. Scope the invariant to bytes and configuration the application controls, pin provider/model versions where reproducibility requires it, and use cache telemetry only as corroboration—a cache hit is not semantic proof.

## Change gate

When behavior must change, create a candidate base version, show a semantic and byte diff, rerun held-out behavior/security tests, then publish or revert. In-flight tasks stay on their recorded version unless an explicit migration policy authorizes restart. Never mutate a base merely to improve cache hits or squeeze under a context limit; use `references/context/context-budget.md` and `references/context/token-economics.md` to select a behavior-preserving lever.

## Sources

- OpenAI, [Prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching) — stable developer/tool prefixes, cache lineage, and prefix-preserving changes.
- Anthropic, [Tool use with prompt caching](https://platform.claude.com/docs/en/agents-and-tools/tool-use/tool-use-with-prompt-caching) — tool-definition invalidation and the `tools → system → messages` cache hierarchy.

Next: when defining the overlay/handoff load `references/agents/agent-communication.md`; when diagnosing prefix reuse load `references/context/prompt-caching.md`; when the tool catalog changed load `references/tools/tool-contracts.md` and `references/tools/contract-audit.md`; validate the candidate with `references/flow/evaluation-data.md`.
