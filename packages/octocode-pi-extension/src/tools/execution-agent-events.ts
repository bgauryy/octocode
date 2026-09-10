import { z } from 'zod';

const nonnegative = z.number().finite().nonnegative();

export const executionAgentSchema = z.strictObject({
  id: z.string().min(1),
  name: z.string(),
  parentRunId: z.string().min(1),
  status: z.string(),
  task: z.string().optional(),
  planStep: z.string().optional(),
  activity: z.string().optional(),
  pendingMessages: nonnegative.optional(),
  lastMessage: z
    .strictObject({
      direction: z.enum(['to-agent', 'from-agent']),
      action: z.enum(['send', 'steer', 'follow-up', 'reply']),
      preview: z.string(),
      timestamp: nonnegative,
    })
    .optional(),
  startedAt: nonnegative.optional(),
  updatedAt: nonnegative,
});

export const executionAgentMessageSchema = z.strictObject({
  id: z.string().min(1),
  name: z.string(),
  direction: z.enum(['to-agent', 'from-agent']),
  action: z.enum(['send', 'steer', 'follow-up', 'reply']),
  preview: z.string(),
  timestamp: nonnegative,
  task: z.string().optional(),
  planStep: z.string().optional(),
});

export const executionAgentTransitionSchema = z.strictObject({
  id: z.string().min(1),
  name: z.string(),
  from: z.string().optional(),
  to: z.string(),
  summary: z.string().optional(),
  task: z.string().optional(),
  planStep: z.string().optional(),
  updatedAt: nonnegative,
});
