import { z } from 'zod';
import { anchorSchema } from './knowledge-anchor.js';

const id = z.string().trim().min(1).max(128).regex(/^[A-Za-z0-9_.:-]+$/);
const prose = (maximum: number) => z.string().trim().min(1).max(maximum)
  .refine(value => !/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(value), 'Control characters are not valid evidence text');
const page = { limit: z.number().int().min(1).max(10).optional(), cursor: z.string().max(2048).optional() };
export const experienceEventSchema = z.object({
  trace_id: id, event_id: id, title: prose(160), summary: prose(1000),
  kind: z.enum(['attempt', 'decision', 'result', 'gotcha', 'verification']),
  outcome: z.enum(['success', 'failure', 'unresolved', 'skipped']).optional(),
  relates_to: z.array(id).max(16).default([]),
  rationale: prose(1000).optional(), attribution: prose(240).optional(),
  anchors: z.array(anchorSchema).max(8).default([]),
  evidence: z.array(z.object({ title: prose(160), text: prose(1024) }).strict()).max(4).default([]),
}).strict();
export const experienceInputSchema = z.discriminatedUnion('action', [
  experienceEventSchema.extend({ action: z.literal('record') }),
  z.object({ action: z.literal('get'), trace_id: id, source: z.enum(['journal', 'archive']).optional(), ...page }).strict(),
  z.object({ action: z.literal('list'), ...page }).strict(),
  z.object({ action: z.literal('seal'), trace_id: id }).strict(),
  z.object({ action: z.literal('compare'), trace_id: id, other_trace_id: id, ...page }).strict(),
  z.object({ action: z.literal('recover'), ...page }).strict(),
]);
export type ExperienceInput = z.input<typeof experienceInputSchema>;
export type ExperienceEvent = z.infer<typeof experienceEventSchema> & {
  sequence: number; actor_id: string; session_id?: string; created_at: string;
};
export interface ExperienceBinding { workspace: string; actorId: string; sessionId?: string }
export const MAX_EXPERIENCE_EVENTS = 128;
export const MAX_EXPERIENCE_EVENT_BYTES = 8192;
export const EXPERIENCE_PAGE_BYTES = 24 * 1024;
