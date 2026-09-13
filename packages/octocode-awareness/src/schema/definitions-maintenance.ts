import { z } from 'zod';

export const MAINTENANCE_RETENTION_OPERATION = 'maintenance.retention' as const;
export const MAINTENANCE_RETENTION_CONFIRMATION = 'apply-retention' as const;
export const STORE_RETIREMENT_OPERATION = 'maintenance.store-retire' as const;
export const STORE_RETIREMENT_CONFIRMATION = 'retire' as const;

/** Canonical operator-only retention contract. It is intentionally not routine discovery. */
export const maintenanceRetentionSchema = z.object({
  action: z.enum(['report', 'apply']).default('report')
    .describe('Read-only report or confirmed apply.'),
  confirm: z.literal(MAINTENANCE_RETENTION_CONFIRMATION).optional()
    .describe('Required only for apply.'),
  retention_days: z.number().int().min(1).max(3650).default(90)
    .describe('Age for prunable terminal rows.'),
  operational_retention_days: z.number().int().min(1).max(3650).default(90)
    .describe('Age for operational events; audit is retained.'),
  stale_run_age_days: z.number().int().min(1).max(3650).default(1),
  fail_stale_active_runs: z.boolean().default(false),
  limit: z.number().int().min(1).max(500).default(100)
    .describe('Per-owner apply batch size.'),
  as_of: z.string().datetime({ offset: true }).optional()
    .describe('Continuation cutoff time.'),
}).strict().superRefine((value, ctx) => {
  if (value.action === 'apply' && value.confirm !== MAINTENANCE_RETENTION_CONFIRMATION) {
    ctx.addIssue({
      code: 'custom',
      path: ['confirm'],
      message: `Retention apply requires --confirm ${MAINTENANCE_RETENTION_CONFIRMATION}.`,
    });
  }
  if (value.action === 'report' && value.confirm !== undefined) {
    ctx.addIssue({ code: 'custom', path: ['confirm'], message: 'Confirmation is valid only with --action apply.' });
  }
});

export type MaintenanceRetentionInput = z.infer<typeof maintenanceRetentionSchema>;

export const maintenanceRetentionDescriptor = Object.freeze({
  operation: MAINTENANCE_RETENTION_OPERATION,
  command: 'maintenance retention',
  use: 'Report retention candidates, or apply them after exact confirmation.',
  effects: ['read', 'write'] as const,
});

/** Canonical operator-only whole-store retirement contract. */
export const storeRetirementSchema = z.object({
  action: z.enum(['report', 'apply']).default('report')
    .describe('Read-only report or confirmed quarantine.'),
  confirm: z.literal(STORE_RETIREMENT_CONFIRMATION).optional()
    .describe('Required only for apply.'),
  report_file: z.string().min(1).optional()
    .describe('Reviewed report JSON required for apply.'),
}).strict().superRefine((value, ctx) => {
  if (value.action === 'apply' && value.confirm !== STORE_RETIREMENT_CONFIRMATION) {
    ctx.addIssue({ code: 'custom', path: ['confirm'], message: 'Store retirement apply requires --confirm retire.' });
  }
  if (value.action === 'apply' && !value.report_file) {
    ctx.addIssue({ code: 'custom', path: ['report_file'], message: 'Store retirement apply requires --report-file.' });
  }
  if (value.action === 'report' && value.confirm !== undefined) {
    ctx.addIssue({ code: 'custom', path: ['confirm'], message: 'Confirmation is valid only with --action apply.' });
  }
  if (value.action === 'report' && value.report_file !== undefined) {
    ctx.addIssue({ code: 'custom', path: ['report_file'], message: 'Report file is valid only with --action apply.' });
  }
});

export type StoreRetirementCliInput = z.infer<typeof storeRetirementSchema>;

export const storeRetirementDescriptor = Object.freeze({
  operation: STORE_RETIREMENT_OPERATION,
  command: 'maintenance store-retire',
  use: 'Report or recoverably quarantine the complete Awareness store.',
  effects: ['read', 'filesystem-write'] as const,
});
