/** Valid example payloads for the specialized integration schemas. */
export const integrationExamples = {
  agent_presence: { agent_id: 'agent', status: 'ACTIVE' },
  verified_memory: { label: 'TEST', text: 'Parser check passed.', source_digest: 'sha256:source' },
  verified_recall: { query: 'parser', mode: 'lexical' },
  memory_evaluate: { corpus_json: '{"version":1,"corpusId":"example","cases":[]}' },
  memory_reindex: { force: false },
  memory_prune: { older_than: '30d' },
  handoff_add: { agent_id: 'agent', summary: 'Continue the parser review.' },
  handoff_list: { include_cleared: false },
  handoff_clear: { handoff_id: 'handoff_123' },
  guide: { json: true },
  instructions_export: { format: 'prompt' },
  pre_edit: { host: 'generic', event_json: '{}' },
  database_consolidate: { source: '/tmp/source.sqlite3', destination: '/tmp/canonical.sqlite3' },
};
