import { z } from 'zod';

const safeNonnegativeInteger = z.number().int().min(0).max(Number.MAX_SAFE_INTEGER);

const contextMeasurementFields = {
  current_tokens: safeNonnegativeInteger,
  measured_at: safeNonnegativeInteger,
  input_limit_tokens: safeNonnegativeInteger.min(1).optional(),
  remaining_input_tokens: safeNonnegativeInteger.optional(),
  saturation_basis_points: safeNonnegativeInteger.max(10_000).optional(),
} as const;

function hasValidContextArithmetic(context: {
  current_tokens: number;
  input_limit_tokens?: number;
  remaining_input_tokens?: number;
  saturation_basis_points?: number;
}): boolean {
  const grouped = [
    context.input_limit_tokens,
    context.remaining_input_tokens,
    context.saturation_basis_points,
  ];
  const defined = grouped.filter((value) => value !== undefined).length;
  if (defined === 0) return true;
  if (defined !== grouped.length) return false;

  const inputLimit = context.input_limit_tokens!;
  return context.remaining_input_tokens === Math.max(0, inputLimit - context.current_tokens)
    && context.saturation_basis_points
      === Math.min(10_000, Math.floor((context.current_tokens / inputLimit) * 10_000));
}

const NativeContextMeasurementSchema = z
  .object(contextMeasurementFields)
  .strict()
  .refine(hasValidContextArithmetic, { message: 'Context token measurements are inconsistent.' });

const PiContextMeasurementSchema = z
  .object({ measurement: z.literal('host_reported'), ...contextMeasurementFields })
  .strict()
  .refine(hasValidContextArithmetic, { message: 'Context token measurements are inconsistent.' });

const toolMeasurementFields = {
  window: z.literal(32),
  observed: safeNonnegativeInteger.max(32),
  failed: safeNonnegativeInteger,
  cancelled: safeNonnegativeInteger,
  blocked: safeNonnegativeInteger,
} as const;

const ToolMeasurementSchema = z
  .object(toolMeasurementFields)
  .strict()
  .refine(
    ({ observed, failed, cancelled, blocked }) => failed + cancelled + blocked <= observed,
    { message: 'Tool outcome counts cannot exceed observed tools.' },
  );

const PiToolMeasurementSchema = z
  .object({
    ...toolMeasurementFields,
    total_observed: safeNonnegativeInteger,
    latest_outcome: z.enum(['succeeded', 'failed', 'cancelled', 'blocked']),
  })
  .strict()
  .refine(
    ({ observed, failed, cancelled, blocked }) => failed + cancelled + blocked <= observed,
    { message: 'Tool outcome counts cannot exceed observed tools.' },
  )
  .refine(
    ({ observed, total_observed }) => total_observed >= observed,
    { message: 'Total observed tools cannot be smaller than the rolling window.' },
  );

export const NativeRuntimeObservationSchema = z
  .object({
    schema_version: z.literal(1),
    source: z.literal('native_runtime'),
    context: NativeContextMeasurementSchema.optional(),
    tools: ToolMeasurementSchema,
    controls: z
      .object({
        owner: z.literal('agent_core'),
        compactions_committed: safeNonnegativeInteger,
        compactions_failed: safeNonnegativeInteger,
        retries_scheduled: safeNonnegativeInteger,
        provider_attempt: safeNonnegativeInteger.min(1).optional(),
        provider_max_attempts: safeNonnegativeInteger.min(1).optional(),
      })
      .strict()
      .refine(
        ({ provider_attempt, provider_max_attempts }) => {
          if (provider_attempt === undefined || provider_max_attempts === undefined) {
            return provider_attempt === provider_max_attempts;
          }
          return provider_attempt <= provider_max_attempts;
        },
        { message: 'Provider attempt measurements are inconsistent.' },
      ),
  })
  .strict();

export const PiRuntimeObservationSchema = z
  .object({
    schema_version: z.literal(1),
    source: z.literal('pi_runtime'),
    session: z
      .object({
        owner: z.literal('pi'),
        session_id: z.string().trim().min(1).max(256),
        generation: safeNonnegativeInteger,
        observed_at: safeNonnegativeInteger,
      })
      .strict(),
    context: PiContextMeasurementSchema.optional(),
    tools: PiToolMeasurementSchema.optional(),
    compaction: z
      .object({
        owner: z.literal('pi'),
        committed: safeNonnegativeInteger,
        failed: safeNonnegativeInteger,
      })
      .strict()
      .optional(),
  })
  .strict()
  .refine(
    ({ session, context }) => context === undefined || context.measured_at <= session.observed_at,
    { message: 'Context measurement cannot be newer than the observation.' },
  );

export const RuntimeObservationSchema = z.discriminatedUnion('source', [
  NativeRuntimeObservationSchema,
  PiRuntimeObservationSchema,
]);

export type NativeRuntimeObservation = z.infer<typeof NativeRuntimeObservationSchema>;
export type PiRuntimeObservation = z.infer<typeof PiRuntimeObservationSchema>;
export type RuntimeObservation = z.infer<typeof RuntimeObservationSchema>;
