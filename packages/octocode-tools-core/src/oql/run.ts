/**
 * OQL runner — the single entry point behind `octocode search`.
 *
 *   normalize -> plan -> (explain) -> execute via adapter -> envelope
 *
 * Handles single queries and bounded batches (1-5). `--explain` includes the
 * plan; `--dry-run` returns the plan without executing.
 *
 * This file is the top-level orchestrator only; the implementation is split
 * across ./run/* (see each file's header comment for its slice):
 *  - run/single.ts                 — per-query execution (plan -> adapter -> envelope)
 *  - run/batch.ts                  — bounded-batch execution + merge
 *  - run/select.ts                 — `select` row/continuation projection
 *  - run/proofGrades.ts            — proofGrade inference
 *  - run/dryRun.ts                 — dry-run / non-executable-plan envelopes
 *  - run/paths.ts                  — local-path relativization (+ inverse)
 *  - run/continuations/registry.ts — next.* continuation attachment
 *  - run/continuations/records.ts  — code/content/materialized/research/graph builders
 *  - run/continuations/semantics.ts— semantics builder
 *
 * Everything previously importable from './run.js' stays importable from here,
 * either defined directly below or re-exported from its new home.
 */
import { normalizeInput } from './normalize.js';
import { OqlValidationError } from './diagnostics.js';
import { unsupportedEnvelope } from './envelope.js';
import {
  isCanonicalBatch,
  type OqlRunResult,
  type OqlSearchInput,
} from './types.js';
import { runBatch } from './run/batch.js';
import {
  runSingle,
  stripUniformSource,
  type RunOptions,
} from './run/single.js';

export type { RunOptions };
export type { ContinuationCtx } from './run/continuations/types.js';

export async function runOqlSearch(
  input: OqlSearchInput,
  options: RunOptions = {}
): Promise<OqlRunResult> {
  let canonical;
  try {
    canonical = normalizeInput(input);
  } catch (err) {
    if (err instanceof OqlValidationError) {
      return unsupportedEnvelope(err.diagnostics);
    }
    throw err;
  }

  if (isCanonicalBatch(canonical)) {
    return runBatch(canonical, input, options);
  }
  const env = await runSingle(canonical, input, options);
  stripUniformSource(env.results);
  return env;
}
