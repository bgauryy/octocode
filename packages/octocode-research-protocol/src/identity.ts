import { z } from "zod/v3";

export const RESEARCH_PROTOCOL_VERSION = "1" as const;
export const AGENT_PROTOCOL_IDENTIFIER_MAX_CHARS = 128;

/** Opaque durable id (task, message, session, ...); the protocol never parses its shape. */
export const AgentProtocolIdentifierSchema = z.string().trim().min(1).max(
  AGENT_PROTOCOL_IDENTIFIER_MAX_CHARS,
).describe("Opaque durable identifier; callers must not parse or derive meaning from its shape.");
export const AgentProtocolTimestampSchema = z.string().datetime({ offset: true }).describe(
  "ISO-8601 timestamp with an explicit offset (never a bare 'Z'-less local time).",
);

export const AgentIdentitySchema = z.string().min(1).max(60).regex(
  /^[a-z][a-z0-9_-]*$/,
  "agent identity must be a lowercase identifier (letters, digits, _ or -)",
).describe(
  "A deployment-configured routing label (e.g. \"researcher\") — not an authentication claim.",
);
export type AgentIdentity = z.infer<typeof AgentIdentitySchema>;

/** Shared across every ack schema — who performed the acknowledging action, if the runtime tracks it. */
export const AgentAckAuthorSchema = AgentIdentitySchema.optional().describe(
  "Who performed this action, if tracked — needed for mesh attribution.",
);

/**
 * Deliberately distinct from AgentIdentitySchema: a lane is a suggested next
 * capability/queue, not necessarily a single agent's routing label, so it
 * permits "." and ":" as structural separators (e.g. "octocode.search",
 * "repo:search") that an agent identity does not need.
 */
export const AgentLaneIdentifierSchema = z.string().min(1).max(80).regex(
  /^[a-z][a-z0-9_.:-]*$/,
  "lane identifier must be a lowercase identifier (letters, digits, _, -, ., or : as separators)",
).describe(
  "A deployment-defined next capability/queue name to route to — distinct from a single agent identity.",
);
export type AgentLaneIdentifier = z.infer<typeof AgentLaneIdentifierSchema>;
