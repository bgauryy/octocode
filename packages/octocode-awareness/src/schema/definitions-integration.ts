import { z } from 'zod';
import { MEMORY_RECALL_MODES_V1 } from '../memory-hardening.js';

const text = z.string().min(1);
const scope = { workspace: text.optional() };
const limit = z.number().int().positive().optional();

/** CLI-only integrations retain their distinct evidence and host contracts. */
export const integrationSchemas = {
  agent_presence: z.object({ ...scope, agent_id: text, status: z.enum(['ACTIVE', 'IDLE']).optional() }),
  verified_memory: z.object({ ...scope, label: text, text, source_digest: text, scope: z.enum(['project', 'artifact']).optional(), verified_at: text.optional(), valid_until: text.optional(), importance: z.number().int().min(1).max(10).optional(), tags: text.optional() }),
  verified_recall: z.object({ ...scope, query: text.optional(), label: text.optional(), source_digest: text.optional(), scope: z.enum(['project', 'artifact']).optional(), mode: z.enum(MEMORY_RECALL_MODES_V1).optional(), limit, now: text.optional(), min_similarity: z.number().optional() }),
  memory_evaluate: z.object({ ...scope, corpus_json: text.optional(), now: text.optional(), limit, min_similarity: z.number().optional() }),
  memory_reindex: z.object({ ...scope, force: z.boolean().optional(), limit }),
  memory_prune: z.object({ ...scope, older_than: text, label: text.optional(), confirm: z.boolean().optional() }),
  handoff_add: z.object({ ...scope, agent_id: text, summary: text, file: z.array(text).optional() }),
  handoff_list: z.object({ ...scope, include_cleared: z.boolean().optional() }),
  handoff_clear: z.object({ ...scope, handoff_id: text }),
  guide: z.object({ json: z.boolean().optional() }),
  instructions_export: z.object({ format: z.enum(['prompt', 'agents-md', 'json']).optional() }),
  pre_edit: z.object({ ...scope, host: text.optional(), agent_id: text.optional(), event_json: text.optional() }),
  database_consolidate: z.object({ source: text, destination: text, unattributed_agent_id: text.optional() }),
};
