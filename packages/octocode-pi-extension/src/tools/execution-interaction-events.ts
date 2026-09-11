import { z } from 'zod';

const nonnegative = z.number().finite().nonnegative();

export const executionMessageSchema = z.strictObject({
  messageId: z.string().min(1),
  outputRef: z.unknown().optional(),
});

export const executionResolutionSchema = z.strictObject({
  id: z.string().min(1),
  decision: z.string(),
});

export const executionPermissionResolutionSchema = executionResolutionSchema.extend({
  requester: z.string().min(1).optional(),
  operation: z.string().min(1).optional(),
  scope: z.string().min(1).optional(),
  matchedPolicy: z.string().min(1).optional(),
  decisionSource: z.enum(['policy', 'user', 'host']).optional(),
  decidedAt: nonnegative.optional(),
});

export const executionPermissionRequestedSchema = z.strictObject({
  id: z.string().min(1),
  title: z.string(),
  requester: z.string().min(1).optional(),
  operation: z.string().min(1).optional(),
  scope: z.string().min(1).optional(),
  matchedPolicy: z.string().min(1).optional(),
  persistent: z.boolean().optional(),
  expiresAt: nonnegative.optional(),
});

export const executionQuestionRequestedSchema = z.strictObject({
  id: z.string().min(1),
  title: z.string(),
  persistent: z.boolean().optional(),
  expiresAt: nonnegative.optional(),
});
