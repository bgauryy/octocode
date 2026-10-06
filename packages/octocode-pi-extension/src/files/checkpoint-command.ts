import path from 'node:path';
import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import { sessionDir } from '../shared/home.js';
import { wordCompletions, type Subcommands } from '../shared/commands.js';
import { withDialog } from '../shared/locks.js';
import { plural } from '../shared/render.js';
import { inheritCheckpoints, sessionIdOf } from './checkpoint-store.js';
import type { Checkpoints, RewindResult } from './checkpoint.js';

/*
 * Checkpoints wired to the session (the store is checkpoint.ts): turns, `/octocode rewind`, anchor labels, and the
 * restore offer before a fork or a tree move.
 */

/** The label Octocode puts on turns that changed files ("✎ 3 files"); any other label is the user's and is left alone. */
const CHANGED_LABEL_MARK = '✎';
const CHANGED_LABEL = /^✎ \d+ files?$/;

/** A session's checkpoint folder: `<session folder>/checkpoints` (not created). */
const checkpointDir = (id: string): string => path.join(sessionDir(id), 'checkpoints');

/** The parent session file of a fork: the fork event's, else the session header's (`pi --fork <file>` starts as 'startup'). */
function parentSessionFile(event: { reason?: string; previousSessionFile?: string } | undefined, ctx: ExtensionContext): string | undefined {
  if (event?.reason === 'fork' && event.previousSessionFile) return event.previousSessionFile;
  return ctx.sessionManager.getHeader?.()?.parentSession || undefined;
}

function describe(result: RewindResult, cwd: string, turns: number): string {
  const show = (files: string[]) => files.map((file) => path.relative(cwd, file) || file).join(', ');
  const lines = [`Rewound ${turns} turn${turns === 1 ? '' : 's'}: restored ${result.restored.length} file${result.restored.length === 1 ? '' : 's'}${result.restored.length ? ` (${show(result.restored)})` : ''}.`];
  if (result.skipped.length) lines.push(`Left alone, changed since the agent's edit: ${show(result.skipped)}.`);
  return lines.join('\n');
}

/** The entry a tree navigation to `targetId` lands on: Pi puts a user or custom message back in the editor and stops at its parent. */
function treeLeaf(ctx: ExtensionContext, targetId: string): string | null {
  const target = ctx.sessionManager.getEntry(targetId);
  if (!target) return targetId;
  const editable = (target.type === 'message' && target.message.role === 'user') || target.type === 'custom_message';
  return editable ? target.parentId : targetId;
}

/**
 * Wire checkpoints to the session: one directory per session, a turn per user prompt, `/octocode rewind`, and a restore
 * offer before a fork or a tree navigation leaves turns that changed files.
 */
export function registerCheckpoints(pi: ExtensionAPI, checkpoints: Checkpoints, forget: (file: string) => void, commands: Subcommands): void {
  // A rewind turn is one user prompt (typed or over RPC), not every run (team messages and job reports are not turns).
  // The first run opens one even without a prompt event; a prompt typed mid-run (a steer or follow-up) opens its turn at
  // the next run, so edits in flight settle in the turn they were captured in.
  let [begun, pending] = [false, false];
  const begin = () => void (checkpoints.beginTurn(), (begun = true), (pending = false));
  pi.on('session_start', async (event, ctx) => {
    [begun, pending] = [false, false];
    const id = ctx.sessionManager.getSessionId();
    const dir = checkpointDir(id);
    // A fork starts with the parent's checkpoints on its branch (in-memory parents have no file and pass nothing on);
    // inheritCheckpoints does nothing once the fork has a journal, so a later resume inherits nothing again.
    const parentFile = parentSessionFile(event, ctx);
    const parent = parentFile ? await sessionIdOf(parentFile) : undefined;
    if (parent && parent !== id) {
      const branch = ctx.sessionManager.getBranch().map((entry) => entry.id);
      await inheritCheckpoints(checkpointDir(parent), dir, branch).catch(() => 0);
    }
    checkpoints.open(dir);
  });
  const branchIds = (ctx: ExtensionContext, from?: string) => ctx.sessionManager.getBranch(from).map((entry) => entry.id);
  pi.on('input', async (event) => void (event.source === 'extension' ? undefined : event.streamingBehavior === undefined ? begin() : (pending = true)));
  pi.on('agent_start', async () => void ((!begun || pending) && begin()));

  // /tree and /fork show which turns changed files: each anchor (the turn's user message) is labelled "✎ N files".
  const relabel = (ctx: ExtensionContext, anchors: Iterable<string>) => {
    const files = checkpoints.filesByAnchor();
    for (const anchor of new Set(anchors)) {
      if (!ctx.sessionManager.getEntry(anchor)) continue;
      const count = files.get(anchor)?.size ?? 0;
      const want = count ? `${CHANGED_LABEL_MARK} ${plural(count, 'file')}` : undefined;
      const have = ctx.sessionManager.getLabel(anchor);
      if (have === want || (have !== undefined && !CHANGED_LABEL.test(have))) continue;
      pi.setLabel(anchor, want);
    }
  };
  // Anchors whose checkpoints were pruned since keep an Octocode label that no longer holds: relabel those too.
  const labelled = (ctx: ExtensionContext) => branchIds(ctx).filter((id) => CHANGED_LABEL.test(ctx.sessionManager.getLabel(id) ?? ''));
  pi.on('agent_end', async (_event, ctx) => relabel(ctx, [...checkpoints.filesByAnchor().keys(), ...labelled(ctx)]));

  const apply = async (turns: number[], ctx: ExtensionContext): Promise<RewindResult> => {
    const anchors = checkpoints.list().filter((entry) => turns.includes(entry.turn) && entry.anchor !== undefined).map((entry) => entry.anchor!);
    const result = await checkpoints.rewind(turns);
    relabel(ctx, anchors);
    for (const file of result.restored) forget(file);
    ctx.ui.notify(describe(result, ctx.cwd, turns.length), result.skipped.length ? 'warning' : 'info');
    return result;
  };

  commands.add('rewind', {
    description: 'rewind [turns] — undo file changes of the last N turns on this branch (default 1); files changed since are left alone',
    complete: (prefix) => wordCompletions(['1', '2', '3'], prefix),
    handler: async (args, ctx) => {
      const count = Math.max(1, Number.parseInt(args.trim(), 10) || 1);
      // Only this branch's turns: after a /tree move, turns on the branch left behind are not this conversation's.
      const turns = checkpoints.turnsOn(branchIds(ctx)).slice(0, count);
      if (turns.length === 0) {
        ctx.ui.notify('No file changes to rewind on this branch.', 'info');
        return;
      }
      const result = await apply(turns, ctx);
      // The model still believes its edits are in place: tell it on its next turn (never triggers one itself).
      if (result.restored.length > 0) {
        pi.sendMessage(
          { customType: 'octocode-rewind', content: `The user ran /octocode rewind. ${describe(result, ctx.cwd, turns.length)} Those files are back to their earlier content: read them again before editing.`, display: false },
          { triggerTurn: false },
        );
      }
    },
  });

  // The restore is asked before a fork or tree move but done only once the move happened: the move can still be
  // cancelled (Esc during the branch summary, a fork Pi refuses), and the files must then stay as they are.
  let chosen: number[] | undefined;
  const offer = async (turns: number[], ctx: ExtensionContext, signal?: AbortSignal): Promise<void> => {
    chosen = undefined;
    if (turns.length === 0) return;
    const files = new Set(checkpoints.list().filter((entry) => turns.includes(entry.turn)).map((entry) => entry.path)).size;
    const yes = 'Yes, restore files to that point';
    const title = `The agent changed ${files} file${files === 1 ? '' : 's'} after this point. Restore them?`;
    const choice = await withDialog(() => ctx.ui.select(title, [yes, 'No, keep current files'], signal ? { signal } : undefined), signal);
    if (choice === yes) chosen = turns;
  };
  const restoreChosen = async (ctx: ExtensionContext) => {
    const turns = chosen;
    chosen = undefined;
    if (turns) await apply(turns, ctx);
  };

  pi.on('session_before_fork', async (event, ctx) => {
    if (ctx.hasUI) await offer(checkpoints.turnsFrom(branchIds(ctx), event.entryId), ctx);
    return undefined;
  });
  // Pi tears the parent session down only once the fork is certain; its checkpoints are still open here.
  pi.on('session_shutdown', async (event, ctx) => (event.reason === 'fork' ? restoreChosen(ctx) : void (chosen = undefined)));

  // /tree moves the leaf in place: turns on the current branch that the new position does not include are abandoned.
  pi.on('session_before_tree', async (event, ctx) => {
    if (!ctx.hasUI) return undefined;
    const leaf = treeLeaf(ctx, event.preparation.targetId);
    const keep = new Set(leaf === null ? [] : branchIds(ctx, leaf));
    await offer(checkpoints.turnsLeaving(branchIds(ctx), keep), ctx, event.signal);
    return undefined;
  });
  pi.on('session_tree', async (_event, ctx) => restoreChosen(ctx));
}
