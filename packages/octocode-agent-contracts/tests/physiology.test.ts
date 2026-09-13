import { describe, expect, expectTypeOf, it } from 'vitest';
import {
  NativeRuntimeObservationSchema,
  PiRuntimeObservationSchema,
  RuntimeObservationSchema,
  type NativeRuntimeObservation,
  type PiRuntimeObservation,
  type RuntimeObservation,
} from '../src/physiology.js';

const nativeObservation = {
  schema_version: 1,
  source: 'native_runtime',
  context: {
    current_tokens: 250,
    measured_at: 1_000,
    input_limit_tokens: 1_000,
    remaining_input_tokens: 750,
    saturation_basis_points: 2_500,
  },
  tools: { window: 32, observed: 8, failed: 1, cancelled: 1, blocked: 1 },
  controls: {
    owner: 'agent_core',
    compactions_committed: 2,
    compactions_failed: 1,
    retries_scheduled: 3,
    provider_attempt: 2,
    provider_max_attempts: 4,
  },
} as const;

const piObservation = {
  schema_version: 1,
  source: 'pi_runtime',
  session: {
    owner: 'pi',
    session_id: 'session-123',
    generation: 2,
    observed_at: 2_000,
  },
  context: {
    measurement: 'host_reported',
    current_tokens: 900,
    measured_at: 1_999,
    input_limit_tokens: 1_000,
    remaining_input_tokens: 100,
    saturation_basis_points: 9_000,
  },
  tools: {
    window: 32,
    observed: 7,
    total_observed: 41,
    latest_outcome: 'blocked',
    failed: 1,
    cancelled: 1,
    blocked: 2,
  },
  compaction: { owner: 'pi', committed: 2, failed: 1 },
} as const;

describe('runtime observation contracts', () => {
  it('parses both host variants and infers their TypeScript types', () => {
    expect(NativeRuntimeObservationSchema.parse(nativeObservation)).toEqual(nativeObservation);
    expect(PiRuntimeObservationSchema.parse(piObservation)).toEqual(piObservation);
    expect(RuntimeObservationSchema.parse(nativeObservation)).toEqual(nativeObservation);
    expect(RuntimeObservationSchema.parse(piObservation)).toEqual(piObservation);

    expectTypeOf(NativeRuntimeObservationSchema.parse(nativeObservation)).toEqualTypeOf<NativeRuntimeObservation>();
    expectTypeOf(PiRuntimeObservationSchema.parse(piObservation)).toEqualTypeOf<PiRuntimeObservation>();
    expectTypeOf(RuntimeObservationSchema.parse(piObservation)).toEqualTypeOf<RuntimeObservation>();
  });

  it('keeps undeclared Pi measurements absent', () => {
    const minimal = PiRuntimeObservationSchema.parse({
      schema_version: 1,
      source: 'pi_runtime',
      session: { owner: 'pi', session_id: 'session-123', generation: 0, observed_at: 1 },
    });

    expect(minimal).toEqual({
      schema_version: 1,
      source: 'pi_runtime',
      session: { owner: 'pi', session_id: 'session-123', generation: 0, observed_at: 1 },
    });
    expect('context' in minimal).toBe(false);
    expect('tools' in minimal).toBe(false);
    expect('compaction' in minimal).toBe(false);
  });

  it.each([
    { ...nativeObservation, smuggled: 'prompt content' },
    { ...nativeObservation, tools: { ...nativeObservation.tools, content: 'tool output' } },
    { ...piObservation, session: { ...piObservation.session, transcript: 'secret' } },
    { ...piObservation, compaction: { ...piObservation.compaction, summary: 'secret' } },
  ])('rejects unknown payload fields', (value) => {
    expect(RuntimeObservationSchema.safeParse(value).success).toBe(false);
  });

  it.each([
    { ...nativeObservation, tools: { ...nativeObservation.tools, observed: 33 } },
    { ...nativeObservation, tools: { ...nativeObservation.tools, observed: 2 } },
    { ...nativeObservation, controls: { ...nativeObservation.controls, provider_attempt: 5 } },
    { ...nativeObservation, controls: { ...nativeObservation.controls, provider_max_attempts: undefined } },
    { ...nativeObservation, controls: { ...nativeObservation.controls, retries_scheduled: 0.5 } },
    { ...nativeObservation, controls: { ...nativeObservation.controls, compactions_failed: Number.MAX_SAFE_INTEGER + 1 } },
  ])('rejects inconsistent or unsafe native measurements', (value) => {
    expect(NativeRuntimeObservationSchema.safeParse(value).success).toBe(false);
  });

  it.each([
    { ...nativeObservation, context: { ...nativeObservation.context, remaining_input_tokens: 749 } },
    { ...nativeObservation, context: { ...nativeObservation.context, saturation_basis_points: 2_499 } },
    { ...nativeObservation, context: { ...nativeObservation.context, remaining_input_tokens: undefined } },
  ])('validates context measurement groups and arithmetic', (value) => {
    expect(NativeRuntimeObservationSchema.safeParse(value).success).toBe(false);
  });

  it('accepts context occupancy when the host does not know its input limit', () => {
    const value = { ...nativeObservation, context: { current_tokens: 250, measured_at: 1_000 } };
    expect(NativeRuntimeObservationSchema.parse(value)).toEqual(value);
  });

  it.each([
    { ...piObservation, session: { ...piObservation.session, owner: 'agent_core' } },
    { ...piObservation, session: { ...piObservation.session, observed_at: 1_998 } },
    { ...piObservation, context: { ...piObservation.context, measurement: 'tokenizer_exact' } },
    { ...piObservation, context: { ...piObservation.context, measurement: undefined } },
    { ...piObservation, tools: { ...piObservation.tools, observed: 3 } },
    { ...piObservation, tools: { ...piObservation.tools, total_observed: 6 } },
    { ...piObservation, tools: { ...piObservation.tools, latest_outcome: 'unknown' } },
    { ...piObservation, compaction: { ...piObservation.compaction, owner: 'agent_core' } },
    { ...piObservation, session: { ...piObservation.session, session_id: '' } },
  ])('rejects invalid Pi ownership, timing, identity, and counts', (value) => {
    expect(PiRuntimeObservationSchema.safeParse(value).success).toBe(false);
  });

  it('does not accept cross-variant owners', () => {
    expect(RuntimeObservationSchema.safeParse({ ...nativeObservation, source: 'pi_runtime' }).success).toBe(false);
    expect(RuntimeObservationSchema.safeParse({ ...piObservation, source: 'native_runtime' }).success).toBe(false);
  });
});
