import { z } from 'zod';

const OAuthTokenSchema = z.object({
  token: z.string(),
  tokenType: z.literal('oauth'),
  scopes: z.array(z.string()).optional(),
  refreshToken: z.string().optional(),
  expiresAt: z.string().optional(),
  refreshTokenExpiresAt: z.string().optional(),
});

const StoredCredentialsSchema = z.object({
  hostname: z.string(),
  username: z.string(),
  token: OAuthTokenSchema,
  gitProtocol: z.enum(['ssh', 'https']),
  createdAt: z.string(),
  updatedAt: z.string(),
});

export const CredentialsStoreSchema = z.object({
  version: z.number(),
  credentials: z.record(z.string(), StoredCredentialsSchema),
});
