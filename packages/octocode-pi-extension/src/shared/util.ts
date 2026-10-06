import { createRequire } from 'node:module';
import type { Usage } from '@earendil-works/pi-ai';
import { DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, formatSize, truncateHead } from '@earendil-works/pi-coding-agent';
import { redactSecrets } from './sanitize.js';

/** node:sqlite prints an ExperimentalWarning on some Node versions; that would land in the terminal UI. */
export function loadSqlite(): typeof import('node:sqlite') {
  const emit = process.emitWarning;
  process.emitWarning = ((warning: string | Error, ...rest: unknown[]) => {
    const text = typeof warning === 'string' ? warning : warning.message;
    if (/sqlite/i.test(text)) return;
    return (emit as (...args: unknown[]) => void).call(process, warning, ...rest);
  }) as typeof process.emitWarning;
  try {
    return createRequire(import.meta.url)('node:sqlite') as typeof import('node:sqlite');
  } finally {
    process.emitWarning = emit;
  }
}

interface TextResult<T = unknown> {
  content: Array<{ type: 'text'; text: string }>;
  details: T;
}

export function textResult<T = undefined>(text: string, details?: T): TextResult<T> {
  return { content: [{ type: 'text', text }], details: details as T };
}

/** Keep tool output inside Pi's per-result budget and say when it was cut. */
export function capOutput(text: string, maxBytes = DEFAULT_MAX_BYTES, maxLines = DEFAULT_MAX_LINES): string {
  const cut = truncateHead(text, { maxBytes, maxLines });
  if (!cut.truncated) return cut.content;
  if (cut.content === '' && text !== '') {
    // Pi keeps whole lines only, so a first line over the byte budget would leave nothing: keep its head instead.
    const head = utf8Head(text, maxBytes);
    return `${head}\n\n[Output truncated: the first line alone is ${formatSize(Buffer.byteLength(firstLine(text)))}; kept its first ${formatSize(Buffer.byteLength(head))} (${formatSize(cut.totalBytes)} in all).]`;
  }
  return `${cut.content}\n\n[Output truncated: ${cut.outputLines} of ${cut.totalLines} lines (${formatSize(cut.outputBytes)} of ${formatSize(cut.totalBytes)}).]`;
}

/** The first `maxBytes` bytes of `text`, without a character cut in half. */
export function utf8Head(text: string, maxBytes: number): string {
  return Buffer.from(text, 'utf8').subarray(0, maxBytes).toString('utf8').replace(/\uFFFD+$/, '');
}

/** `text` cut to `max` characters, ending in `…` when cut: the one character clip (`clip` adds one sanitized line). */
export function clipText(text: string, max: number): string {
  if (max <= 0) return '';
  return text.length > max ? `${text.slice(0, max - 1)}…` : text;
}

/**
 * Waits for `work` to settle, `ms` to pass, or `signal` to abort, whichever comes first; never rejects. The timer and
 * the abort listener are removed afterwards, so repeated waits on a long-lived signal leave nothing behind.
 */
export async function settleWithin(work: Promise<unknown>, ms: number, signal?: AbortSignal): Promise<void> {
  if (signal?.aborted) return;
  let timer: NodeJS.Timeout | undefined;
  let onAbort: (() => void) | undefined;
  try {
    await Promise.race([
      work.catch(() => undefined),
      new Promise<void>((resolve) => void (timer = setTimeout(resolve, ms))),
      ...(signal ? [new Promise<void>((resolve) => signal.addEventListener('abort', (onAbort = () => resolve()), { once: true }))] : []),
    ]);
  } finally {
    clearTimeout(timer);
    if (onAbort) signal?.removeEventListener('abort', onAbort);
  }
}

export async function withTimeout<T>(work: Promise<T>, ms: number, label: string): Promise<T> {
  let timer: NodeJS.Timeout | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error(`${label} timed out after ${Math.round(ms / 1000)}s`)), ms);
  });
  try {
    return await Promise.race([work, timeout]);
  } finally {
    clearTimeout(timer);
  }
}

/**
 * The placeholders Octocode leaves where it shortened text the model later sees again: trimmed call arguments and
 * tool results (compaction) and `capChars` cuts. A model that copies one into a file writes a hole, not content.
 */
const ELISION_MARKER = /\[(?:… ?)?\d+ more characters (?:of this earlier, successful call omitted|from this earlier tool result were trimmed[^\]\n]*|cut[^\]\n]*)\]/g;

export function elisionMarkers(text: string): string[] {
  return text.match(ELISION_MARKER) ?? [];
}

/** `text` parsed as JSON, or undefined when it is not JSON. */
export function parseJson(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return undefined;
  }
}

/** `text` cut to `max` characters with a note saying how much was cut (and, optionally, what to do instead). */
export function capChars(text: string, max: number, hint?: string): string {
  if (text.length <= max) return text;
  return `${text.slice(0, max)}\n[… ${text.length - max} more characters cut${hint ? `; ${hint}` : ''}]`;
}

/**
 * The text of a message or tool result's content (a string, or an array of `{type:'text',text}` parts), joined by
 * `separator`. Image parts become `image` when given, else nothing.
 */
export function contentText(content: unknown, options: { separator?: string; image?: string } = {}): string {
  if (typeof content === 'string') return content;
  if (!Array.isArray(content)) return '';
  return content
    .flatMap((part): string[] => {
      if (!isRecord(part)) return [];
      if (part['type'] === 'text' && typeof part['text'] === 'string') return [part['text']];
      return part['type'] === 'image' && options.image !== undefined ? [options.image] : [];
    })
    .join(options.separator ?? '\n');
}

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/**
 * The most telling argument of a tool call (path, command, query, url or element ref), one clipped line. It is drawn in
 * the working line and stored as the agent's activity, which other sessions see: typed text is never used, URL
 * credentials are dropped and anything that looks like a secret is redacted.
 */
export function toolHint(args: unknown): string {
  const input = isRecord(args) ? args : {};
  const hint = [input['path'], input['command'], input['query'], input['url'], input['ref']].find((value) => typeof value === 'string') as string | undefined;
  if (!hint) return '';
  const line = firstLine(hint).replace(/\b([a-z][a-z0-9+.-]*:\/\/)[^\s/@]*@/gi, '$1');
  return clipText(redactSecrets(line), 80);
}

/** The first line of a text (all of it when it has one line). */
export const firstLine = (text: string): string => text.split('\n')[0] ?? '';

export function emptyUsage(): Usage {
  return { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, totalTokens: 0, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } };
}

/** Add a model response's usage into `total`. Anything that is not a number counts as zero. */
export function addUsage(total: Usage, usage: unknown): void {
  if (!isRecord(usage)) return;
  const num = (value: unknown) => (typeof value === 'number' && Number.isFinite(value) ? value : 0);
  total.input += num(usage['input']);
  total.output += num(usage['output']);
  total.cacheRead += num(usage['cacheRead']);
  total.cacheWrite += num(usage['cacheWrite']);
  total.totalTokens += num(usage['totalTokens']);
  const cost = isRecord(usage['cost']) ? usage['cost'] : {};
  total.cost.input += num(cost['input']);
  total.cost.output += num(cost['output']);
  total.cost.cacheRead += num(cost['cacheRead']);
  total.cost.cacheWrite += num(cost['cacheWrite']);
  total.cost.total += num(cost['total']);
}

/** A new usage record holding the sum of two. */
export function sumUsage(a: Usage, b: Usage): Usage {
  const total = emptyUsage();
  addUsage(total, a);
  addUsage(total, b);
  return total;
}
