import fs from 'node:fs';
import path from 'node:path';
import {
  createEditToolDefinition,
  createWriteToolDefinition,
  withFileMutationQueue,
  type ExtensionAPI,
  type ExtensionToolContext,
} from '@earendil-works/pi-coding-agent';
import { Type, type Static } from 'typebox';
import { atomicWriteFile, changedOnDisk, sha256, sha256File } from '../shared/atomic.js';
import { canonicalPath, isWithin, readPaths, resolveToolPath, workspaceRoot } from '../shared/home.js';
import type { Permission } from '../permission/ask.js';
import { systemLines } from '../permission/effect.js';
import { runsLater, stateFolderRefusal } from '../permission/gate.js';
import { lineCount, readView } from './coverage.js';
import { lastUserEntry, type Capture, type Checkpoints } from './checkpoint.js';
import { judgeChange, lostAnchor, type FileGuard, type KnownContent, type WrittenContent } from './guard.js';
import { notify } from '../shared/commands.js';
import { askReview } from './review.js';
import { timedTool } from '../shared/tool-timing.js';
import { renderFileCall, renderFileResult, type QueryOutcome } from './render.js';
import { contentText, elisionMarkers, errorMessage, escapeRegExp, isRecord } from '../shared/util.js';

/**
 * Octocode's `file` tool: batched edit / write / delete queries, each with a
 * reasoning line. Edits and writes run through Pi's own engines (exact-match
 * replacement, diffs, per-file mutation queue) but write atomically (temp file + rename) and only over the bytes
 * they checked. A file read with Pi's `read`, or changed by this
 * tool, that changed on disk since then must be read again before a write or delete; an edit re-validates each oldText
 * against the current content and applies when each still matches exactly once. A file whose earlier read left the
 * context (compaction, a branch switch, a rewind that restored it) must be read again before
 * it is changed, even if the disk did not move.
 *
 * Paths are checked against the workspace first: Octocode's own state folder (the Octocode home, holding trust
 * decisions, memory and sessions) is never changed, and a path outside the repository is a `file.outside` permission
 * ask. In `manual` mode the batch's review dialog is its `file.workspace` ask. What the tool knows about each read is
 * the `FileGuard` (guard.ts).
 */

const warnedCheckpoints = new WeakSet<Checkpoints>();

/**
 * Records a change in the checkpoint without failing the change itself. A failure means /octocode rewind would miss this file,
 * so it is noted in the outcome and notified once per session.
 */
async function settleCheckpoint(ctx: ExtensionToolContext, checkpoints: Checkpoints | undefined, capture: Capture | undefined, file: string, ok: boolean, after?: string | null): Promise<string> {
  if (!checkpoints || !capture) return '';
  try {
    await checkpoints.settle(file, capture, ok, after);
    return '';
  } catch (error) {
    if (!warnedCheckpoints.has(checkpoints)) {
      warnedCheckpoints.add(checkpoints);
      if (ctx.ui) notify(ctx.ui, `Octocode could not save a checkpoint (${errorMessage(error)}); /octocode rewind may not restore changed files.`, 'warning');
    }
    return ' (checkpoint not saved: /octocode rewind will not restore this change)';
  }
}

const QuerySchema = Type.Object({
  reasoning: Type.String({ description: 'Why this change is needed, in one sentence' }),
  // A plain string enum (what pi-ai's StringEnum emits) keeps the schema portable across providers.
  type: Type.Unsafe<'edit' | 'write' | 'delete'>({
    type: 'string',
    enum: ['edit', 'write', 'delete'],
    description: 'edit: exact replacements in an existing file; write: create or fully replace a file; delete: remove a file (irreversible; only when the task requires it)',
  }),
  path: Type.String({ description: 'File path, relative to the working directory or absolute' }),
  edits: Type.Optional(
    Type.Array(
      Type.Object({
        oldText: Type.String({ description: 'Exact text to replace; unique in the original file; keep it small' }),
        newText: Type.String({ description: 'Replacement text' }),
      }),
      { description: 'For edit: all replacements for this file, matched against the original content; must not overlap' },
    ),
  ),
  content: Type.Optional(Type.String({ description: 'For write: the complete new file content' })),
});

const Params = Type.Object({ queries: Type.Array(QuerySchema) });
type FileQuery = Static<typeof QuerySchema>;


/**
 * Filesystem operations for Pi's edit and write engines: writes are atomic and succeed only if the file still holds
 * what was checked (`checked`: the digest of `initial`, the bytes already read for this change; null = still absent).
 * Pi's first read of that file is served from `initial` instead of the disk.
 */
function guardedOperations(display: string, checked: string | null, initial?: { file: string; data: Buffer }, reserved?: () => void) {
  let read: string | undefined;
  let written: WrittenContent | undefined;
  let cached = initial;
  return {
    /** The digest and size of what the last write put on disk. */
    written: () => written,
    readFile: async (file: string) => {
      if (cached && cached.file === file && checked !== null) {
        const { data } = cached;
        cached = undefined;
        read = checked;
        return data;
      }
      const data = await fs.promises.readFile(file);
      read = sha256(data);
      return data;
    },
    writeFile: async (file: string, content: string) => {
      reserved?.();
      try {
        await atomicWriteFile(file, content, read ?? checked);
        const bytes = Buffer.from(content);
        written = { sha256: sha256(bytes), size: bytes.length, lines: lineCount(bytes) };
      } catch (error) {
        throw /changed on disk/.test(errorMessage(error)) ? changedOnDisk(display) : error;
      }
    },
    access: (file: string) => fs.promises.access(file, fs.constants.R_OK | fs.constants.W_OK),
    mkdir: async (dir: string) => {
      await fs.promises.mkdir(dir, { recursive: true });
    },
  };
}

async function runQuery(
  query: FileQuery, index: number, id: string, guard: FileGuard, ctx: ExtensionToolContext,
  signal: AbortSignal | undefined, permission: Permission, checkpoints?: Checkpoints,
): Promise<QueryOutcome> {
  const base = { type: query.type, path: query.path, reasoning: query.reasoning };
  const file = resolveToolPath(ctx.cwd, query.path);
  let capture: Capture | undefined;
  let operations: ReturnType<typeof guardedOperations> | undefined;
  let before: Buffer | undefined;
  try {
    if (query.type === 'edit' && !query.edits?.length) throw new Error('edit needs a non-empty edits array.');
    if (query.type === 'write' && query.content === undefined) throw new Error('write needs content.');
    const outside = await workspaceRefusal(file, query, ctx, signal, permission, id);
    if (outside) throw new Error(outside);
    const inspection = guard.inspect(file, query.type, query.path);
    if (inspection.refusal) throw new Error(inspection.refusal);
    const { current } = inspection;
    // Checked again right before the write: a peer may reserve the file while the change is prepared (asks, reads, checkpoint).
    const reserved = () => {
      const now = guard.reservedBy?.(file);
      if (now) throw new Error(now);
    };
    // The one read before the change: it serves the content check, the checkpoint and Pi's edit engine. The atomic
    // write's compare-and-swap is the only other read.
    const data = current
      ? await fs.promises.readFile(file).catch(() => {
          throw changedOnDisk(query.path);
        })
      : undefined;
    before = data;
    const read = data ? { data, sha256: sha256(data) } : undefined;
    const checked = read ? read.sha256 : null;
    // A write or delete over changed bytes is refused. An edit goes on when every anchor still matches once.
    const { changed, refusal: unfit } = judgeChange(query.path, query.type, inspection, read);
    if (unfit) throw new Error(unfit);
    const lost = changed && data ? lostAnchor(query.path, query.edits ?? [], data.toString('utf8')) : undefined;
    if (lost) throw new Error(lost);
    const copied = copiedElision(query, data?.toString('utf8') ?? '');
    if (copied) {
      throw new Error(
        `The new text contains "${copied}": a placeholder Octocode puts where it cut a tool's output, not file content. Re-read the source and write the full text.`,
      );
    }
    capture = await checkpoints?.capture(file, () => lastUserEntry(ctx), read ? { ...read, mode: current?.mode ?? 0o644 } : null);
    const note = capture?.note ? ` (${capture.note})` : '';
    if (query.type === 'delete') {
      await withFileMutationQueue(file, async () => {
        signal?.throwIfAborted();
        if (((await sha256File(file)) ?? null) !== checked) throw changedOnDisk(query.path);
        signal?.throwIfAborted();
        reserved();
        await fs.promises.rm(file);
      });
      guard.record(file);
      const unsaved = await settleCheckpoint(ctx, checkpoints, capture, file, true, null);
      return { ...base, ok: true, message: `Deleted ${query.path}${note}${unsaved}` };
    }
    operations = guardedOperations(query.path, checked, data ? { file, data } : undefined, reserved);
    const tool = query.type === 'edit' ? createEditToolDefinition(ctx.cwd, { operations }) : createWriteToolDefinition(ctx.cwd, { operations });
    const args = query.type === 'edit' ? { path: query.path, edits: query.edits } : { path: query.path, content: query.content };
    const result = await tool.execute(`${id}:${index}`, args as never, signal, undefined, ctx);
    const written = operations.written();
    // An edit over outside changes leaves them unseen: later edits go on, a write or delete needs a fresh read.
    const lineDelta = recordChange(guard, file, query.type, before, written);
    if (changed) guard.stale(file);
    const unsaved = await settleCheckpoint(ctx, checkpoints, capture, file, true, written?.sha256);
    const message = (contentText(result.content).trim() || `${query.type} ${query.path}`) + note + unsaved;
    const diff = isRecord(result.details) && typeof result.details['diff'] === 'string' ? result.details['diff'] : undefined;
    return { ...base, ok: true, message, ...(diff ? { diff } : {}), ...(lineDelta !== undefined ? { lineDelta } : {}) };
  } catch (error) {
    // Pi can observe cancellation after writeFile succeeded: retain both the read guard and the undo checkpoint.
    const written = operations?.written();
    if (written) recordChange(guard, file, query.type, before, written);
    const note = await settleCheckpoint(ctx, checkpoints, capture, file, written !== undefined, written?.sha256);
    return { ...base, ok: false, message: `${errorMessage(error)}${written ? ' The file was changed; inspect it before retrying.' : ''}${note}` };
  }
}

/**
 * Records what this tool wrote. A write puts known content on disk, so the agent knows the whole file; an edit only
 * knows the digest, and the agent keeps the coverage its reads had (`FileGuard.edited`). Returns an edit's change in
 * line count, for the outcome (the session rebuild applies the same rule from it).
 */
function recordChange(guard: FileGuard, file: string, type: FileQuery['type'], before: Buffer | undefined, written: WrittenContent | undefined): number | undefined {
  if (type !== 'edit') {
    guard.record(file, written);
    return undefined;
  }
  const lineDelta = written && before ? written.lines - lineCount(before) : undefined;
  guard.edited(file, written, lineDelta);
  return lineDelta;
}

/**
 * Why `file` may not be changed for its place: inside Octocode's state folder (always refused, in every mode), or
 * outside the repository and the `file.outside` ask was not allowed. Undefined when the change may go ahead.
 */
async function workspaceRefusal(file: string, query: FileQuery, ctx: ExtensionToolContext, signal: AbortSignal | undefined, permission: Permission, toolCallId: string): Promise<string | undefined> {
  const real = canonicalPath(file);
  const home = stateFolderRefusal(real, query.path, 'the file tool');
  if (home) {
    permission.refused([`${query.type} ${real}`], home, 'file');
    return home;
  }
  const root = workspaceRoot(ctx.cwd);
  const inside = root !== undefined && isWithin(real, root);
  if (inside && !runsLater(real, root)) return undefined;
  const dir = path.dirname(real);
  const verdict = await permission.check(ctx, {
    action: 'file.outside',
    title: inside ? 'Change a file that runs in later sessions?' : 'Change a file outside the workspace?',
    what: [`${query.type} ${real}`, `workspace: ${root ?? 'none (the home folder is not a workspace)'}`],
    purpose: query.reasoning,
    toolCallId,
    cwd: ctx.cwd,
    grant: { key: dir, label: `file changes under ${dir}` },
    ...(signal ? { signal } : {}),
  });
  return verdict.allowed ? undefined : `${query.path} is ${inside ? 'a file that runs in later sessions' : root ? `outside the workspace (${root})` : 'outside any workspace (the home folder is not one)'}. ${verdict.message}`;
}

/** The batch's `purpose`: what its changes' `reasoning` fields say, in order; none when they say nothing (the assistant's message then stands in). */
function changePurpose(queries: readonly Pick<FileQuery, 'reasoning'>[]): { purpose?: string } {
  const said = [...new Set(queries.map((query) => query.reasoning?.trim()).filter(Boolean))];
  return said.length > 0 ? { purpose: said.join('; ') } : {};
}

/** The first output-cut placeholder a change would add to the file (one already in the file or the replaced text is fine). */
export function copiedElision(query: Pick<FileQuery, 'type' | 'edits' | 'content'>, original: string): string | undefined {
  const added = query.type === 'write' ? [{ oldText: '', newText: query.content ?? '' }] : query.type === 'edit' ? (query.edits ?? []) : [];
  for (const { oldText, newText } of added) {
    const found = elisionMarkers(newText).find((marker) => !oldText.includes(marker) && !original.includes(marker));
    if (found) return found;
  }
  return undefined;
}

/** Pi's messages end with "in <path>." or "to <path>"; the outcome line already names the path. */
function withoutPath(message: string, file: string): string {
  return message.replace(new RegExp(`\\s+(?:in|to) ${escapeRegExp(file)}\\.?$`), '').replace(/^Successfully /, '');
}

export function formatOutcomes(outcomes: QueryOutcome[]): string {
  return outcomes
    .map((outcome, index) => `${index + 1}. ${outcome.ok ? 'OK' : 'FAILED'} ${outcome.type} ${outcome.path}: ${outcome.ok ? withoutPath(outcome.message, outcome.path) : outcome.message}`)
    .join('\n');
}

/**
 * The digest of what Pi's read returned when that is the whole file verbatim (no offset, limit or truncation), so the
 * guard checks the change against what the model saw rather than what the disk holds a moment later.
 */
export function returnedContent(toolName: string, input: Record<string, unknown>, content: unknown, details: unknown): KnownContent | undefined {
  if (readView(toolName, input, content, details)?.whole !== true) return undefined;
  if (!Array.isArray(content) || content.length !== 1 || !isRecord(content[0]) || content[0]['type'] !== 'text') return undefined;
  const text = content[0]['text'];
  return typeof text === 'string' ? { sha256: sha256(text), size: Buffer.byteLength(text) } : undefined;
}

export function registerFileTool(pi: ExtensionAPI, guard: FileGuard, permission: Permission, checkpoints?: Checkpoints): void {
  pi.registerTool(timedTool({
    name: 'file',
    label: 'File',
    description:
      'Create, edit or delete files in this repository; other paths need approval in manual and workspace permission modes. Queries run in order and report individual outcomes; a failure does not roll back earlier changes. Put replacements for one file in one query. ' +
      'Read an existing file with `read` first: an unread file, or a write or delete over a changed one, is refused; an edit applies if each oldText still matches once. Inspect every outcome.',
    promptSnippet: 'Create, edit or delete files (guarded)',
    promptGuidelines: [
      'Use file for authored changes; batch related files when useful. Run formatters, generators and builds with bash, then inspect their changes.',
    ],
    parameters: Params,
    // `file` calls run one at a time (the guard, review dialog and checkpoints are shared); other tools batched beside
    // them still run in parallel. Each path's writes also queue behind Pi's edit/write through withFileMutationQueue.
    execute: async (id: string, params: Static<typeof Params>, signal: AbortSignal | undefined, _onUpdate: unknown, ctx: ExtensionToolContext) => {
      if (params.queries.length === 0) throw new Error('No queries: pass at least one edit, write or delete query.');
      const outcomes: QueryOutcome[] = [];
      // When the mode asks (manual), the review is the ask: nothing is applied until the user has said yes to each
      // change (or to all of them). Without a UI to ask in, the batch is refused.
      let rejected = new Map<number, string | undefined>();
      const verdict = await permission.check(
        ctx,
        { action: 'file.workspace', title: 'Apply these file changes?', what: params.queries.map((query) => `${query.type} ${query.path}`), ...changePurpose(params.queries), toolCallId: id, cwd: ctx.cwd, ...(signal ? { signal } : {}) },
        async (dialogCtx, request) => {
          const answer = await askReview(dialogCtx, params.queries, request.signal, systemLines(request, permission.mode));
          if (answer === 'cancelled') return undefined;
          if (answer === 'session') return 'kind';
          rejected = answer;
          if (answer.size !== params.queries.length) return 'once';
          const notes = [...answer].flatMap(([index, note]) => note ? [`${params.queries[index]!.path}: ${note}`] : []);
          return notes.length > 0 ? { deny: notes.join('\n') } : 'deny';
        },
      );
      // A dismissed or interrupted review is not a verdict on the changes.
      if ((!verdict.allowed && verdict.via === 'cancelled') || signal?.aborted) throw new Error('Cancelled before any change was applied.');
      if (!verdict.allowed) throw new Error(verdict.message);
      for (const [index, query] of params.queries.entries()) {
        const note = rejected.get(index);
        const skip = rejected.has(index) ? `Rejected by the user; do not retry this change.${note ? ` User's note: ${note}` : ''}` : signal?.aborted ? 'Not applied: the batch was aborted.' : undefined;
        const skipped = skip ? { type: query.type, path: query.path, reasoning: query.reasoning, ok: false, message: skip } : undefined;
        outcomes.push(skipped ?? (await runQuery(query, index, id, guard, ctx, signal, permission, checkpoints)));
      }
      if (outcomes.every((outcome) => !outcome.ok)) throw new Error(formatOutcomes(outcomes));
      return { content: [{ type: 'text' as const, text: formatOutcomes(outcomes) }], details: { outcomes } };
    },
    renderCall: renderFileCall,
    renderResult: renderFileResult,
  }, { exclusive: true }));

  pi.on('tool_result', async (event, ctx) => {
    if (event.isError) return;
    const known = returnedContent(event.toolName, event.input, event.content, event.details);
    const view = readView(event.toolName, event.input, event.content, event.details);
    for (const file of readPaths(event.toolName, event.input, ctx.cwd)) guard.record(file, known, view?.whole !== true, view?.lines);
  });
}
