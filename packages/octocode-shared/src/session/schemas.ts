/**
 * Zod schemas for session storage validation.
 *
 * These schemas mirror the TypeScript interfaces in types.ts and provide
 * runtime validation for data parsed from the session JSON file.
 */

import { z } from 'zod/v4';

/**
 * Schema for session statistics
 */
const ToolCharSavingsStatsSchema = z.object({
  rawChars: z.number(),
  responseChars: z.number(),
  savedChars: z.number(),
  calls: z.number(),
});

const GitHubCacheHitStatsSchema = z.object({
  hits: z.record(z.string(), z.number()).default({}),
  rateLimits: z.number().default(0),
});

const StatsCounterMapSchema = z.record(z.string(), z.number()).default({});

const SessionTotalUsageStatsSchema = z.object({
  toolCalls: z.number(),
  promptCalls: z.number(),
  errors: z.number(),
  rateLimits: z.number(),
  rateLimitsByProvider: StatsCounterMapSchema,
  rawChars: z.number(),
  responseChars: z.number(),
  savedChars: z.number(),
  charSavingsCalls: z.number(),
  githubCacheHits: z.number(),
  githubCacheRateLimits: z.number(),
  packageRegistryFailures: z.number().default(0),
  packageRegistryFailuresByRegistry: StatsCounterMapSchema,
});

export const SessionStatsSchema = z.object({
  toolCalls: z.number(),
  promptCalls: z.number(),
  errors: z.number(),
  rateLimits: z.number(),
  rateLimitsByProvider: StatsCounterMapSchema,
  charsSavedByTool: z
    .record(z.string(), ToolCharSavingsStatsSchema)
    .default({}),
  githubCacheHits: GitHubCacheHitStatsSchema.default({
    hits: {},
    rateLimits: 0,
  }),
  packageRegistryFailures: StatsCounterMapSchema,
  totalUsage: SessionTotalUsageStatsSchema.optional(),
});

/**
 * Schema for persisted session identity data.
 *
 * `stats` remains optional here so older session.json files can be read and
 * migrated into stats.json.
 */
export const PersistedSessionSchema = z.object({
  version: z.literal(1),
  sessionId: z.string(),
  createdAt: z.string(),
  lastActiveAt: z.string(),
  stats: SessionStatsSchema.optional(),
});

/**
 * Schema for persisted stats data
 */
export const PersistedStatsSchema = z.object({
  version: z.literal(1),
  stats: SessionStatsSchema,
});
