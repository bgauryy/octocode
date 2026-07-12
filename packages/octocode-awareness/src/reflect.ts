/**
 * reflect.ts — Post-task reflection.
 * Calls insertMemory() and insertRefinement() directly — no stdout patching.
 */

import type { DatabaseSync } from 'node:sqlite';
import { resolve } from 'node:path';
import { normalizeArtifact, normalizeReflectionOutcome, REFLECTION_IMPORTANCE } from './helpers.js';
import { fillScope } from './git.js';
import { insertMemory, insertMemoryWithSimilarityGate } from './memory.js';
import { insertRefinement } from './refinements.js';
import { insertHarnessLog } from './audit.js';
import type { ReflectParams, ReflectResult, ReflectionOutcome } from './types.js';

const NEXT_MSG = [
  'Next: inspect created fixes with octocode-awareness refinement get --state open.',
  'After applying and verifying a fix, close it with octocode-awareness refinement set --refinement-id <id> --state done.',
  'Use octocode-awareness reflect mine-weakness for recurring failures and octocode-awareness reflect export-harness for human-reviewed harness proposals.',
].join(' ');

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
    fixInstructions,
    failureSignature: failSigArg,
    importance: impArg,
    judgmentNote,
    duo = false,
    evalFailures = [],
      allowSimilar = false,
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

  const resolvedOutcome: ReflectionOutcome = normalizeReflectionOutcome(outcome);

  // Build narrative observation
  const bits: string[] = [`[reflection:${resolvedOutcome}] ${task}`];
  if (worked) bits.push(`worked: ${worked}`);
  if (didntWork) bits.push(`didn't work: ${didntWork}`);
  if (judgmentNote) bits.push(`judgment: ${judgmentNote}`);
  if (fixHarness) bits.push(`harness fix: ${fixHarness}`);
  if (fixInstructions) bits.push(`instructions feedback: ${fixInstructions}`);
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
        // `developer-review` is the query tag used by the live review view;
    // `instructions` scopes it to the instruction-author feedback channel.
    ...(fixInstructions ? ['instructions', 'developer-review'] : []),
    ...(hasEvalFailures ? ['eval'] : []),
  ];

  // The summary memory participates in failure clustering only when there are
  // no structured eval-event rows below.
  const fallbackSig = resolvedOutcome === 'failed' && fixHarness
    ? 'harness:reflection|outcome:failed'
    : null;
  // Structured eval rows are the failure events. Copying their signature onto
  // the summary memory made one failed eval satisfy mine-weakness minCount=2.
  const sig = hasEvalFailures ? null : (failSigArg ?? fallbackSig);
  const evalFallbackSig = failSigArg ?? fallbackSig;
  const scope = fillScope(
    { workspace_path: workspacePath ?? null, artifact: normalizeArtifact(artifact), repo: repoArg ?? null, ref: refArg ?? null },
    cwd ?? process.cwd(),
  );
  const pathBase = cwd ?? scope.workspace_path ?? workspacePath ?? process.cwd();
  const scopeReferences = [
    ...references,
    ...normalizeScopePaths(file ? [file] : [], 'file', pathBase),
    ...normalizeScopePaths(files, 'file', pathBase),
    ...normalizeScopePaths(folders, 'dir', pathBase),
  ];

  // Route summaries through the same atomic duplicate gate as other agent
  // surfaces. Repeated routine reflections reuse the existing memory instead
  // of growing ACTIVE storage; a caller must explicitly justify recurrence.
  const guardedSummary = insertMemoryWithSimilarityGate(db, {
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
  }, allowSimilar);
  const summaryInsert = guardedSummary.skipped ? null : guardedSummary.result;
  const memoryId = summaryInsert?.memoryId ?? guardedSummary.similar[0]!.memory_id;
  const similarMemoryIds = guardedSummary.similar.map((memory) => memory.memory_id);
  const noveltyScore = summaryInsert?.noveltyScore
    ?? Math.max(0, 1 - (guardedSummary.similar[0]?.similarity ?? 1));

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
      failureSignature: failure.failure_signature ?? evalFallbackSig,
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
      files: [...normalizeScopePaths(files, 'file', pathBase), ...normalizeScopePaths(folders, 'dir', pathBase)],
      cwd,
    });
    refinementId = rid;
  }

  // Optional instructions-feedback refinement — a tracked, human-owned item addressed
  // to the developer who authored this agent's operating instructions. Quality
  // 'instructions' keeps it out of the coding refinement queue; it surfaces via
      // `reflect developer-review` and the bounded KNOWLEDGE projection.
  let developerReviewRefinementId: string | null = null;
  if (fixInstructions) {
    const { refinementId: rid } = insertRefinement(db, {
      agentId,
      reasoning: `Instructions feedback (from ${resolvedOutcome} reflection on "${task}"): ${fixInstructions}`,
      remember: fixInstructions,
      quality: 'instructions',
      state: 'open',
      workspacePath: scope.workspace_path,
      artifact: scope.artifact,
      repo: scope.repo,
      ref: scope.ref,
      files: [...normalizeScopePaths(files, 'file', pathBase), ...normalizeScopePaths(folders, 'dir', pathBase)],
      cwd,
    });
    developerReviewRefinementId = rid;
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
        memory_skipped: guardedSummary.skipped,
        harness_fix: Boolean(fixHarness),
        instructions_feedback: Boolean(fixInstructions),
        refinement_id: refinementId,
        developer_review_refinement_id: developerReviewRefinementId,
        eval_count: evalFailureIds.length,
        workspace_path: scope.workspace_path,
        artifact: scope.artifact,
      },
    });
  } catch { /* non-critical harness log */ }

  const result: ReflectResult = {
    outcome: resolvedOutcome,
    learning_memory_id: memoryId,
    ...(guardedSummary.skipped ? { learning_memory_skipped: true as const } : {}),
    repo_fix_refinement_id: refinementId,
    harness_fix: Boolean(fixHarness),
    instructions_feedback: Boolean(fixInstructions),
    developer_review_refinement_id: developerReviewRefinementId,
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
