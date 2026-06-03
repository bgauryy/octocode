/**
 * Zod schemas for credential storage validation.
 *
 * These schemas mirror the TypeScript interfaces in types.ts and provide
 * runtime validation for data parsed from encrypted credential files.
 */

import { z } from 'zod';

/**
 * Schema for OAuth token structure
 */
const OAuthTokenSchema = z.object({
  token: z.string(),
  tokenType: z.literal('oauth'),
  scopes: z.array(z.string()).optional(),
  refreshToken: z.string().optional(),
  expiresAt: z.string().optional(),
  refreshTokenExpiresAt: z.string().optional(),
});

/**
 * Schema for stored credentials for a GitHub host
 */
const StoredCredentialsSchema = z.object({
  hostname: z.string(),
  username: z.string(),
  token: OAuthTokenSchema,
  gitProtocol: z.enum(['ssh', 'https']),
  createdAt: z.string(),
  updatedAt: z.string(),
});

/**
 * Schema for the full credentials store file
 */
export const CredentialsStoreSchema = z.object({
  version: z.number(),
  credentials: z.record(z.string(), StoredCredentialsSchema),
});
