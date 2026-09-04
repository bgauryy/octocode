/**
 * compaction-artifacts — durable markdown breadcrumbs for compacted sessions.
 *
 * Pi's compaction entry stays in the JSONL transcript, but a small markdown copy
 * under Octocode home gives humans and resumed agents a stable path to reopen
 * after context is cleared. These artifacts are best-effort: compaction must
 * never fail because a temp doc could not be written.
 */

import { createSessionArtifactContext, type SessionIdentityInput } from './session-artifacts.js';
import type { CompactionCheckpointDetails } from './custom-messages.js';
import { redactCompactionText } from './compaction-redaction.js';

export interface CompactionArtifact {
  path: string;
  latestPath: string;
}

export interface CompactionArtifactSession {
  getSessionId?(): string | undefined;
  getSessionFile?(): string | undefined;
}

function safeFilename(value: string): string {
  const cleaned = value.trim().replace(/[^a-zA-Z0-9._-]+/g, '-').replace(/^-+|-+$/g, '');
  return cleaned || 'checkpoint';
}

function mdList(items: string[] | undefined): string {
  if (!items || items.length === 0) return '(none)';
  return items.map((item) => `- ${item}`).join('\n');
}

function mdValue(value: string | undefined): string {
  return value?.trim() || '(none)';
}

function renderPlanContinuation(details: CompactionCheckpointDetails): string[] {
  const plan = details.continuation?.plan;
  if (!plan) return ['## Active plan', '', '(none)', ''];
  const { review, coordination, steps } = plan;
  const lines = [
    '## Active plan',
    '',
    `Snapshot: ${review.branchSnapshotId} · generation ${review.generation}`,
    `Phase: ${review.phase}`,
    `RFC: ${mdValue(review.rfcPath)}`,
    `Displayed revision: ${mdValue(review.revision)}`,
    `Accepted revision: ${mdValue(review.acceptedRevision)}`,
    `Coordination: ${coordination.mode}`,
    `Awareness plan: ${mdValue(coordination.awarenessPlanId)}`,
    `Materialized revision: ${mdValue(coordination.materializedRevision)}`,
    '',
  ];
  if (review.decisions.length > 0) {
    lines.push('### Decisions', '', ...review.decisions.map((decision) => `- **${decision.q}** — ${decision.a}`), '');
  }
  if (review.blockingQuestions.length > 0) {
    lines.push('### Blocking questions', '', ...review.blockingQuestions.map((question) => `- [${question.answer ? 'x' : ' '}] ${question.prompt}${question.answer ? ` — ${question.answer}` : ''}`), '');
  }
  if (review.comments.length > 0) {
    lines.push('### Review comments', '', ...review.comments.map((comment) => `- [${comment.resolved ? 'x' : ' '}] ${comment.body}${comment.section ? ` (${comment.section})` : ''}`), '');
  }
  lines.push('### Steps', '');
  for (const step of steps) {
    const mark = step.status === 'done' ? 'x' : step.status === 'doing' ? '~' : ' ';
    lines.push(`- [${mark}] ${step.text} <!-- id:${step.id} -->`);
    if (step.activeForm) lines.push(`  - Active form: ${step.activeForm}`);
    if (step.dependsOnStepIds?.length) lines.push(`  - Depends on: ${step.dependsOnStepIds.join(', ')}`);
    if (step.paths?.length) lines.push(`  - Paths: ${step.paths.join(', ')}`);
    if (step.reasoning) lines.push(`  - Reasoning: ${step.reasoning}`);
    if (step.acceptance) lines.push(`  - Acceptance: ${step.acceptance}`);
    if (step.checkCommand) lines.push(`  - Check: \`${step.checkCommand}\``);
    if (step.awarenessTaskId) lines.push(`  - Awareness task: ${step.awarenessTaskId}`);
  }
  lines.push('');
  return lines;
}

export function buildCompactionMarkdown(details: CompactionCheckpointDetails, createdAt = new Date()): string {
  const source = details.fromExtension === undefined ? 'unknown' : details.fromExtension ? 'octocode' : 'pi';
  return redactCompactionText([
    `# Compaction checkpoint ${details.label}`,
    '',
    `Created: ${createdAt.toISOString()}`,
    details.reason ? `Reason: ${details.reason}` : undefined,
    details.tokensBefore !== undefined ? `Tokens before: ${details.tokensBefore}` : undefined,
    details.estimatedTokensAfter !== undefined ? `Estimated tokens after: ${details.estimatedTokensAfter}` : undefined,
    details.reclaimedTokens !== undefined
      ? `Estimated reclaimed: ${details.reclaimedTokens}${details.reclaimedPercent !== undefined ? ` (${details.reclaimedPercent}%)` : ''}`
      : undefined,
    `Source: ${source}`,
    '',
    '## Summary',
    '',
    details.summary?.trim() || '(summary unavailable)',
    '',
    '## Read files',
    '',
    mdList(details.readFiles),
    '',
    '## Modified files',
    '',
    mdList(details.modifiedFiles),
    '',
    ...renderPlanContinuation(details),
    '## Resume',
    '',
    'Treat this checkpoint as a recovery hint. Read the inline summary from the context marker first; only read this file when the summary is absent or insufficient. If there is active authorized work in the plan, continue it directly without announcing the rehydration. Current authoritative sources override stale checkpoint text. Stop only when all active work is complete, the user asks to pause, or a real blocker requires input.',
    '',
  ].filter((line): line is string => line !== undefined).join('\n'));
}

/**
 * Write compaction checkpoint markdown artifacts.
 *
 * Requires `cwd` + `session` and routes into the session
 * artifact tree under `$OCTOCODE_HOME/extension/workspaces/<workspace>/sessions/<session-key>/compaction/`.
 * Both a timestamped snapshot and a `latest.md` pointer are written there,
 * and both are registered as `compaction` producers in the manifest.
 * Never throws.
 */
export function writeCompactionArtifact(
  details: CompactionCheckpointDetails,
  session?: CompactionArtifactSession,
  cwd?: string,
): CompactionArtifact | undefined {
  try {
    if (!cwd || !session) return undefined;
    const input: SessionIdentityInput = { cwd, sessionManager: session };
    const artifactCtx = createSessionArtifactContext(input);
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
    const snapshotRel = `compaction/${timestamp}-${safeFilename(details.label)}.md`;
    const latestRel = 'compaction/latest.md';
    const markdown = buildCompactionMarkdown(details);
    artifactCtx.writeText(snapshotRel, markdown);
    artifactCtx.writeText(latestRel, markdown);
    artifactCtx.registerProducer('compaction', snapshotRel);
    artifactCtx.registerProducer('compaction', latestRel);
    return { path: artifactCtx.resolve(snapshotRel), latestPath: artifactCtx.resolve(latestRel) };
  } catch {
    return undefined;
  }
}
