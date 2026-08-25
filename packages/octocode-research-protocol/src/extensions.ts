import { z } from "zod/v3";

export const AGENT_EXTENSIONS_LIMITS = Object.freeze({
  keys: 16,
  keyChars: 64,
  valueChars: 2_000,
});

/**
 * Escape hatch for deployment-specific data this protocol doesn't standardize
 * — mirrors A2A's `extensions` array and MCP's `experimental` capabilities,
 * both real precedent from protocols with actual cross-org adoption. Every
 * envelope in this package is `.strict()`, which is valuable (it catches
 * typos and malformed messages on the fields the protocol DOES own) but
 * without this, a consumer can't attach so much as a trace id without the
 * entire object being rejected. Values are bounded strings, not arbitrary
 * JSON, so this stays inspectable and can't smuggle a second protocol inside
 * the first.
 */
export const AgentExtensionsSchema = z.record(
  z.string().trim().min(1).max(AGENT_EXTENSIONS_LIMITS.keyChars).regex(
    /^[a-z][a-z0-9_.]*$/,
    "extension key must be a lowercase snake_case or dotted identifier",
  ),
  z.string().max(AGENT_EXTENSIONS_LIMITS.valueChars),
).refine(
  (extensions) => Object.keys(extensions).length <= AGENT_EXTENSIONS_LIMITS.keys,
  `at most ${AGENT_EXTENSIONS_LIMITS.keys} extension keys`,
).describe(
  "Deployment-specific data this protocol doesn't standardize (e.g. an internal trace id) — "
  + "never a place to smuggle protocol-shaped fields the schema should own instead.",
);
export type AgentExtensions = z.infer<typeof AgentExtensionsSchema>;
