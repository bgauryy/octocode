/**
 * Tool Type Utilities
 *
 * Provides type-safe patterns for MCP tool registration that avoid
 * TypeScript's exponential type inference when combining complex Zod schemas
 * with MCP SDK's Zod v3/v4 compatibility layer.
 *
 * @see .octocode/research/type-recursion/research.md for background
 */

import type { AnySchema } from '@modelcontextprotocol/sdk/server/zod-compat.js';

/**
 * Casts a Zod schema to MCP's AnySchema for tool registration.
 * MCP SDK expects Zod v3/v4 compatible schemas; this centralizes the cast
 * instead of scattering schema compatibility casts across tool registrations.
 */
export function toMCPSchema<T extends object>(schema: T): AnySchema {
  return schema as AnySchema;
}
