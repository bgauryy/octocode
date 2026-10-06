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
import { resolveToolPath } from '../shared/home.js';
import { lastUserEntry, type Capture, type Checkpoints } from './checkpoint.js';
import { reviewChanges, type ReviewMode } from './review.js';
import { exclusive } from '../shared/locks.js';
import { timed } from '../shared/render.js';
import { renderFileCall, renderFileResult, type QueryOutcome } from './render.js';
import { contentText, elisionMarkers, errorMessage, isRecord } from '../shared/util.js';

/**
 * Octocode's `file` tool: batched edit / write / delete queries, each with a
 * reasoning line. Edits and writes run through Pi's own engines (exact-match
 * replacement, diffs, per-file mutation queue) but write atomically (temp file + rename) and only over the bytes
 * they checked. A file read with Pi's `read` (or whole with Octocode MCP's `localGetFileContent`), or changed by this
 * tool, that changed on disk since then must be read again before it is changed. A file whose earlier read left the
 * context (compaction, a branch switch, a rewind that restored it, its read result trimmed) must be read again before
 * it is changed, even if the disk did not move.
 *
 * A read is recorded by stat (size, mtime, ctime, inode) of the canonical path, without reading the file again; its
 * digest is known only when the read returned the whole file (Pi's read) or the file tool wrote it, and is then
 * checked against the bytes the change reads anyway. Residual race: a file changed between the read tool reading it
 * and the stat here is taken as read (unless its digest was known from the returned text).
 */

interface Snapshot {
  mtimeMs: number;
  ctimeMs: number;
  size: number;
  ino: number;
  mode: number;
  sha256?: string;
}

/** Content known without reading the file again (e.g. what was just written). */
interface KnownContent {
  sha256: string;
  size: number;
}

interface Inspection {
  refusal?: string;
  /** The file's stat now; undefined when it does not exist. */
  current?: Snapshot;
  /** The digest the file's bytes must still have (known from the read or the last write), if any. */
  expected?: string;
}

function staleRefusal(display: string): string {
  return `${display} changed on disk since you last read it. Read it again with \`read\` before modifying it.`;
}

function rereadRefusal(display: string): string {
  return `${display} was read before the context was rewound or compacted (or its read result was trimmed), so its content is no longer in view. Read it again with \`read\` before modifying it.`;
}

/** The real path, so a symlink or case alias of a read file counts as read; a missing file's parent is resolved. */
export function canonicalPath(file: string): string {
  try {
    return fs.realpathSync.native(file);
  } catch {
    try {
      return path.join(fs.realpathSync.native(path.dirname(file)), path.basename(file));
    } catch {
      return path.resolve(file);
    }
  }
}

export class FileGuard {
  readonly #seen = new Map<string, Snapshot>();
  /** Files whose read left the context: changing them needs a fresh read first. */
  readonly #reread = new Set<string>();
  /** Set by the team layer: why another agent's reservation forbids changing `file`, if it does. */
  reservedBy: ((file: string) => string | undefined) | undefined;

  /** Remember the file as read now (stat only). `known`: its content digest when the caller has it (kept if the size agrees). */
  record(file: string, known?: KnownContent): void {
    const real = canonicalPath(file);
    const stat = statOf(real);
    this.#reread.delete(real);
    if (!stat) this.#seen.delete(real);
    else this.#seen.set(real, known && known.size === stat.size ? { ...stat, sha256: known.sha256 } : stat);
  }

  /** Forget one file, e.g. after /octocode rewind replaced it: the model must read it again before writing it. */
  forget(file: string): void {
    const real = canonicalPath(file);
    this.#seen.delete(real);
    this.#reread.add(real);
  }

  /** Forget every read, e.g. after compaction or a branch switch drops file contents from context: each must be read again. */
  reset(): void {
    for (const real of this.#seen.keys()) this.#reread.add(real);
    this.#seen.clear();
  }

  /** Returns an error message when the mutation must be refused, else undefined. */
  check(file: string, mode: 'edit' | 'write' | 'delete', display: string): string | undefined {
    return this.inspect(file, mode, display).refusal;
  }

  /**
   * Like check, by stat alone (no read). Also returns the current stat and the digest the file must still have; the
   * caller verifies that against the bytes it reads for the change.
   */
  inspect(file: string, mode: 'edit' | 'write' | 'delete', display: string): Inspection {
    const reserved = this.reservedBy?.(file);
    if (reserved) return { refusal: reserved };
    const real = canonicalPath(file);
    const current = statOf(real);
    if (!current) return mode === 'edit' || mode === 'delete' ? { refusal: `${display} does not exist.` } : {};
    if (this.#reread.has(real)) return { refusal: rereadRefusal(display) };
    const seen = this.#seen.get(real);
    if (!seen) return { current };
    // ctime and inode also move on a same-size rewrite that restored the mtime, or a rename over the file. With a
    // known digest a same-size move is left to the content check (a touch or a no-op rewrite is not a change).
    const moved = seen.mtimeMs !== current.mtimeMs || seen.ctimeMs !== current.ctimeMs || seen.size !== current.size || seen.ino !== current.ino;
    if (moved && (!seen.sha256 || seen.size !== current.size)) return { refusal: staleRefusal(display) };
    return { current, ...(seen.sha256 ? { expected: seen.sha256 } : {}) };
  }
}

function statOf(file: string): Snapshot | undefined {
  try {
    const stat = fs.statSync(file);
    return stat.isFile() ? { mtimeMs: stat.mtimeMs, ctimeMs: stat.ctimeMs, size: stat.size, ino: stat.ino, mode: stat.mode } : undefined;
  } catch {
    return undefined;
  }
}

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
      ctx.ui?.notify?.(`Octocode could not save a checkpoint (${errorMessage(error)}); /octocode rewind may not restore changed files.`, 'warning');
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
      { minItems: 1, description: 'For edit: all replacements for this file, matched against the original content; must not overlap' },
    ),
  ),
  content: Type.Optional(Type.String({ description: 'For write: the complete new file content' })),
});

const Params = Type.Object({ queries: Type.Array(QuerySchema, { minItems: 1, maxItems: 10 }) });
type FileQuery = Static<typeof QuerySchema>;


/**
 * Filesystem operations for Pi's edit and write engines: writes are atomic and succeed only if the file still holds
 * what was checked (`checked`: the digest of `initial`, the bytes already read for this change; null = still absent).
 * Pi's first read of that file is served from `initial` instead of the disk.
 */
function guardedOperations(display: string, checked: string | null, initial?: { file: string; data: Buffer }) {
  let read: string | undefined;
  let written: KnownContent | undefined;
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
      try {
        await atomicWriteFile(file, content, read ?? checked);
        written = { sha256: sha256(content), size: Buffer.byteLength(content) };
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
  query: FileQuery,
  index: number,
  id: string,
  guard: FileGuard,
  ctx: ExtensionToolContext,
  signal: AbortSignal | undefined,
  checkpoints?: Checkpoints,
): Promise<QueryOutcome> {
  const base = { type: query.type, path: query.path, reasoning: query.reasoning };
  const file = resolveToolPath(ctx.cwd, query.path);
  let capture: Capture | undefined;
  try {
    if (query.type === 'edit' && !query.edits?.length) throw new Error('edit needs a non-empty edits array.');
    if (query.type === 'write' && query.content === undefined) throw new Error('write needs content.');
    const { refusal, current, expected } = guard.inspect(file, query.type, query.path);
    if (refusal) throw new Error(refusal);
    // The one read before the change: it serves the content check, the checkpoint and Pi's edit engine. The atomic
    // write's compare-and-swap is the only other read.
    const data = current
      ? await fs.promises.readFile(file).catch(() => {
          throw changedOnDisk(query.path);
        })
      : undefined;
    const checked = data ? sha256(data) : null;
    if (expected !== undefined && expected !== checked) throw new Error(staleRefusal(query.path));
    const copied = copiedElision(query, data?.toString('utf8') ?? '');
    if (copied) {
      throw new Error(
        `The new text contains "${copied}": a placeholder Octocode puts where it shortened an earlier call or result in your context, not file content. Re-read the source and write the full text.`,
      );
    }
    capture = await checkpoints?.capture(file, () => lastUserEntry(ctx), data && checked ? { data, sha256: checked, mode: current?.mode ?? 0o644 } : null);
    const note = capture?.note ? ` (${capture.note})` : '';
    if (query.type === 'delete') {
      await withFileMutationQueue(file, async () => {
        if ((sha256File(file) ?? null) !== checked) throw changedOnDisk(query.path);
        await fs.promises.rm(file);
      });
      guard.record(file);
      const unsaved = await settleCheckpoint(ctx, checkpoints, capture, file, true, null);
      return { ...base, ok: true, message: `Deleted ${query.path}${note}${unsaved}` };
    }
    const operations = guardedOperations(query.path, checked, data ? { file, data } : undefined);
    const tool = query.type === 'edit' ? createEditToolDefinition(ctx.cwd, { operations }) : createWriteToolDefinition(ctx.cwd, { operations });
    const args = query.type === 'edit' ? { path: query.path, edits: query.edits } : { path: query.path, content: query.content };
    const result = await tool.execute(`${id}:${index}`, args as never, signal, undefined, ctx);
    const written = operations.written();
    guard.record(file, written);
    const unsaved = await settleCheckpoint(ctx, checkpoints, capture, file, true, written?.sha256);
    const message = (contentText(result.content).trim() || `${query.type} ${query.path}`) + note + unsaved;
    const diff = isRecord(result.details) && typeof result.details['diff'] === 'string' ? result.details['diff'] : undefined;
    return { ...base, ok: true, message, ...(diff ? { diff } : {}) };
  } catch (error) {
    await settleCheckpoint(ctx, checkpoints, capture, file, false);
    return { ...base, ok: false, message: errorMessage(error) };
  }
}

/** The first context-shortening placeholder a change would add to the file (one already in the file or the replaced text is fine). */
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
  const escaped = file.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  return message.replace(new RegExp(`\\s+(?:in|to) ${escaped}\\.?$`), '').replace(/^Successfully /, '');
}

export function formatOutcomes(outcomes: QueryOutcome[]): string {
  return outcomes
    .map((outcome, index) => `${index + 1}. ${outcome.ok ? 'OK' : 'FAILED'} ${outcome.type} ${outcome.path}: ${outcome.ok ? withoutPath(outcome.message, outcome.path) : outcome.message}`)
    .join('\n');
}

/** Octocode MCP's local file reader: its `fullContent` queries return a whole file. */
export const MCP_READ_TOOL = 'mcp__octocode__localGetFileContent';

/**
 * Whether an MCP `localGetFileContent` query returned its whole file unminified: `fullContent` without a character
 * window or minification (a range, match or default view shows only part of it, or a compacted form).
 */
function wholeFileQuery(query: unknown): query is { path: string } {
  if (!isRecord(query) || typeof query['path'] !== 'string' || query['fullContent'] !== true) return false;
  if (query['charOffset'] !== undefined || query['charLength'] !== undefined) return false;
  return query['minify'] === undefined || query['minify'] === 'none';
}

/**
 * Paths the model read whole enough to change: Pi's `read` (any range: Pi's own guard semantics), and the whole-file
 * queries of Octocode MCP's `localGetFileContent`. The guard keeps their stat to compare against later.
 */
export function readPaths(toolName: string, input: Record<string, unknown>, cwd: string): string[] {
  if (toolName === 'read') return typeof input['path'] === 'string' ? [resolveToolPath(cwd, input['path'])] : [];
  if (toolName !== MCP_READ_TOOL || !Array.isArray(input['queries'])) return [];
  return input['queries'].filter(wholeFileQuery).map((query) => resolveToolPath(cwd, query.path));
}

/**
 * The digest of what Pi's read returned when that is the whole file verbatim (no offset, limit or truncation), so the
 * guard checks the change against what the model saw rather than what the disk holds a moment later.
 */
export function returnedContent(toolName: string, input: Record<string, unknown>, content: unknown, details: unknown): KnownContent | undefined {
  if (toolName !== 'read' || input['offset'] !== undefined || input['limit'] !== undefined) return undefined;
  if (isRecord(details) && details['truncation'] !== undefined) return undefined;
  if (!Array.isArray(content) || content.length !== 1 || !isRecord(content[0]) || content[0]['type'] !== 'text') return undefined;
  const text = content[0]['text'];
  return typeof text === 'string' ? { sha256: sha256(text), size: Buffer.byteLength(text) } : undefined;
}

export function registerFileTool(pi: ExtensionAPI, guard: FileGuard, review?: ReviewMode, checkpoints?: Checkpoints): void {
  pi.registerTool({
    name: 'file',
    label: 'File',
    description:
      'Create, edit or delete workspace files. Queries run in order and report individual outcomes; a failure does not roll back earlier changes. Put replacements for one file in one query. ' +
      'Read existing files with `read` (or Octocode MCP `localGetFileContent` with `fullContent`) before changing them. If a change is refused because the file changed or left the context, read it again with `read` (only `read` or a `fullContent` read clears the refusal), then retry. Inspect every outcome before continuing.',
    promptSnippet: 'Apply guarded file changes and inspect each outcome',
    promptGuidelines: [
      'Use file for authored changes; batch related files when useful. Run formatters, generators and builds with bash, then inspect their changes.',
    ],
    parameters: Params,
    // `file` calls run one at a time (the guard, review dialog and checkpoints are shared); other tools batched beside
    // them still run in parallel. Each path's writes also queue behind Pi's edit/write through withFileMutationQueue.
    execute: exclusive(timed(async (id: string, params: Static<typeof Params>, signal: AbortSignal | undefined, _onUpdate: unknown, ctx: ExtensionToolContext) => {
      const outcomes: QueryOutcome[] = [];
      // With review on, nothing is applied until the user has said yes to each change (or to all of them).
      const rejected = review?.on && ctx.hasUI ? await reviewChanges(ctx, params.queries, signal) : new Set<number>();
      // A dismissed or interrupted review is not a verdict on the changes.
      if (rejected === 'cancelled' || signal?.aborted) throw new Error('Cancelled before any change was applied.');
      if (rejected.size === params.queries.length) throw new Error('The user rejected these changes. Do not retry them; ask what they want changed instead.');
      for (const [index, query] of params.queries.entries()) {
        const skip = rejected.has(index) ? 'Rejected by the user; do not retry this change.' : signal?.aborted ? 'Not applied: the batch was aborted.' : undefined;
        outcomes.push(skip ? { type: query.type, path: query.path, reasoning: query.reasoning, ok: false, message: skip } : await runQuery(query, index, id, guard, ctx, signal, checkpoints));
      }
      if (outcomes.every((outcome) => !outcome.ok)) throw new Error(formatOutcomes(outcomes));
      return { content: [{ type: 'text' as const, text: formatOutcomes(outcomes) }], details: { outcomes } as { outcomes: QueryOutcome[]; durationMs?: number } };
    })),
    renderCall: renderFileCall,
    renderResult: renderFileResult,
  });

  pi.on('tool_result', async (event, ctx) => {
    if (event.isError) return;
    const known = returnedContent(event.toolName, event.input, event.content, event.details);
    for (const file of readPaths(event.toolName, event.input, ctx.cwd)) guard.record(file, known);
  });
}
