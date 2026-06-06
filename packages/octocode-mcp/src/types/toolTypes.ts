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
