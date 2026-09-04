/**
 * custom-messages — branded transcript cards for Octocode lifecycle moments.
 *
 * Two custom message types get first-class renderers instead of pi's default
 * plain custom-message row:
 *  - compaction checkpoints (emitted from compaction-hooks on session_compact)
 *  - awareness handoffs (emitted when session awareness is handed to a
 *    successor context/agent)
 *
 * Contract discipline: `content` on a custom message ENTERS THE LLM CONTEXT.
 * Compaction therefore emits one bounded, explicit marker containing the
 * summary checkpoint and active-plan pointer; renderer-only detail stays in
 * `details`. Cards follow the box/rule visual language of
 * cli-design (`╭─ ◆ … │ … ╰─`) so transcript cards and tool rows read as one
 * system.
 */

import { paint } from '../tui/cli-design.js';
import { BRAND_DIAMOND, SEP } from '../tui/palette.js';
import type { PiInstance, PiTheme } from '../types.js';
import type { PlanCoordination, PlanStep, ReviewState } from './active-plan.js';
import { makeRenderer, truncateToWidth } from './render-helpers.js';
import { renderFrame } from '../tui/components.js';

export const COMPACTION_CHECKPOINT_TYPE = 'octocode-compaction-checkpoint';
export const AWARENESS_HANDOFF_TYPE = 'octocode-awareness-handoff';

const MAX_SUMMARY_LINES = 8;
const MAX_LIST_ITEMS = 6;

// ─── Details payloads (renderer-only; never enter the LLM context) ───────────

export interface CompactionCheckpointDetails {
  /** Short human label for the checkpoint (entry id or reason). */
  label: string;
  reason?: string;
  tokensBefore?: number;
  /** Pi's post-compaction estimate; actual provider usage is unknown until the next response. */
  estimatedTokensAfter?: number;
  reclaimedTokens?: number;
  reclaimedPercent?: number;
  /** True when the extension (not pi/user) triggered the compaction. */
  fromExtension?: boolean;
  readFiles?: string[];
  modifiedFiles?: string[];
  /** Markdown artifact written under Octocode home for reopening after compaction. */
  artifactPath?: string;
  /** Stable pointer to the most recent compaction artifact. */
  latestArtifactPath?: string;
  rehydrationLedgerPath?: string;
  /** Compaction summary text (shown truncated when expanded). */
  summary?: string;
  /** Snapshot of the active plan state at compaction time. */
  activePlan?: {
    total: number;
    done: number;
    running?: string;
  };
  /** One versioned recovery projection shared by the LLM marker and markdown artifact. */
  continuation?: {
    version: 1;
    plan?: {
      review: ReviewState;
      coordination: PlanCoordination;
      steps: PlanStep[];
    };
  };
}

export interface AwarenessHandoffDetails {
  /** Short human label for the handoff. */
  label: string;
  from?: string;
  to?: string;
  goal?: string;
  status?: string;
  notes?: string[];
  artifacts?: string[];
}

// ─── Card builders (pure) ─────────────────────────────────────────────────────

function fit(line: string, width: number): string {
  return truncateToWidth(line, Math.max(1, width));
}

function cardHeader(title: string, label: string, theme: PiTheme | undefined): string {
  return `${paint(theme, 'brand', BRAND_DIAMOND)} ${paint(theme, 'title', title)}${paint(theme, 'dim', SEP)}${paint(theme, 'brand', label)}`;
}

function listLine(title: string, items: string[], theme: PiTheme | undefined): string | undefined {
  if (items.length === 0) return undefined;
  const shown = items.slice(0, MAX_LIST_ITEMS).join(', ');
  const more = items.length > MAX_LIST_ITEMS ? `, … +${items.length - MAX_LIST_ITEMS}` : '';
  return `${paint(theme, 'muted', `${title} (${items.length}):`)} ${paint(theme, 'path', shown)}${paint(theme, 'dim', more)}`;
}

function compactionStatLine(details: CompactionCheckpointDetails, theme: PiTheme | undefined): string | undefined {
  const parts = [
    details.reason ? `reason: ${details.reason}` : '',
    details.tokensBefore !== undefined ? `tokens before: ${details.tokensBefore}` : '',
    details.estimatedTokensAfter !== undefined ? `after: ~${details.estimatedTokensAfter}` : '',
    details.reclaimedTokens !== undefined
      ? `reclaimed: ~${details.reclaimedTokens}${details.reclaimedPercent !== undefined ? ` (${details.reclaimedPercent}%)` : ''}`
      : '',
    details.fromExtension === undefined ? '' : `source: ${details.fromExtension ? 'octocode' : 'pi'}`,
    details.activePlan
      ? `plan: ${details.activePlan.done}/${details.activePlan.total}${details.activePlan.running ? ` · ${details.activePlan.running}` : ''}`
      : '',
  ].filter(Boolean);
  if (parts.length === 0) return undefined;
  return paint(theme, 'dim', parts.join(SEP));
}

/**
 * Branded compaction-checkpoint card. Collapsed = 1–2 lines (header + stat
 * line); expanded = full box with file lists and a summary excerpt.
 */
export function buildCompactionCard(
  details: CompactionCheckpointDetails,
  expanded: boolean,
  theme: PiTheme | undefined,
  width: number,
): string[] {
  const label = details.label || 'checkpoint';
  const stat = compactionStatLine(details, theme);

  if (!expanded) {
    const lines = [cardHeader('Compaction checkpoint', label, theme)];
    if (stat) lines.push(`  ${stat}`);
    return lines.map((line) => fit(line, width));
  }

  const body: (string | undefined)[] = [];
  if (stat) body.push(stat);
  const read = listLine('read files', details.readFiles ?? [], theme);
  if (read) body.push(read);
  const modified = listLine('modified files', details.modifiedFiles ?? [], theme);
  if (modified) body.push(modified);
  if (details.artifactPath) {
    body.push(`${paint(theme, 'muted', 'doc:')} ${paint(theme, 'path', details.artifactPath)}`);
  }
  if (details.summary) {
    const summaryLines = details.summary.split('\n');
    for (const line of summaryLines.slice(0, MAX_SUMMARY_LINES)) {
      body.push(paint(theme, 'dim', line));
    }
    const omitted = summaryLines.length - MAX_SUMMARY_LINES;
    if (omitted > 0) {
      body.push(paint(theme, 'muted', `… ${omitted} more summary line${omitted === 1 ? '' : 's'}`));
    }
  }
  return renderFrame({
    title: cardHeader('Compaction checkpoint', label, theme),
    body: body.filter((line): line is string => Boolean(line)),
    footer: 'context compacted — checkpoint ready',
    borderToken: 'brand',
  }, { width, theme });
}

/**
 * Branded awareness-handoff card. Collapsed = 1–2 lines (header + route);
 * expanded = full box with goal, status, notes, and artifacts.
 */
export function buildHandoffCard(
  details: AwarenessHandoffDetails,
  expanded: boolean,
  theme: PiTheme | undefined,
  width: number,
): string[] {
  const label = details.label || 'handoff';
  const routeParts = [
    details.from || details.to ? `${details.from ?? '?'} → ${details.to ?? '?'}` : '',
    details.status ? `status: ${details.status}` : '',
  ].filter(Boolean);
  const route = routeParts.length > 0 ? paint(theme, 'dim', routeParts.join(SEP)) : undefined;

  if (!expanded) {
    const lines = [cardHeader('Awareness handoff', label, theme)];
    if (route) lines.push(`  ${route}`);
    return lines.map((line) => fit(line, width));
  }

  const body: string[] = [];
  if (route) body.push(route);
  if (details.goal) {
    body.push(`${paint(theme, 'muted', 'goal:')} ${paint(theme, 'dim', details.goal)}`);
  }
  for (const note of (details.notes ?? []).slice(0, MAX_LIST_ITEMS)) {
    body.push(paint(theme, 'dim', `- ${note}`));
  }
  const omittedNotes = (details.notes?.length ?? 0) - MAX_LIST_ITEMS;
  if (omittedNotes > 0) {
    body.push(paint(theme, 'muted', `… ${omittedNotes} more note${omittedNotes === 1 ? '' : 's'}`));
  }
  const artifacts = listLine('artifacts', details.artifacts ?? [], theme);
  if (artifacts) body.push(artifacts);
  return renderFrame({
    title: cardHeader('Awareness handoff', label, theme),
    body,
    footer: 'awareness handed off',
    borderToken: 'brand',
  }, { width, theme });
}

// ─── Renderer registration ────────────────────────────────────────────────────

function detailsOf(message: unknown): Record<string, unknown> {
  if (message && typeof message === 'object') {
    const details = (message as { details?: unknown }).details;
    if (details && typeof details === 'object' && !Array.isArray(details)) {
      return details as Record<string, unknown>;
    }
  }
  return {};
}

/**
 * Register the branded renderers for both Octocode custom-message types.
 * First-registrant wins in pi, so this should run once at extension setup.
 */
export function registerOctocodeMessageRenderers(pi: PiInstance): void {
  pi.registerMessageRenderer?.(COMPACTION_CHECKPOINT_TYPE, (message, options, theme) =>
    makeRenderer((width) =>
      buildCompactionCard(
        detailsOf(message) as unknown as CompactionCheckpointDetails,
        options?.expanded === true,
        theme,
        width,
      ),
    ),
  );
  pi.registerMessageRenderer?.(AWARENESS_HANDOFF_TYPE, (message, options, theme) =>
    makeRenderer((width) =>
      buildHandoffCard(
        detailsOf(message) as unknown as AwarenessHandoffDetails,
        options?.expanded === true,
        theme,
        width,
      ),
    ),
  );
}

// ─── Emitters ─────────────────────────────────────────────────────────────────
//
// CRITICAL: `content` participates in the LLM context — keep it ONE terse line.
// All rich data rides in `details`, which only the renderer sees. No
// triggerTurn: these are passive transcript records, never turn starters.

export function renderCompactionContextMarker(details: CompactionCheckpointDetails): string {
  const bounded = (value: string | undefined, limit: number): string | undefined => {
    const normalized = value?.replace(/\s+/g, ' ').trim();
    return normalized ? normalized.slice(0, limit) : undefined;
  };
  const plan = details.continuation?.plan;
  const references = plan
    ? [...new Set([
        plan.review.rfcPath,
        ...plan.steps.flatMap((step) => step.paths ?? []),
      ].filter((value): value is string => Boolean(value)).map((value) => bounded(value, 256)!))].slice(0, 16)
    : [];
  const planPointer = plan
    ? {
        phase: plan.review.phase,
        snapshot: bounded(plan.review.branchSnapshotId, 128),
        generation: plan.review.generation,
        ...(plan.review.revision ? { revision: bounded(plan.review.revision, 128) } : {}),
        ...(plan.review.acceptedRevision ? { acceptedRevision: bounded(plan.review.acceptedRevision, 128) } : {}),
        activeStepIds: plan.steps.filter((step) => step.status === 'doing').map((step) => bounded(step.id, 128)!).slice(0, 20),
        ...(references.length > 0 ? { references } : {}),
      }
    : undefined;
  const payload = {
    checkpoint: bounded(details.label, 256),
    ...(details.reason ? { reason: bounded(details.reason, 64) } : {}),
    ...(details.tokensBefore !== undefined ? { tokensBefore: details.tokensBefore } : {}),
    ...(details.estimatedTokensAfter !== undefined ? { estimatedTokensAfter: details.estimatedTokensAfter } : {}),
    ...(details.latestArtifactPath ? { artifact: bounded(details.latestArtifactPath, 512) } : {}),
    ...(planPointer ? { plan: planPointer } : {}),
    ...(details.summary?.trim() ? { summaryAvailable: true, summary: bounded(details.summary, 3500) } : {}),
  };
  return `<octocode_compaction_context>${JSON.stringify(payload)}</octocode_compaction_context>`;
}

export function emitCompactionCheckpoint(pi: PiInstance, details: CompactionCheckpointDetails): void {
  pi.sendMessage?.({
    customType: COMPACTION_CHECKPOINT_TYPE,
    content: renderCompactionContextMarker(details),
    display: true,
    details,
  });
}

export function emitAwarenessHandoff(pi: PiInstance, details: AwarenessHandoffDetails): void {
  pi.sendMessage?.({
    customType: AWARENESS_HANDOFF_TYPE,
    content: `Awareness handoff recorded: ${details.label}`,
    display: true,
    details,
  });
}
