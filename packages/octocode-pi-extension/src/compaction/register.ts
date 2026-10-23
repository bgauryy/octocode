import type { ExtensionAPI, ExtensionContext, SessionBeforeCompactEvent, SessionCompactEvent } from '@earendil-works/pi-coding-agent';
import { contextReads, seenFiles, sessionCleared, type WorkedFile } from '../files/reads.js';
import type { FileGuard } from '../files/guard.js';
import { chooseSessionCacheRetention } from '../shared/env.js';
import { interactive } from '../shared/interactive.js';
import { isRecord } from '../shared/util.js';
import { reserveTokensFor } from '../shared/tokens.js';
import { parts, type ContextEntry } from './images.js';
import { clearColdResults } from './clearing.js';
import { requestMessages } from './thinking.js';
import { PromptCacheLifetimes, promptCacheTtlMs, warmingDecision } from './cache.js';
import { recordCompactionNote } from './record.js';
import { TooLargeRecovery } from './recovery.js';
import { RESTORE_SHARE, RESTORE_TOTAL_CHARS, deliverRestoredFiles, recentFiles, settleRestoredFiles, type PendingRestore } from './restore.js';
import { THRASH_MESSAGE, ThrashBreaker, contextTokens, needsDownshiftCompaction, toolDefinitionChars } from './pacing.js';
import type { Api, Model } from '@earendil-works/pi-ai';
import { carryFileLists, splitSummary, summaryMethodOf, windowBudget, writeSummary } from './summary.js';
import { ArchiveCapture, deliverArchivePointer, forkLineage, sessionRecordOf, summaryLabel } from './archive.js';
import { sessionLineage, setSessionLineage } from '../shared/archive.js';
import { writeSessionRecord } from '../shared/session-record.js';
import { notify } from '../shared/commands.js';
import { errorMessage } from '../shared/util.js';

/**
 * Tool results and call arguments stay verbatim until Pi compacts (a long result is bounded and spilled to a file when
 * written), with two exceptions: after an idle gap longer than the prompt-cache lifetime the old results of re-fetchable
 * tools are replaced by placeholders naming their saved files (`clearing.ts`), and a repeated read of an unchanged file
 * gets a short note (`files/dedupe.ts`). Octocode also supplies prompt-cache lifetimes and warming decisions, strips
 * images from a request rejected as too large (HTTP 413, else the session is stuck), and writes the compaction summary
 * (`summary.ts`). Each compaction's summarized messages are also filed as a searchable Markdown segment once it
 * succeeded (`archive.ts`).
 */

/**
 * Files whose latest result in the context is among the `edited` entries (413 recovery replaced their images): their
 * content left the model's view, so the file guard asks for a fresh read before they are changed.
 */
function editedReadPaths(entries: ContextEntry[], edited: ReadonlySet<string>, cwd: string): string[] {
  return [...seenFiles(entries, cwd).values()].filter((seen) => edited.has(seen.id)).map((seen) => seen.file);
}

export function registerCompaction(pi: ExtensionAPI, guard: FileGuard, isSubagent = false): void {
  const tooLarge = new TooLargeRecovery();
  const lifetimes = new PromptCacheLifetimes();
  const thrash = new ThrashBreaker();
  const archive = new ArchiveCapture();
  /** The model a pending downshift compaction summarizes on (the one the context still fits). */
  let downshiftFrom: Model<Api> | undefined;
  let session: string | undefined;
  pi.on('cache_warming_decision', async (event, ctx) => warmingDecision(event, ctx, promptCacheTtlMs(lifetimes.of(ctx.model))));
  pi.on('turn_end', async (event, ctx) => {
    lifetimes.observe(pi, ctx, event.message);
    thrash.turnEnded();
    const current = ctx.sessionManager.getSessionId();
    if (current !== session) {
      session = current;
      tooLarge.reset();
    }
    const entries = event.context.contextEntries.flatMap((entry) =>
      entry.sourceEntry.type === 'message' && entry.messages.length === 1 ? [{ id: entry.sourceEntry.id, message: entry.messages[0] }] : [],
    );
    const recovery = tooLarge.onTurnEnd(event.message, event.messageEntryId, entries, ctx);
    if (!recovery) return undefined;
    for (const file of editedReadPaths(entries, new Set(recovery.map((edit) => edit.targetId)), ctx.cwd)) guard.forget(file);
    return { entries: [...event.entries, ...recovery] };
  });
  pi.on('agent_before_settle', async (event) => (tooLarge.shouldRetry(event.outcome) ? { continue: true } : undefined));
  // The files worked on in the summarized part, most recent first, come back after the compaction.
  let restoreCandidates: WorkedFile[] = [];
  let restoreBudget = RESTORE_TOTAL_CHARS;
  let pendingRestore: PendingRestore | undefined;
  pi.on('session_before_compact', async (event, ctx) => {
    if (event.reason !== 'manual' && thrash.tripped()) {
      recordCompactionNote(pi, 'thrash-stop', THRASH_MESSAGE);
      if (ctx.hasUI) notify(ctx.ui, THRASH_MESSAGE, 'error');
      ctx.abort();
      return { cancel: true };
    }
    archive.capture(event);
    restoreCandidates = recentFiles(event.preparation, ctx.cwd);
    // Sized for the window the context lands in (after a downshift, the smaller model's), with Pi's reserve for it.
    restoreBudget = windowBudget(ctx.model?.contextWindow, event.preparation.settings.reserveTokens, RESTORE_SHARE, RESTORE_TOTAL_CHARS);
    // The file lists know only Pi's read/write/edit and drop a hook summary's lists: both are added first, so Pi's
    // default summary (when Octocode's fails) has them too.
    addFileToolOps(event.preparation);
    carryFileLists(event.preparation, event.branchEntries);
    const from = downshiftFrom;
    downshiftFrom = undefined;
    return writeSummary(pi, event, ctx, from);
  });

  // The retention is chosen before the session's first request, so pi-ai and Pi's warmer share one lifetime, and again
  // before every run, so a process that lost it (an earlier build, a reload mid-rebuild) heals at once.
  pi.on('session_start', async (_event, ctx) => {
    chooseSessionCacheRetention(interactive(ctx, isSubagent));
    tooLarge.reset();
    thrash.reset();
    downshiftFrom = undefined;
    restoreCandidates = [];
    pendingRestore = undefined;
    archive.discard();
    // A forked session's history before the fork is in its sources' archives: its own (`ownArchives`, the pointer).
    setSessionLineage(forkLineage(ctx.sessionManager));
    recordWarned = false;
  });
  // The session's record (goal, ending, repository) is rewritten after each run, so other sessions of the repository
  // can tell what it was for and how it ended (`archive` tool). A failure is warned about once and never thrown.
  let recordWarned = false;
  pi.on('agent_end', async (_event, ctx) => {
    try {
      const record = sessionRecordOf(ctx.sessionManager, sessionLineage());
      if (record) writeSessionRecord(record);
    } catch (error) {
      if (!recordWarned && ctx.hasUI) notify(ctx.ui, `The session record could not be written (${errorMessage(error)}).`, 'warning');
      recordWarned = true;
    }
  });
  // The retention is chosen again before every run (above), so the lifetime the `context` handler reads is the one in use.
  pi.on('before_agent_start', async (_event, ctx) => {
    chooseSessionCacheRetention(interactive(ctx, isSubagent));
  });
  // File contents summarized away by compaction, or on another branch, are no longer in context: the guard keeps only
  // the reads the context still shows, then the files worked on most recently are read back (and count as read).
  // Switching to a model whose window the context does not fit compacts first, on the previous model: the next
  // request on the new one would fail, and only the previous model can still read the whole history.
  pi.on('model_select', async (event, ctx) => {
    const reserve = reserveTokensFor(pi.getSettings(), event.model);
    if (!needsDownshiftCompaction(event.previousModel, event.model, contextTokens(ctx, toolDefinitionChars(pi)), reserve)) return;
    downshiftFrom = event.previousModel;
    const reason = `The context does not fit ${event.model.id}'s window; compacting first on ${event.previousModel!.id}.`;
    recordCompactionNote(pi, 'downshift', reason);
    if (ctx.hasUI) notify(ctx.ui, reason, 'info');
    ctx.compact({ onError: () => (downshiftFrom = undefined) });
  });
  pi.on('session_compact', async (event, ctx) => {
    thrash.compacted(event.reason === 'manual');
    archiveCompaction(pi, archive, event, ctx);
    guard.restore(contextReads(ctx));
    const pending = restoreCandidates;
    restoreCandidates = [];
    pendingRestore = deliverRestoredFiles(pi, pending, ctx, restoreBudget);
  });
  pi.on('session_compact_failed', async () => archive.discard());
  // The restored files count as read from the first model call that carries them (Pi may hold the message until the
  // running turn ends).
  pi.on('context', async (event, ctx) => {
    // Before every request, in a run or at its start: when the cache was last used longer ago than its lifetime (an idle
    // gap, or a long tool in the run), this request rewrites it anyway, so old results are cleared first.
    clearColdResults(pi, ctx, guard, promptCacheTtlMs(lifetimes.of(ctx.model)));
    if (pendingRestore && settleRestoredFiles(event.messages, pendingRestore, guard)) pendingRestore = undefined;
    // Every request shows the results cleared on this branch as their placeholders, the same way each time, and no
    // thinking a compaction made stale (`thinking.ts`).
    const messages = requestMessages(event.messages, ctx.model, sessionCleared(ctx.sessionManager));
    return messages === event.messages ? undefined : { messages };
  });
  pi.on('session_tree', async (_event, ctx) => {
    tooLarge.reset();
    restoreCandidates = [];
    pendingRestore = undefined;
    guard.restore(contextReads(ctx));
  });
}

type Preparation = SessionBeforeCompactEvent['preparation'];

/**
 * Files the messages captured for the compaction that just succeeded (`ArchiveCapture`), labelled with what its summary
 * says. A summary Octocode did not write has no pointer to the archive in its Recovery section, so one follows as a
 * message. A failure is reported and recorded, never thrown: the compaction itself is done.
 */
function archiveCompaction(pi: ExtensionAPI, archive: ArchiveCapture, event: SessionCompactEvent, ctx: ExtensionContext): void {
  if (!archive.captured) return;
  try {
    const entry = event.compactionEntry;
    const written = archive.write(entry, ctx.sessionManager.getSessionId(), summaryLabel(splitSummary(entry.summary).body), summaryMethodOf(entry.details) ?? (entry.fromHook ? 'extension' : 'pi-default'));
    if (written && summaryMethodOf(entry.details) === undefined) deliverArchivePointer(pi, written.n);
  } catch (error) {
    const reason = `The compaction succeeded, but its messages could not be archived (${errorMessage(error)}).`;
    recordCompactionNote(pi, 'archive-failed', reason);
    if (ctx.hasUI) notify(ctx.ui, reason, 'warning');
  }
}

/**
 * Adds every path the `file` tool changed in the summarized messages to Pi's modified-file list (in place). A call's
 * result is found by `toolCallId`; a path counts only when the call succeeded and its query's outcome is `ok`
 * (`details.outcomes` is aligned to `queries`; a call where every query failed has `isError` and no details).
 */
export function addFileToolOps(preparation: Pick<Preparation, 'fileOps' | 'messagesToSummarize' | 'turnPrefixMessages'>): void {
  const messages = [...preparation.messagesToSummarize, ...preparation.turnPrefixMessages];
  const results = new Map<string, Record<string, unknown>>();
  for (const message of messages) {
    if (isRecord(message) && message['role'] === 'toolResult' && typeof message['toolCallId'] === 'string') results.set(message['toolCallId'], message);
  }
  for (const message of messages) {
    for (const part of parts(message)) {
      if (!isRecord(part) || part['type'] !== 'toolCall' || part['name'] !== 'file' || !isRecord(part['arguments'])) continue;
      const result = typeof part['id'] === 'string' ? results.get(part['id']) : undefined;
      if (!result || result['isError'] === true) continue;
      const outcomes = isRecord(result['details']) && Array.isArray(result['details']['outcomes']) ? result['details']['outcomes'] : [];
      const queries = part['arguments']['queries'];
      (Array.isArray(queries) ? queries : []).forEach((query, index) => {
        const outcome: unknown = outcomes[index];
        if (isRecord(query) && typeof query['path'] === 'string' && isRecord(outcome) && outcome['ok'] === true) preparation.fileOps.edited.add(query['path']);
      });
    }
  }
}
