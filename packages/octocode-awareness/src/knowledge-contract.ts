import { z } from 'zod';
import { anchorSchema, createAnchorSchema } from './knowledge-anchor.js';

const key = z.string().trim().min(1).max(180);
const revision = z.string().min(1).max(128);
const evidenceFiles = z.array(z.string().trim().min(1).max(400)).min(1).max(8);
const applicability = z.object({ files: evidenceFiles, fingerprint: z.string().regex(/^awareness-evidence-v1:[a-f0-9]{64}$/) }).strict();
const validity = z.object({ from: z.string().datetime().optional(), until: z.string().datetime().optional() }).strict();

export const knowledgeSetSchema = z.object({
  key,
  title: z.string().trim().min(1).max(256),
  lesson: z.string().trim().min(1).max(4000),
  why: z.string().trim().min(1).max(1000).optional(),
  constraint: z.string().trim().min(1).max(1000).optional(),
  anchors: z.array(createAnchorSchema(400)).min(1).max(8),
  evidence_refs: z.array(z.string().trim().min(1).max(512)).max(8).optional(),
  expected_revision: revision.nullable(),
  request_id: z.string().trim().min(1).max(128),
  applicability: z.union([applicability, z.object({ files: evidenceFiles, capture_fingerprint: z.literal(true) }).strict()]).optional(),
  validity: validity.optional(),
}).strict();

const readFields = {
  key: key.optional(),
  anchors: z.array(anchorSchema).min(1).max(64).optional(),
  query: z.string().trim().min(1).max(512).optional(),
  failure_signature: z.string().trim().min(1).max(400).optional(),
  limit: z.number().int().min(1).max(50).optional(),
  offset: z.number().int().min(0).max(1_000_000_000).optional(),
  snapshot: z.string().min(1).max(128).optional(),
  byte_budget: z.number().int().min(1024).max(24 * 1024).optional(),
};
export const knowledgeGetSchema = z.union([
  z.object({ ...readFields, key, revision: revision.optional(), anchors: z.never().optional(), query: z.never().optional(), failure_signature: z.never().optional() }).strict(),
  z.object({ ...readFields, key: z.never().optional(), revision: z.never().optional() }).strict(),
]);
export const knowledgeRevalidateSchema = z.union([
  z.object({ ...readFields, key, anchors: z.never().optional(), query: z.never().optional(), failure_signature: z.never().optional() }).strict(),
  z.object({ ...readFields, key: z.never().optional() }).strict(),
]);
export type KnowledgeSetInput = z.infer<typeof knowledgeSetSchema>;
export type KnowledgeGetInput = z.infer<typeof knowledgeGetSchema>;
export type KnowledgeRevalidateInput = z.infer<typeof knowledgeRevalidateSchema>;

export const knowledgeMetadataSchema = z.object({
  version: z.literal(1), key, title: z.string(), anchors: z.array(anchorSchema),
  evidence_refs: z.array(z.string()), applicability: applicability.optional(), validity: validity.optional(),
  request_id: z.string(), request_digest: z.string(), expected_revision: revision.nullable(),
  attribution: z.object({ actor_id: z.string(), session_id: z.string().optional(), recorded_at: z.string(), rationale_source: z.literal('caller') }).strict(),
}).strict();
export type KnowledgeMetadata = z.infer<typeof knowledgeMetadataSchema>;
