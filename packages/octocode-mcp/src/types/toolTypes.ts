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
 *
 * Unwraps any ZodPipe wrappers (created by z.preprocess, .transform, or
 * .pipe) to expose the inner ZodObject shape. The MCP SDK's
 * normalizeObjectSchema only recognises schemas whose def.type is "object";
 * a ZodPipe has def.type === "pipe" and no def.shape, so it falls through to
 * EMPTY_OBJECT_JSON_SCHEMA — making tools/list return { properties: {} }.
 *
 * Unwrapping the pipe here ensures the correct input schema is advertised in
 * tools/list while leaving runtime validation intact (the SDK validates
 * incoming calls with safeParseAsync on the original schema, which handles
 * ZodPipe correctly).
 */
export function toMCPSchema<T extends object>(schema: T): AnySchema {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let s: any = schema;
  // Unwrap Zod v4 pipe schemas until we reach the inner object schema.
  // _zod.def is Zod v4's internal structure; _def is the v3 compat layer.
  while (s?._zod?.def?.type === 'pipe') {
    s = s._zod.def.out;
  }
  // Fallback: Zod v3 compat layer uses _def.schema for ZodEffects (preprocess)
  if (
    s?._def?.typeName === 'ZodEffects' ||
    s?._def?.typeName === 'ZodPipeline'
  ) {
    s = s._def.schema ?? s._def.in ?? schema;
  }
  return (s ?? schema) as AnySchema;
}
