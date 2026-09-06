import { FTS_SCHEMA_DDL, HOOK_RECEIPTS_DDL, SCHEMA_DDL } from '../db-schema.js';
import { DEFAULT_AWARENESS_STORAGE_SCOPE, globalAwarenessDatabasePath } from '../storage-scope.js';

export type AwarenessEntityKind = 'table' | 'virtual_table';
export type AwarenessEntityOwner = 'awareness';

export interface AwarenessEntity {
  name: string;
  kind: AwarenessEntityKind;
  owner: AwarenessEntityOwner;
  family: string;
}

export interface AwarenessEntityCatalog {
  storage: {
    default_scope: typeof DEFAULT_AWARENESS_STORAGE_SCOPE;
    default_path: string;
    repo_override: '--db-scope repo';
    explicit_override: '--db <path>';
  };
  entities: AwarenessEntity[];
}

const TABLE_PATTERN = /CREATE\s+(?:VIRTUAL\s+)?TABLE\s+IF\s+NOT\s+EXISTS\s+([a-z][a-z0-9_]*)/gi;

function ddlRelations(ddl: string): Array<{ name: string; kind: AwarenessEntityKind }> {
  const relations: Array<{ name: string; kind: AwarenessEntityKind }> = [];
  for (const match of ddl.matchAll(TABLE_PATTERN)) {
    relations.push({
      name: match[1]!.toLowerCase(),
      kind: /CREATE\s+VIRTUAL\s+TABLE/i.test(match[0]!) ? 'virtual_table' : 'table',
    });
  }
  return relations;
}

const FAMILY_BY_NAME: Record<string, string> = {
  hook_receipts: 'hooks',
  sessions: 'presence',
  awareness_memories: 'memory',
  awareness_plans: 'planning',
  plan_members: 'planning',
  plan_docs: 'planning',
  awareness_tasks: 'tasks',
  task_paths: 'tasks',
  task_dependencies: 'tasks',
  task_runs: 'execution',
  run_files: 'execution',
  task_claims: 'execution',
  task_events: 'execution',
  awareness_locks: 'locks',
  delivery_state: 'delivery',
  run_log: 'execution',
  refinements: 'learning',
  signals: 'messaging',
  signal_reads: 'messaging',
  memory_refs: 'memory',
  awareness_agents: 'identity',
  edit_log: 'audit',
  harness_log: 'harness',
  memories_fts: 'search',
  handoffs: 'handoff',
  event_outbox: 'events',
  event_consumers: 'events',
  event_acknowledgements: 'events',
  pending_interactions: 'interactions',
  authorization_receipts: 'authorization',
  capability_receipts: 'authorization',
  worker_lifecycle_events: 'workers',
};

/**
 * Read-only entity catalog derived from the executable Awareness DDL and
 * optional worker audit relation. It never opens a database or reads rows.
 */
export function awarenessEntityCatalog(env: NodeJS.ProcessEnv = process.env): AwarenessEntityCatalog {
  const canonical = ddlRelations(`${HOOK_RECEIPTS_DDL}\n${SCHEMA_DDL}`);
  const search = ddlRelations(FTS_SCHEMA_DDL);
  const names = new Map<string, { name: string; kind: AwarenessEntityKind }>();
  for (const relation of [...canonical, ...search]) names.set(relation.name, relation);
  names.set('worker_lifecycle_events', { name: 'worker_lifecycle_events', kind: 'table' });
  const entities = [...names.values()]
    .map(({ name, kind }) => {
      const family = FAMILY_BY_NAME[name];
      if (family === undefined) throw new Error(`entity catalog missing family mapping: ${name}`);
      return {
        name,
        kind,
        owner: 'awareness' as const,
        family,
      };
    })
    .sort((left, right) => left.name.localeCompare(right.name));
  return {
    storage: {
      default_scope: DEFAULT_AWARENESS_STORAGE_SCOPE,
      default_path: globalAwarenessDatabasePath(env),
      repo_override: '--db-scope repo',
      explicit_override: '--db <path>',
    },
    entities,
  };
}
