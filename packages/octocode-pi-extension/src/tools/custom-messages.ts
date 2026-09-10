import { truncateToWidth } from '../tui/width.js';
import { stripVTControlCharacters } from 'node:util';
import { wrapTextWithAnsi } from '@earendil-works/pi-tui';
import { AWARENESS_PEER_EVENT_MESSAGE_TYPE } from '@octocodeai/octocode-awareness';
/**
 * custom-messages — branded transcript cards for Octocode lifecycle moments.
 *
 * Lifecycle and peer messages get dedicated renderers instead of pi's default
 * plain custom-message row:
 *  - compaction checkpoints (emitted from compaction-hooks on session_compact)
 *  - accepted Awareness peer messages (the existing attributed context payload),
 *    including peer handoffs (details.messageClass === 'handoff')
 * Recovery receipts use the state-entry renderer and add no model context.
 *
 * Contract discipline: `content` on a custom message ENTERS THE LLM CONTEXT.
 * Compaction therefore emits one bounded, explicit marker containing the
 * summary checkpoint and active-plan pointer; renderer-only detail stays in
 * `details`. Cards follow the box/rule visual language of
 * cli-design (`╭─ ◆ … │ … ╰─`) so transcript cards and tool rows read as one
 * system.
 */


import { BRAND_DIAMOND, SEP, paint, sanitizeLine } from '../tui/palette.js';
import type { PiInstance, PiTheme } from '../types.js';
import type { PlanCoordination, PlanStep, ReviewState } from './planning/plan-types.js';
import { makeComponentRenderer } from './render-helpers.js';
import { renderFrame } from '../tui/components.js';
import {
  EXECUTION_TRANSCRIPT_ENTRY_TYPE,
  isExecutionEvent,
  type ExecutionEvent,
} from './execution-events.js';

export const COMPACTION_CHECKPOINT_TYPE = 'octocode-compaction-checkpoint';

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

// ─── Card builders (pure) ─────────────────────────────────────────────────────

function fit(line: string, width: number): string {
  return truncateToWidth(line, Math.max(1, width));
}

function cardHeader(title: string, label: string, theme: PiTheme | undefined): string {
  return `${paint(theme, 'brand', BRAND_DIAMOND)} ${paint(theme, 'title', title)}${paint(theme, 'dim', SEP)}${paint(theme, 'muted', label)}`;
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
      body.push(paint(theme, 'bright', line));
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
    borderToken: 'dim',
  }, { width, theme });
}

/** Peer messages reuse their existing attributed content; rendering adds no context. */
export function buildPeerEventCard(message: unknown, expanded: boolean, theme: PiTheme | undefined, width: number): string[] {
  const details = detailsOf(message);
  const content = message && typeof message === 'object' && typeof (message as { content?: unknown }).content === 'string'
    ? (message as { content: string }).content : '';
  const clean = stripVTControlCharacters(content);
  const attribution = /^\[peer:([^;\n]+); class:[^;\n]+; authority:data\]\n/.exec(clean);
  const from = attribution?.[1] ?? 'peer';
  const body = (attribution ? clean.slice(attribution[0].length) : clean).split('\n').map(sanitizeLine);
  const kind = details['messageClass'] === 'blocking' ? 'blocking' : details['messageClass'] === 'handoff' ? 'handoff' : 'message';
  const title = cardHeader(`Awareness ${kind}`, sanitizeLine(from), theme);
  if (!expanded) {
    return [title, ...body.filter(Boolean).slice(0, 1).map((line) => `  ${paint(theme, 'bright', line)}`),
      paint(theme, kind === 'blocking' ? 'warning' : 'muted', '  Peer data · Ctrl+O expands'),
    ].map((line) => fit(line, width));
  }
  return renderFrame({
    title,
    body: body.flatMap((line) => wrapTextWithAnsi(line, Math.max(1, width - 3))).map((line) => paint(theme, 'bright', line)),
    footer: 'Peer data',
    borderToken: kind === 'blocking' ? 'warning' : 'dim',
  }, { width, theme });
}

/** A state-entry view: recovery receipts are inspectable without spending model tokens. */
export function buildRecoveryCard(receipt: unknown, expanded: boolean, theme: PiTheme | undefined, width: number): string[] {
  if (!receipt || typeof receipt !== 'object') return [];
  const data = receipt as Record<string, unknown>;
  const outcome = data['outcome'];
  if (!['restored', 'expired', 'corrupt', 'identity-mismatch'].includes(String(outcome))) return [];
  const strings = (key: string): string[] => Array.isArray(data[key])
    ? (data[key] as unknown[]).filter((item): item is string => typeof item === 'string').map((item) => sanitizeLine(stripVTControlCharacters(item))) : [];
  const restored = strings('restored');
  const validated = strings('validated');
  const corrupt = strings('corrupt');
  const overBudget = strings('overBudget');
  const stale = strings('stale');
  const pending = strings('pendingInteractionIds').length;
  const partial = outcome !== 'restored' || corrupt.length > 0 || overBudget.length > 0;
  const label = outcome === 'restored' ? partial ? 'partial' : 'checked'
    : outcome === 'expired' ? 'expired checkpoint'
      : outcome === 'corrupt' ? 'unreadable checkpoint' : 'different session';
  const title = cardHeader('Context recovery', label, theme);
  const stats = `${pending ? `${pending} pending decision${pending === 1 ? '' : 's'} · ` : ''}${restored.length} restored · ${validated.length} validated`;
  if (!expanded) return [title, ...wrapTextWithAnsi(stats, Math.max(1, width - 2))
    .map((line) => paint(theme, partial ? 'warning' : 'muted', `  ${line}`))].map((line) => fit(line, width));
  const body = [stats,
    listLine('restored', restored, theme),
    listLine('current sources changed', stale, theme),
    listLine('unreadable', corrupt, theme),
    listLine('over budget', overBudget, theme),
    pending ? 'Pending decisions still require an explicit answer.' : undefined,
  ].filter((line): line is string => Boolean(line));
  return renderFrame({ title, body: body.flatMap((line) => wrapTextWithAnsi(line, Math.max(1, width - 3))), footer: partial ? 'Some checkpoint context was not restored' : 'Current sources checked', borderToken: partial ? 'warning' : 'dim' }, { width, theme });
}

function executionEventBody(
  event: ExecutionEvent,
  expanded: boolean,
  theme: PiTheme | undefined,
): { title: string; body: string[]; footer?: string; warning?: boolean } | undefined {
  if (event.type === 'plan.updated') {
    const tasks = event.payload.tasks;
    const done = tasks.filter(task => task.status === 'done').length;
    const active = tasks.filter(task => task.status === 'doing').length;
    const blocked = tasks.filter(task => task.status === 'blocked').length;
    const ready = tasks.filter(task => task.status === 'todo').length;
    const counts = `${done} done${active ? `${SEP}${active} active` : ''}${ready ? `${SEP}${ready} ready` : ''}${blocked ? `${SEP}${blocked} blocked` : ''}`;
    const marks: Record<string, string> = {
      done: '✓',
      doing: '⟳',
      todo: '○',
      blocked: '!',
    };
    return {
      title: cardHeader('Plan', sanitizeLine(event.payload.phase.replaceAll('_', ' ')), theme),
      body: [
        paint(theme, blocked ? 'warning' : 'muted', counts),
        ...(expanded
          ? tasks.map(task =>
              `${paint(theme, task.status === 'done' ? 'success' : task.status === 'blocked' ? 'warning' : task.status === 'doing' ? 'brand' : 'dim', marks[task.status] ?? '○')} ${paint(theme, 'bright', sanitizeLine(stripVTControlCharacters(task.title)))}`
            )
          : []),
      ],
      footer: expanded ? 'Plan progress' : undefined,
      warning: blocked > 0,
    };
  }
  if (event.type === 'agent.message') {
    const direction = event.payload.direction === 'from-agent' ? '←' : '→';
    return {
      title: cardHeader(
        'Agent message',
        `${sanitizeLine(event.payload.name)}${SEP}${event.payload.action}`,
        theme,
      ),
      body: [
        `${paint(theme, 'brand', direction)} ${paint(theme, 'bright', sanitizeLine(stripVTControlCharacters(event.payload.preview)))}`,
        ...(expanded && event.payload.planStep
          ? [paint(theme, 'muted', `plan: ${sanitizeLine(event.payload.planStep)}`)]
          : []),
      ],
      footer: expanded ? 'Worker communication' : undefined,
    };
  }
  if (event.type === 'agent.transition') {
    const warning = ['blocked', 'failed'].includes(event.payload.to);
    const transition = event.payload.from
      ? `${event.payload.from} → ${event.payload.to}`
      : event.payload.to;
    return {
      title: cardHeader(
        'Agent',
        `${sanitizeLine(event.payload.name)}${SEP}${sanitizeLine(event.payload.to)}`,
        theme,
      ),
      body: [
        ...(event.payload.summary
          ? [paint(theme, 'bright', sanitizeLine(stripVTControlCharacters(event.payload.summary)))]
          : [paint(theme, warning ? 'warning' : event.payload.to === 'done' ? 'success' : 'muted', transition)]),
        ...(expanded && event.payload.summary
          ? [paint(theme, warning ? 'warning' : event.payload.to === 'done' ? 'success' : 'muted', transition)]
          : []),
        ...(expanded && event.payload.planStep
          ? [paint(theme, 'muted', `plan: ${sanitizeLine(event.payload.planStep)}`)]
          : []),
      ],
      footer: expanded ? 'Worker state' : undefined,
      warning,
    };
  }
  return undefined;
}

/** Render durable execution facts as transcript cards without adding model context. */
export function buildExecutionEventCard(
  value: unknown,
  expanded: boolean,
  theme: PiTheme | undefined,
  width: number,
): string[] {
  if (!isExecutionEvent(value)) return [];
  const card = executionEventBody(value, expanded, theme);
  if (!card) return [];
  if (!expanded)
    return [card.title, ...card.body.slice(0, 1).map(line => `  ${line}`)].map(line => fit(line, width));
  return renderFrame(
    {
      title: card.title,
      body: card.body.flatMap(line =>
        wrapTextWithAnsi(line, Math.max(1, width - 3))
      ),
      footer: card.footer,
      borderToken: card.warning ? 'warning' : 'dim',
    },
    { width, theme },
  );
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
 * Register the branded renderers for Octocode lifecycle and peer messages.
 * First-registrant wins in pi, so this should run once at extension setup.
 */
export function registerOctocodeMessageRenderers(pi: PiInstance): void {
  pi.registerMessageRenderer?.(AWARENESS_PEER_EVENT_MESSAGE_TYPE, (message, options, theme) =>
    makeComponentRenderer((_props, { width }) => buildPeerEventCard(message, options?.expanded === true, theme, width), undefined),
  );
  pi.registerMessageRenderer?.(COMPACTION_CHECKPOINT_TYPE, (message, options, theme) =>
    makeComponentRenderer((_props, { width: width }) => buildCompactionCard(
        detailsOf(message) as unknown as CompactionCheckpointDetails,
        options?.expanded === true,
        theme,
        width,
      ), undefined),
  );
  pi.registerEntryRenderer?.(
    EXECUTION_TRANSCRIPT_ENTRY_TYPE,
    (entry, options, theme) =>
      makeComponentRenderer(
        (_props, { width }) =>
          buildExecutionEventCard(
            entry.data,
            options?.expanded === true,
            theme,
            width,
          ),
        undefined,
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
    // Pi's native compaction summary is already retained in model context. Keep
    // the body in details/artifacts for UI inspection without replaying it in
    // this model-visible checkpoint marker.
    ...(details.summary?.trim() ? { summaryAvailable: true } : {}),
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
