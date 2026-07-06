/**
 * reflect.ts — Post-task reflection.
 * Calls insertMemory() and insertRefinement() directly — no stdout patching.
 */

import type { DatabaseSync } from 'node:sqlite';
import { resolve } from 'node:path';
import { REFLECTION_IMPORTANCE } from './helpers.js';
import { insertMemory } from './memory.js';
import { insertRefinement } from './refinements.js';
import type { ReflectParams, ReflectResult, ReflectionOutcome } from './types.js';

const VALID_OUTCOMES: ReadonlyArray<string> = ['worked', 'partial', 'failed'];
const NEXT_MSG = 'memory_refine_get → repo fixes for the next agent · memory_mine_weakness → recurring failures · memory_digest export_doc:true → preview harness improvements. A human merges.';

function normalizeScopePaths(paths: string[] = [], prefix: 'file' | 'dir', baseCwd?: string): string[] {
  // RFLX-1: Resolve relative paths against the caller-supplied cwd, not process.cwd().
  // Without this, a reflect() call with an explicit cwd would silently resolve file:
  // references against the wrong directory.
  const base = baseCwd ?? process.cwd();
  return [...new Set(paths.filter(Boolean).map((p) => {
    const abs = p.startsWith('/') ? p : resolve(base, p);
    return `${prefix}:${abs}`;
  }))];
}

/**
 * Record a reflection: learning memory + optional repo-fix refinement.
 * Returns the result object — does NOT emit JSON to stdout.
 */
export function reflect(db: DatabaseSync, params: ReflectParams): ReflectResult {
  const {
    agentId = 'agent',
    task,
    outcome,
    lesson,
    worked,
    didntWork,
    fixRepo,
    fixHarness,
    failureSignature: failSig,
    importance: impArg,
      references = [],
      file,
      files = [],
      folders = [],
      validFrom,
      validTo,
      workspacePath,
    repo: repoArg,
    ref: refArg,
    cwd,
  } = params;

  const resolvedOutcome: ReflectionOutcome = VALID_OUTCOMES.includes(outcome ?? '')
    ? (outcome as ReflectionOutcome)
    : 'partial';

  // Build narrative observation
  const bits: string[] = [`[reflection:${resolvedOutcome}] ${task}`];
  if (worked) bits.push(`worked: ${worked}`);
  if (didntWork) bits.push(`didn't work: ${didntWork}`);
  if (fixHarness) bits.push(`harness fix: ${fixHarness}`);
  const narrative = bits.join(' | ');
  const observation = lesson
    ? (bits.length > 1 ? `${lesson}  (${narrative})` : lesson)
    : narrative;

  const importance = impArg != null
    ? Number(impArg)
    : (REFLECTION_IMPORTANCE[resolvedOutcome] ?? 5);

  const tags = ['reflection', resolvedOutcome, ...(fixHarness ? ['harness'] : [])];

  const sig = failSig
    ?? (resolvedOutcome === 'failed' && fixHarness ? 'harness:reflection|outcome:failed' : null);
    const scopeReferences = [
      ...references,
      ...normalizeScopePaths(file ? [file] : [], 'file', cwd),
      ...normalizeScopePaths(files, 'file', cwd),
      ...normalizeScopePaths(folders, 'dir', cwd),
    ];

  // Insert learning memory — direct call, no subprocess, no stdout capture
  const { memoryId, similarMemoryIds, noveltyScore } = insertMemory(db, {
    agentId,
    taskContext: task,
    observation,
    importanceScore: importance,
    label: 'EXPERIENCE', // distinct label so reflections are filterable and excluded from briefings
    tags,
      references: scopeReferences,
      failureSignature: sig,
      validFrom,
      validTo,
      workspacePath,
    repo: repoArg,
    ref: refArg,
      file: file ?? files[0] ?? folders[0] ?? null,
      cwd,
  });

  // Optional repo-fix refinement
  let refinementId: string | null = null;
  if (fixRepo) {
    // R-2: Quality reflects whether this is a fix (broken path) or improvement (good path).
    // worked  → quality:'good'  — the code works; this is an enhancement or clarification
    // partial → quality:'bad'   — something was wrong; fix it
    // failed  → quality:'bad'   — definitely broken
    const refinementQuality = resolvedOutcome === 'worked' ? 'good' : 'bad';
    const { refinementId: rid } = insertRefinement(db, {
      agentId,
      reasoning: `Fix in repo (from ${resolvedOutcome} reflection): ${fixRepo}`,
      remember: fixRepo,
      quality: refinementQuality,
      state: 'open',
      workspacePath,
      repo: repoArg,
      ref: refArg,
        files: [...files, ...folders],
        cwd,
    });
    refinementId = rid;
  }

  return {
    outcome: resolvedOutcome,
    learning_memory_id: memoryId,
    repo_fix_refinement_id: refinementId,
    harness_fix: Boolean(fixHarness),
    eval_failure_count: 0,
    eval_failure_ids: [],
    next: NEXT_MSG,
    novelty_score: noveltyScore,
    similar_memory_ids: similarMemoryIds,
  };
}
