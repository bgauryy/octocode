import { existsSync } from 'node:fs';
import { DatabaseSync } from './sqlite.js';
import { schemaStateForMigrationVersion, type DatabaseMigrationVerification, type DatabaseMigrationVerificationRequest } from './db-migration-contracts.js';
import { assertDatabaseIntegrity, inspectSchemaState, readAwarenessMeta } from './db-introspection.js';
import { verifyLocalGitObjects, verifyMigrationContent } from './db-consolidation-validation.js';

export function verifyDatabaseMigration(request: DatabaseMigrationVerificationRequest): DatabaseMigrationVerification {
  if (!existsSync(request.sourcePath)) throw new Error(`migration source no longer exists: ${request.sourcePath}`);
  if (!existsSync(request.destinationPath)) throw new Error(`migration destination does not exist: ${request.destinationPath}`);
  const source = new DatabaseSync(request.sourcePath, { readOnly: true });
  const destination = new DatabaseSync(request.destinationPath, { readOnly: true });
  try {
    const expectedSourceState = schemaStateForMigrationVersion(request.sourceVersion);
    if (inspectSchemaState(source) !== expectedSourceState) throw new Error(`migration source no longer matches ${request.sourceVersion}`);
    if (inspectSchemaState(destination) !== 'canonical') throw new Error('migration destination is not canonical');
    const meta = readAwarenessMeta(destination);
    if (meta.storeId !== request.expectedStoreId) throw new Error('migration destination store_id does not match the source mapping');
    assertDatabaseIntegrity(destination);
    verifyMigrationContent(source, destination, {
      expectedCounts: request.expectedCounts,
      expectedEventIds: request.expectedEventIds,
      expectedEventSequences: request.expectedEventSequences,
      expectedEventHighWater: request.expectedEventHighWater,
    });
    const localGit = verifyLocalGitObjects(destination, request.expectedLocalGitRoot);
    return {
      integrity: 'ok', foreignKeyViolations: 0, storeId: meta.storeId,
      sourcePresent: true, destinationPresent: true,
      tableCountsVerified: Object.keys(request.expectedCounts).length,
      eventOrderVerified: true, eventReplayVerified: true,
      localGitRootReachable: localGit.reachable,
      localGitObjectsVerified: localGit.objects,
    };
  } finally { source.close(); destination.close(); }
}
