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

export function toMCPSchema<T extends object>(schema: T): AnySchema {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let s: any = schema;
  // Zod v4 uses `_zod.def`; v3 uses `_def`. Walking these private fields unwraps pipe/effects
  // wrappers that cause exponential type inference in the SDK's Zod compat layer.
  // If this breaks after a Zod upgrade, check the research doc linked in the file header.
  while (s?._zod?.def?.type === 'pipe') {
    s = s._zod.def.out;
  }
  if (
    s?._def?.typeName === 'ZodEffects' ||
    s?._def?.typeName === 'ZodPipeline'
  ) {
    s = s._def.schema ?? s._def.in ?? schema;
  }
  return (s ?? schema) as AnySchema;
}
