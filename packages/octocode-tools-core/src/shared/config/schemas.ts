import { z } from 'zod';

export const OctocodeConfigSchema = z.looseObject({
  $schema: z.string().optional(),
  version: z.number().int().optional(),
  github: z.record(z.string(), z.unknown()).optional(),
  local: z.record(z.string(), z.unknown()).optional(),
  tools: z.record(z.string(), z.unknown()).optional(),
  network: z.record(z.string(), z.unknown()).optional(),
  lsp: z.record(z.string(), z.unknown()).optional(),
  output: z.record(z.string(), z.unknown()).optional(),
});
