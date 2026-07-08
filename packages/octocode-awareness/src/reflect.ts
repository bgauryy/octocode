/**
 * reflect.ts — Post-task reflection.
 * Calls insertMemory() and insertRefinement() directly — no stdout patching.
 */

import type { DatabaseSync } from 'node:sqlite';
import { resolve } from 'node:path';
import { normalizeArtifact, REFLECTION_IMPORTANCE } from './helpers.js';
import { fillScope } from './git.js';
import { insertMemory } from './memory.js';
import { insertRefinement } from './refinements.js';
import { insertHarnessLog } from './audit.js';
import type { ReflectParams, ReflectResult, ReflectionOutcome } from './types.js';

const VALID_OUTCOMES: ReadonlyArray<string> = ['worked', 'partial', 'failed'];
const NEXT_MSG = 'memory_refine_get → repo fixes for the next agent · octocode-awareness reflect mine-weakness/maintenance digest → recurring failures and harness previews. A human merges.';

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
    failureSignature: failSigArg,
    importance: impArg,
    judgmentNote,
    duo = false,
    evalFailures = [],
      references = [],
      file,
      files = [],
      folders = [],
      validFrom,
      validTo,
      workspacePath,
      artifact,
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
  if (judgmentNote) bits.push(`judgment: ${judgmentNote}`);
  if (fixHarness) bits.push(`harness fix: ${fixHarness}`);
  const narrative = bits.join(' | ');
  const observation = lesson
    ? (bits.length > 1 ? `${lesson}  (${narrative})` : lesson)
    : narrative;

  const importance = impArg != null
    ? Number(impArg)
    : (REFLECTION_IMPORTANCE[resolvedOutcome] ?? 5);

  const hasEvalFailures = evalFailures.length > 0;
  const tags = [
    'reflection', resolvedOutcome,
    ...(fixHarness ? ['harness'] : []),
    ...(hasEvalFailures ? ['eval'] : []),
  ];

  // When --failure-signature is omitted, the first structured eval failure's
  // signature drives mine-weakness clustering for the main lesson memory.
  const failSig = failSigArg ?? evalFailures.find((f) => f.failure_signature)?.failure_signature ?? null;
  const sig = failSig
    ?? (resolvedOutcome === 'failed' && fixHarness ? 'harness:reflection|outcome:failed' : null);
  const scope = fillScope(
    { workspace_path: workspacePath ?? null, artifact: normalizeArtifact(artifact), repo: repoArg ?? null, ref: refArg ?? null },
    cwd ?? process.cwd(),
  );
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
    importance: importance,
    label: 'EXPERIENCE', // distinct label so reflections are filterable and excluded from briefings
    tags,
      references: scopeReferences,
      failureSignature: sig,
      validFrom,
      validTo,
      workspacePath: scope.workspace_path,
      artifact: scope.artifact,
      repo: scope.repo,
      ref: scope.ref,
      cwd,
  });

  // Structured eval failures — one eval-tagged memory each, so mine-weakness
  // can cluster them by failure_signature. Diagnostic packets, never auto-patches.
  const evalFailureIds: string[] = [];
  for (const failure of evalFailures) {
    if (!failure || typeof failure.id !== 'string' || !failure.id.trim()) continue;
    const lessonText = failure.suggested_lesson?.trim()
      || `Eval question ${failure.id} failed${failure.dimension ? ` on ${failure.dimension}` : ''}.`;
    const { memoryId: evalMemId } = insertMemory(db, {
      agentId,
      taskContext: `[eval:${failure.id}]${failure.dimension ? ` ${failure.dimension} —` : ''} ${task}`,
      observation: lessonText,
      importance: importance,
      label: 'EXPERIENCE',
      tags: ['reflection', 'eval', resolvedOutcome],
      failureSignature: failure.failure_signature ?? sig,
      workspacePath: scope.workspace_path,
      artifact: scope.artifact,
      repo: scope.repo,
      ref: scope.ref,
      cwd,
    });
    evalFailureIds.push(evalMemId);
  }

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
      workspacePath: scope.workspace_path,
      artifact: scope.artifact,
      repo: scope.repo,
      ref: scope.ref,
        files: [...normalizeScopePaths(files, 'file', cwd), ...normalizeScopePaths(folders, 'dir', cwd)],
        cwd,
    });
    refinementId = rid;
  }

  // Record harness loop lifecycle event
  try {
    insertHarnessLog(db, {
      agentId,
      eventType: 'reflect',
      memoryId,
      workspacePath: scope.workspace_path,
      artifact: scope.artifact,
      payload: {
        outcome: resolvedOutcome,
        novelty_score: noveltyScore,
        harness_fix: Boolean(fixHarness),
        refinement_id: refinementId,
        eval_count: evalFailureIds.length,
        workspace_path: scope.workspace_path,
        artifact: scope.artifact,
      },
    });
  } catch { /* non-critical harness log */ }

  const result: ReflectResult = {
    outcome: resolvedOutcome,
    learning_memory_id: memoryId,
    repo_fix_refinement_id: refinementId,
    harness_fix: Boolean(fixHarness),
    eval_failure_count: evalFailureIds.length,
    eval_failure_ids: evalFailureIds,
    next: NEXT_MSG,
    novelty_score: noveltyScore,
    similar_memory_ids: similarMemoryIds,
  };

  // Advisory duo packet — two reviewer roles the agent may run itself.
  // Not stored, not scored, not enforced (self-harness.md contract).
  if (duo) {
    result.reflection_duo = {
      advisory: true,
      roles: [
        {
          role: 'supporter',
          prompt: `Reviewing "${task}" (outcome: ${resolvedOutcome}): what in this approach worked and should be reinforced or generalized? Name the strongest evidence for keeping it.`,
        },
        {
          role: 'skeptic',
          prompt: `Reviewing "${task}" (outcome: ${resolvedOutcome}): what evidence is missing or unverified? What alternative explanation or failure mode does this reflection overlook?`,
        },
      ],
    };
  }

  return result;
}
