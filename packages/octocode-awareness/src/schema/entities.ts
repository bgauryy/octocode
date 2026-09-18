import { FTS_SCHEMA_DDL, SCHEMA_DDL } from '../db-schema.js';
import { HOOK_RECEIPTS_DDL } from '../db-meta-schema.js';
import { DEFAULT_AWARENESS_STORAGE_SCOPE, globalAwarenessDatabasePath } from '../storage-scope.js';

export type AwarenessEntityKind = 'table' | 'virtual_table';
export type AwarenessEntityOwner = 'storage' | 'work' | 'message' | 'memory' | 'context' | 'history' | 'host' | 'infrastructure';
export type AwarenessEntityAccess = 'read-write' | 'derived-index';
export type AwarenessEntityRetention = 'store-lifetime' | 'domain-lifecycle' | 'end-state' | 'lease-bound' | 'lease-and-receipt' | 'policy-prunable' | 'retention-class';
export type AwarenessEntityDeletion = 'store-only' | 'cascade' | 'expiry-maintenance' | 'expired-ready-maintenance' | 'retention-maintenance' | 'index-rebuild';
export type AwarenessEntityCleanupOperation = 'maintenance retention' | 'maintenance store-retire';

export interface AwarenessEntityLifecycle {
  access: AwarenessEntityAccess;
  retention: AwarenessEntityRetention;
  deletion: AwarenessEntityDeletion;
  cleanup_operation: AwarenessEntityCleanupOperation;
}

export interface AwarenessEntity {
  name: string;
  kind: AwarenessEntityKind;
  owner: AwarenessEntityOwner;
  family: string;
  lifecycle: AwarenessEntityLifecycle;
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

interface EntityMetadata {
  family: string;
  owner: AwarenessEntityOwner;
  lifecycle: AwarenessEntityLifecycle;
}

type AwarenessEntityLifecyclePolicy = Omit<AwarenessEntityLifecycle, 'cleanup_operation'>;

const lifecycle = (
  access: AwarenessEntityAccess,
  retention: AwarenessEntityRetention,
  deletion: AwarenessEntityDeletion,
): AwarenessEntityLifecyclePolicy => Object.freeze({ access, retention, deletion });

const DURABLE = lifecycle('read-write', 'store-lifetime', 'store-only');
const DOMAIN = lifecycle('read-write', 'domain-lifecycle', 'store-only');
const END_STATE = lifecycle('read-write', 'end-state', 'store-only');
const TERMINAL_RETENTION = lifecycle('read-write', 'domain-lifecycle', 'retention-maintenance');
const CASCADE = lifecycle('read-write', 'domain-lifecycle', 'cascade');
const LEASE = lifecycle('read-write', 'lease-bound', 'expiry-maintenance');
const RESTORE = lifecycle('read-write', 'lease-and-receipt', 'expired-ready-maintenance');
const PRUNABLE = lifecycle('read-write', 'policy-prunable', 'retention-maintenance');
const EVENT = lifecycle('read-write', 'retention-class', 'retention-maintenance');
const INDEX = lifecycle('derived-index', 'domain-lifecycle', 'index-rebuild');

const metadata = (
  family: string,
  owner: AwarenessEntityOwner,
  policy: AwarenessEntityLifecyclePolicy,
  cleanupOperation: AwarenessEntityCleanupOperation,
): EntityMetadata => Object.freeze({
  family,
  owner,
  lifecycle: Object.freeze({ ...policy, cleanup_operation: cleanupOperation }),
});

const RETENTION = 'maintenance retention' as const;
const STORE_RETIREMENT = 'maintenance store-retire' as const;

/** Canonical semantic owner and lifecycle policy for every current DDL relation. */
const ENTITY_METADATA_BY_NAME: Readonly<Record<string, EntityMetadata>> = Object.freeze({
  awareness_meta: metadata('storage', 'storage', DURABLE, STORE_RETIREMENT),
  hook_receipts: metadata('hooks', 'host', DURABLE, STORE_RETIREMENT),
  sessions: metadata('presence', 'work', END_STATE, STORE_RETIREMENT),
  awareness_memories: metadata('memory', 'memory', PRUNABLE, RETENTION),
  awareness_plans: metadata('planning', 'work', DOMAIN, STORE_RETIREMENT),
  plan_members: metadata('planning', 'work', CASCADE, STORE_RETIREMENT),
  plan_docs: metadata('planning', 'work', CASCADE, STORE_RETIREMENT),
  awareness_tasks: metadata('tasks', 'work', DOMAIN, STORE_RETIREMENT),
  task_paths: metadata('tasks', 'work', CASCADE, STORE_RETIREMENT),
  task_dependencies: metadata('tasks', 'work', CASCADE, STORE_RETIREMENT),
  task_runs: metadata('execution', 'work', PRUNABLE, RETENTION),
  run_files: metadata('execution', 'work', CASCADE, RETENTION),
  task_claims: metadata('execution', 'work', CASCADE, RETENTION),
  awareness_locks: metadata('locks', 'work', LEASE, RETENTION),
  delivery_state: metadata('delivery', 'message', DOMAIN, STORE_RETIREMENT),
  signals: metadata('messaging', 'message', PRUNABLE, RETENTION),
  signal_reads: metadata('messaging', 'message', CASCADE, RETENTION),
  memory_refs: metadata('memory', 'memory', CASCADE, RETENTION),
  awareness_agents: metadata('identity', 'work', DOMAIN, STORE_RETIREMENT),
  memories_fts: metadata('search', 'memory', INDEX, RETENTION),
  event_outbox: metadata('events', 'infrastructure', EVENT, RETENTION),
  event_consumers: metadata('events', 'infrastructure', DOMAIN, STORE_RETIREMENT),
  event_acknowledgements: metadata('events', 'infrastructure', CASCADE, RETENTION),
  pending_interactions: metadata('interactions', 'host', TERMINAL_RETENTION, RETENTION),
  authorization_receipts: metadata('authorization', 'host', DURABLE, STORE_RETIREMENT),
  capability_receipts: metadata('authorization', 'host', DURABLE, STORE_RETIREMENT),
  local_history_operations: metadata('history', 'history', DURABLE, STORE_RETIREMENT),
  local_history_versions: metadata('history', 'history', CASCADE, STORE_RETIREMENT),
  local_history_restores: metadata('history', 'history', RESTORE, RETENTION),
  local_history_durability: metadata('history', 'history', CASCADE, STORE_RETIREMENT),
});

/**
 * Read-only entity catalog derived from the executable canonical Awareness DDL.
 * It never opens a database or reads rows. Predecessor-only relations are not
 * advertised as current entities.
 */
export function awarenessEntityCatalog(env: NodeJS.ProcessEnv = process.env): AwarenessEntityCatalog {
  const canonical = ddlRelations(`${HOOK_RECEIPTS_DDL}\n${SCHEMA_DDL}`);
  const search = ddlRelations(FTS_SCHEMA_DDL);
  const names = new Map<string, { name: string; kind: AwarenessEntityKind }>();
  for (const relation of [...canonical, ...search]) names.set(relation.name, relation);
  const entities = [...names.values()]
    .map(({ name, kind }) => {
      const semantic = ENTITY_METADATA_BY_NAME[name];
      if (semantic === undefined) throw new Error(`entity catalog missing lifecycle metadata: ${name}`);
      return {
        name,
        kind,
        ...semantic,
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
