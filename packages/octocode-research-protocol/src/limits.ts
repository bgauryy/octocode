/** Shared admission bounds for one Agent Sync research session. */
export const AGENT_SYNC_MAX_SESSION_MS = 15 * 60_000;

/** Permitted clock difference between the caller and Agent Sync. */
export const AGENT_SYNC_CLOCK_SKEW_TOLERANCE_MS = 5_000;

/**
 * Aggregate ceiling on one serialized ResearchContinuation. Per-field bounds
 * (48 claims x 16 evidenceRefs x 2048 chars, etc.) compose to a legal worst
 * case in the hundreds of thousands of tokens with no aggregate check —
 * this closes that gap without touching any individual field's bound.
 */
export const RESEARCH_CONTINUATION_MAX_SERIALIZED_CHARS = 80_000;
