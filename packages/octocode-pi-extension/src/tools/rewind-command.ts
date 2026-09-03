/**
 * rewind-command — /octocode-rewind + the before-prompt checkpoint input hook (F7).
 *
 * REQUIRED WIRING in src/index.ts (this module never edits index.ts itself):
 *
 *   import { initCheckpointStore, type CheckpointEngine } from './tools/checkpoints.js';
 *   import { createCheckpointInputHook, registerRewindCommand } from './tools/rewind-command.js';
 *
 *   let checkpointEngine: Promise<CheckpointEngine | undefined> | undefined;
 *   const getEngine = (ctx?: PiContext) => {
 *     checkpointEngine ??= initCheckpointStore(ctx?.cwd ?? process.cwd()).catch(() => undefined);
 *     return checkpointEngine;
 *   };
 *   pi.on('input', createCheckpointInputHook({ getEngine }));   // auto-snapshot before user prompts
 *   registerRewindCommand(pi, { getEngine });                   // /octocode-rewind [list | restore <id>]
 *
 * Behaviour:
 * - The input hook snapshots ONLY user-sourced prompts: it skips source
 *   'extension', steering input, slash commands, and empty text. The snapshot is
 *   fire-and-forget — the hook returns { action: 'continue' } immediately and
 *   never blocks input on git.
 * - /octocode-rewind with no args opens a checkpoint picker overlay, then a
 *   second stage: restore files | restore files + rewind conversation |
 *   show diff | cancel. Conversation rewind uses ctx.navigateTree when the host
 *   provides it and DEGRADES to files-only (with a message) when it does not.
 */

import type { PiCommandContext, PiContext, PiInstance, NotifyFn } from '../types.js';
import type { CheckpointInfo, DiffStatEntry, SnapshotResult } from './checkpoints.js';
import { runSelectOverlay, type SelectOverlayItem, type SelectOverlayOptions } from './ui-overlays.js';

// ─── Deps ────────────────────────────────────────────────────────────────────

/** Minimal engine surface the command needs — satisfied by CheckpointEngine. */
export interface RewindEngine {
  snapshot(label: string, opts?: { entryId?: string }): Promise<SnapshotResult | undefined>;
  listCheckpoints(limit?: number): Promise<CheckpointInfo[]>;
  restoreFiles(id: string, paths?: string[]): Promise<void>;
  diffStat(id: string): Promise<DiffStatEntry[]>;
}

/** Lazy engine provider — index.ts memoizes initCheckpointStore behind this. */
export type EngineProvider = (
  ctx?: PiContext,
) => RewindEngine | undefined | Promise<RewindEngine | undefined>;

export type OverlayRunner = (
  ctx: PiContext | undefined,
  opts: SelectOverlayOptions,
) => Promise<string | null | undefined>;

export interface RewindCommandDeps {
  getEngine: EngineProvider;
  /** Notification sink (default ctx.ui.notify). Injectable for tests. */
  notify?: NotifyFn;
  /** Overlay runner (default runSelectOverlay). Injectable for tests. */
  runOverlay?: OverlayRunner;
}

const defaultNotify: NotifyFn = (ctx, msg, level) => {
  if (ctx?.hasUI) ctx.ui?.notify?.(msg, level);
};

// ─── Pure helpers (exported for tests) ───────────────────────────────────────

const LABEL_PREFIX = 'before: ';
const LABEL_TEXT_CHARS = 40;

/** `before: <first 40 chars of the prompt, whitespace collapsed>` */
export function snapshotLabel(text: string): string {
  const collapsed = text.replace(/\s+/g, ' ').trim();
  return `${LABEL_PREFIX}${collapsed.slice(0, LABEL_TEXT_CHARS)}`;
}

export function buildCheckpointItems(checkpoints: CheckpointInfo[]): SelectOverlayItem[] {
  return checkpoints.map((cp) => ({
    value: cp.id,
    label: `${new Date(cp.ts).toLocaleString()} — ${cp.label || '(no label)'}`,
    description:
      `${cp.filesChanged} file${cp.filesChanged === 1 ? '' : 's'} · ${cp.id.slice(0, 8)}` +
      (cp.entryId ? ' · conversation' : ''),
  }));
}

export function formatCheckpointList(checkpoints: CheckpointInfo[]): string {
  if (checkpoints.length === 0) {
    return 'No checkpoints yet — they are created automatically before each prompt.';
  }
  const lines = checkpoints.map(
    (cp) =>
      `${cp.id.slice(0, 8)}  ${new Date(cp.ts).toLocaleString()}  ${cp.label || '(no label)'}` +
      ` (${cp.filesChanged} file${cp.filesChanged === 1 ? '' : 's'}${cp.entryId ? ', conversation' : ''})`,
  );
  return ['Checkpoints (newest first):', ...lines, 'Restore with /octocode-rewind restore <id>.'].join('\n');
}

export function formatDiffStat(entries: DiffStatEntry[]): string {
  if (entries.length === 0) return 'No differences between this checkpoint and the work tree.';
  return entries.map((e) => `${e.status} ${e.path}`).join('\n');
}

/** Current session leaf entry id, tolerant of hosts without getLeafId. */
export function leafEntryId(ctx: PiContext | undefined): string | undefined {
  const sm = ctx?.sessionManager as
    | { getLeafId?(): string | undefined; getBranch?(): unknown[] }
    | undefined;
  const leaf = sm?.getLeafId?.();
  if (typeof leaf === 'string' && leaf) return leaf;
  const branch = sm?.getBranch?.();
  if (Array.isArray(branch) && branch.length > 0) {
    const last = branch[branch.length - 1] as { id?: unknown } | undefined;
    if (typeof last?.id === 'string' && last.id) return last.id;
  }
  return undefined;
}

// ─── Input hook ──────────────────────────────────────────────────────────────

export interface CheckpointInputEvent {
  text?: string;
  source?: string;
  streamingBehavior?: string;
}

export type CheckpointInputHook = (
  event: CheckpointInputEvent,
  ctx: PiContext | undefined,
) => Promise<{ action: 'continue' }>;

/**
 * Input hook: fire-and-forget snapshot before every USER prompt. Skips
 * extension-sourced input (injection loops), steering, slash commands, and
 * empty text. NEVER awaits git — always returns { action: 'continue' }
 * immediately so input latency is untouched.
 */
export function createCheckpointInputHook(deps: Pick<RewindCommandDeps, 'getEngine'>): CheckpointInputHook {
  return async (event, ctx) => {
    const result = { action: 'continue' as const };
    const text = event?.text ?? '';
    if (event?.source === 'extension') return result;
    if (event?.streamingBehavior === 'steer') return result;
    if (!text.trim() || text.trimStart().startsWith('/')) return result;

    const entryId = leafEntryId(ctx);
    void (async () => {
      const engine = await deps.getEngine(ctx);
      await engine?.snapshot(snapshotLabel(text), entryId ? { entryId } : undefined);
    })().catch(() => undefined);

    return result;
  };
}

// ─── /octocode-rewind ────────────────────────────────────────────────────────

const STAGE_RESTORE_FILES = 'restore-files';
const STAGE_RESTORE_REWIND = 'restore-rewind';
const STAGE_SHOW_DIFF = 'show-diff';
const STAGE_CANCEL = 'cancel';

function stageItems(cp: CheckpointInfo | undefined, canNavigate: boolean): SelectOverlayItem[] {
  const rewindHint = !cp?.entryId
    ? ' (no conversation entry recorded — files only)'
    : !canNavigate
      ? ' (host lacks navigateTree — files only)'
      : '';
  return [
    { value: STAGE_RESTORE_FILES, label: 'restore files', description: 'Overwrite work-tree files from this checkpoint' },
    { value: STAGE_RESTORE_REWIND, label: 'restore files + rewind conversation', description: `Also move the session back${rewindHint}` },
    { value: STAGE_SHOW_DIFF, label: 'show diff', description: 'Name-status of checkpoint vs current files' },
    { value: STAGE_CANCEL, label: 'cancel', description: 'Do nothing' },
  ];
}

async function restoreWithOptionalRewind(
  engine: RewindEngine,
  ctx: PiCommandContext,
  cp: CheckpointInfo | undefined,
  id: string,
  wantRewind: boolean,
  notify: NotifyFn,
): Promise<void> {
  try {
    await engine.restoreFiles(id);
  } catch (error) {
    notify(ctx, `Restore failed: ${error instanceof Error ? error.message : String(error)}`, 'error');
    return;
  }
  if (!wantRewind) {
    notify(ctx, `Files restored from checkpoint ${id.slice(0, 8)}.`, 'info');
    return;
  }
  const entryId = cp?.entryId;
  if (!entryId || typeof ctx.navigateTree !== 'function') {
    const why = !entryId
      ? 'no conversation entry was recorded for this checkpoint'
      : 'this host does not support conversation rewind (navigateTree)';
    notify(ctx, `Files restored from ${id.slice(0, 8)}; conversation left in place — ${why}.`, 'warning');
    return;
  }
  const res = await ctx.navigateTree?.(entryId, { label: `octocode-rewind: ${cp?.label || id.slice(0, 8)}` });
  notify(
    ctx,
    res?.cancelled
      ? `Files restored from ${id.slice(0, 8)}; conversation rewind was cancelled.`
      : `Files restored and conversation rewound to checkpoint ${id.slice(0, 8)}.`,
    'info',
  );
}

/**
 * Register `/octocode-rewind`.
 * - no args → checkpoint picker overlay → restore files | restore + rewind conversation | show diff | cancel
 * - `list` → print checkpoints
 * - `restore <id>` → restore files from that checkpoint
 */
export function registerRewindCommand(pi: PiInstance, deps: RewindCommandDeps): void {
  const notify = deps.notify ?? defaultNotify;
  const overlay = deps.runOverlay ?? runSelectOverlay;

  pi.registerCommand?.('octocode-rewind', {
    description: 'Rewind files (and optionally the conversation) to an automatic checkpoint',
    getArgumentCompletions: (prefix: string) => {
      const items = [
        { value: 'list', label: 'list', description: 'List checkpoints' },
        { value: 'restore ', label: 'restore <id>', description: 'Restore files from a checkpoint id' },
      ];
      const filtered = items.filter((i) => i.value.startsWith(prefix));
      return filtered.length > 0 ? filtered : null;
    },
    handler: async (args: string, ctx: PiCommandContext) => {
      const engine = await deps.getEngine(ctx);
      if (!engine) {
        notify(ctx, 'Checkpoints unavailable (engine disabled or failed to initialize).', 'warning');
        return;
      }

      const trimmed = args.trim();
      if (trimmed === 'list') {
        notify(ctx, formatCheckpointList(await engine.listCheckpoints()), 'info');
        return;
      }
      const restoreMatch = /^restore\s+(\S+)$/.exec(trimmed);
      if (restoreMatch) {
        await restoreWithOptionalRewind(engine, ctx, undefined, restoreMatch[1]!, false, notify);
        return;
      }
      if (trimmed) {
        notify(ctx, 'Usage: /octocode-rewind [list | restore <id>]', 'warning');
        return;
      }

      const checkpoints = await engine.listCheckpoints();
      if (checkpoints.length === 0) {
        notify(ctx, formatCheckpointList(checkpoints), 'info');
        return;
      }
      const picked = await overlay(ctx, {
        title: 'Rewind to checkpoint',
        items: buildCheckpointItems(checkpoints),
      });
      if (picked === undefined) {
        // No interactive UI — point at the arg forms instead of dead-ending.
        notify(ctx, 'No interactive UI. Use /octocode-rewind list and /octocode-rewind restore <id>.', 'info');
        return;
      }
      if (picked === null) return; // cancelled

      const cp = checkpoints.find((c) => c.id === picked);
      const action = await overlay(ctx, {
        title: `Checkpoint ${picked.slice(0, 8)}${cp?.label ? ` — ${cp.label}` : ''}`,
        items: stageItems(cp, typeof ctx.navigateTree === 'function'),
        filter: false,
      });
      if (!action || action === STAGE_CANCEL) return;

      if (action === STAGE_SHOW_DIFF) {
        try {
          notify(ctx, formatDiffStat(await engine.diffStat(picked)), 'info');
        } catch (error) {
          notify(ctx, `Diff failed: ${error instanceof Error ? error.message : String(error)}`, 'error');
        }
        return;
      }
      await restoreWithOptionalRewind(engine, ctx, cp, picked, action === STAGE_RESTORE_REWIND, notify);
    },
  });
}
