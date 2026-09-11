import type { SchemaState } from './db-introspection.js';

export type DatabaseMigrationSourceVersion =
  | 'legacy-renamed-v1'
  | 'canonical-path-identity'
  | 'schema-generation-upgrade'
  | 'event-envelope-upgrade'
  | 'event-envelope-path-identity-upgrade'
  | 'event-envelope-history-durability-upgrade'
  | 'event-envelope-history-durability-path-identity-upgrade'
  | 'worker-lifecycle-upgrade'
  | 'worker-lifecycle-path-identity-upgrade'
  | 'worker-lifecycle-history-durability-upgrade'
  | 'worker-lifecycle-history-durability-path-identity-upgrade'
  | 'refinements-upgrade'
  | 'event-stream-convergence-upgrade';

export interface DatabaseMigrationVerificationRequest {
  sourcePath: string;
  destinationPath: string;
  sourceVersion: DatabaseMigrationSourceVersion;
  expectedStoreId: string;
  expectedCounts: Readonly<Record<string, number>>;
  expectedEventIds: readonly string[];
  expectedEventSequences: readonly number[];
  expectedEventHighWater: number;
  expectedLocalGitRoot?: string;
}

export interface DatabaseMigrationVerification {
  integrity: 'ok';
  foreignKeyViolations: 0;
  storeId: string;
  sourcePresent: boolean;
  destinationPresent: boolean;
  tableCountsVerified: number;
  eventOrderVerified: true;
  eventReplayVerified: true;
  localGitRootReachable: true | null;
  localGitObjectsVerified: number | null;
}

const COPY_ON_WRITE_SOURCE_STATES = new Set<SchemaState>([
  'canonical-path-identity',
  'schema-generation-upgrade',
  'event-envelope-upgrade',
  'event-envelope-path-identity-upgrade',
  'event-envelope-history-durability-upgrade',
  'event-envelope-history-durability-path-identity-upgrade',
  'worker-lifecycle-upgrade',
  'worker-lifecycle-path-identity-upgrade',
  'worker-lifecycle-history-durability-upgrade',
  'worker-lifecycle-history-durability-path-identity-upgrade',
  'refinements-upgrade',
  'event-stream-convergence-upgrade',
  'legacy-renamed-predecessor',
]);

export function migrationSourceVersion(state: SchemaState): DatabaseMigrationSourceVersion {
  if (!COPY_ON_WRITE_SOURCE_STATES.has(state)) {
    throw new Error(`database migration does not support source state ${state}; source has not been changed`);
  }
  return state === 'legacy-renamed-predecessor' ? 'legacy-renamed-v1' : state as DatabaseMigrationSourceVersion;
}

export function schemaStateForMigrationVersion(version: DatabaseMigrationSourceVersion): SchemaState {
  return version === 'legacy-renamed-v1' ? 'legacy-renamed-predecessor' : version;
}
