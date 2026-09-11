/**
 * Explicit operator surface. This module is intentionally absent from routine
 * discovery and contains no argv dispatcher or compatibility aliases.
 */
export {
  applyDatabaseMigration,
  previewDatabaseMigration,
  verifyDatabaseMigration,
} from './db-consolidation.js';
export type {
  DatabaseMigrationPreview,
  DatabaseMigrationReport,
  DatabaseMigrationSourceVersion,
  DatabaseMigrationVerification,
  DatabaseMigrationVerificationRequest,
} from './db-consolidation.js';
