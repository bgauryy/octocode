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
export {
  applyStoreRetirement,
  reportStoreRetirement,
  StoreRetirementError,
} from './store-retirement.js';
export type {
  StoreRetirementBlocker,
  StoreRetirementBlockerCode,
  StoreRetirementInput,
  ApplyStoreRetirementInput,
  StoreRetirementReport,
  StoreRetirementResult,
  StoreRetirementTarget,
} from './store-retirement.js';
