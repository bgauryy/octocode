import { createRequire } from 'node:module';
import type { Usage } from '@earendil-works/pi-ai';
import { DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, formatSize, truncateHead, type ExtensionContext } from '@earendil-works/pi-coding-agent';
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

export interface TextResult<T = unknown> {
  content: Array<{ type: 'text'; text: string }>;
  details: T;
}

export function textResult<T = undefined>(text: string, details?: T): TextResult<T> {
  return { content: [{ type: 'text', text }], details: details as T };
}

/** `12 of 40 lines (3.0KB of 90KB)`: the counts both output notes state (`ELISION_MARKER` finds them). */
export const trimmedCounts = (shownLines: number, totalLines: number, shownBytes: number, totalBytes: number): string =>
  `${shownLines} of ${totalLines} lines (${formatSize(shownBytes)} of ${formatSize(totalBytes)})`;

/** Keep tool output inside Pi's per-result budget and say when it was cut. */
export function capOutput(text: string, maxBytes = DEFAULT_MAX_BYTES, maxLines = DEFAULT_MAX_LINES): string {
  const cut = truncateHead(text, { maxBytes, maxLines });
  if (!cut.truncated) return cut.content;
  if (cut.content === '' && text !== '') {
    // Pi keeps whole lines only, so a first line over the byte budget would leave nothing: keep its head instead.
    const head = cutText(text, maxBytes);
    return `${head}\n\n[Output trimmed: the first line alone is ${formatSize(Buffer.byteLength(firstLine(text)))}; kept its first ${formatSize(Buffer.byteLength(head))} (${formatSize(cut.totalBytes)} in all).]`;
  }
  return `${cut.content}\n\n[Output trimmed: ${trimmedCounts(cut.outputLines, cut.totalLines, cut.outputBytes, cut.totalBytes)}.]`;
}

/**
 * The one text cut: the head (or, with `tail`, the end) of `text` within `max` UTF-8 bytes, or UTF-16 code units with
 * `chars`. A character is never cut in half: neither a multi-byte UTF-8 sequence nor a surrogate pair.
 */
export function cutText(text: string, max: number, { chars = false, tail = false }: { chars?: boolean; tail?: boolean } = {}): string {
  const limit = Math.max(0, Math.floor(max));
  if (chars) {
    if (text.length <= limit) return text;
    if (tail) {
      let start = text.length - limit;
      if (isLowSurrogate(text.charCodeAt(start))) start++;
      return text.slice(start);
    }
    let end = limit;
    if (end > 0 && isHighSurrogate(text.charCodeAt(end - 1))) end--;
    return text.slice(0, end);
  }
  const bytes = Buffer.from(text, 'utf8');
  if (bytes.length <= limit) return text;
  // UTF-8 continuation bytes are 10xxxxxx: a cut moves off them to the nearest character boundary inside the budget.
  const continuation = (index: number) => (bytes[index]! & 0xc0) === 0x80;
  if (tail) {
    let start = bytes.length - limit;
    while (start < bytes.length && continuation(start)) start++;
    return bytes.subarray(start).toString('utf8');
  }
  let end = limit;
  while (end > 0 && continuation(end)) end--;
  return bytes.subarray(0, end).toString('utf8');
}

const isHighSurrogate = (code: number) => code >= 0xd800 && code <= 0xdbff;
const isLowSurrogate = (code: number) => code >= 0xdc00 && code <= 0xdfff;

/** `text` cut to `max` characters, ending in `…` when cut: the one character clip (`clip` adds one sanitized line). */
export function clipText(text: string, max: number): string {
  if (max <= 0) return '';
  return text.length > max ? `${cutText(text, max - 1, { chars: true })}…` : text;
}

/** Whole seconds in `ms`, rounded: the unit of a job's runtime and of a timeout in messages. */
export const wholeSeconds = (ms: number): number => Math.round(ms / 1000);

/**
 * Waits for `work` to settle, `ms` to pass (no limit when undefined), or `signal` to abort, whichever comes first; never rejects. The timer and
 * the abort listener are removed afterwards, so repeated waits on a long-lived signal leave nothing behind.
 */
export async function settleWithin(work: Promise<unknown>, ms: number | undefined, signal?: AbortSignal): Promise<void> {
  if (signal?.aborted) return;
  let timer: NodeJS.Timeout | undefined;
  let onAbort: (() => void) | undefined;
  try {
    await Promise.race([
      work.catch(() => undefined),
      ...(ms === undefined ? [] : [new Promise<void>((resolve) => void (timer = setTimeout(resolve, ms)))]),
      ...(signal ? [new Promise<void>((resolve) => signal.addEventListener('abort', (onAbort = () => resolve()), { once: true }))] : []),
    ]);
  } finally {
    clearTimeout(timer);
    if (onAbort) signal?.removeEventListener('abort', onAbort);
  }
}

/** `promise`, or a rejection with the signal's reason as soon as `signal` aborts (at once when it already has). */
export async function untilAborted<T>(promise: Promise<T>, signal: AbortSignal | undefined): Promise<T> {
  if (!signal) return promise;
  let onAbort: () => void = () => undefined;
  const aborted = new Promise<never>((_, reject) => {
    onAbort = () => reject(signal.reason);
    if (signal.aborted) onAbort();
    else signal.addEventListener('abort', onAbort, { once: true });
  });
  try {
    return await Promise.race([promise, aborted]);
  } finally {
    signal.removeEventListener('abort', onAbort);
  }
}

export async function withTimeout<T>(work: Promise<T>, ms: number, label: string): Promise<T> {
  let timer: NodeJS.Timeout | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error(`${label} timed out after ${wholeSeconds(ms)}s`)), ms);
  });
  try {
    return await Promise.race([work, timeout]);
  } finally {
    clearTimeout(timer);
  }
}

/**
 * The placeholders Octocode leaves where it cut text the model later sees: `capChars` cuts, the gap and trim note of a
 * result spilled to a file (`capOutput`, `capOutputToFile`), the range note of a file read back after compaction, and
 * the cuts of a kept user message or a tool result in a compaction summary. A model that copies one into a file writes
 * a hole, not content.
 */
const ELISION_MARKER =
  /\[… (?:\d+ (?:more characters cut|characters of this message are left out here|characters in the middle left out for the summary|lines? omitted …)|[\d.]+[KM]?B omitted …|shown lines \d+–\d+ of \d+)[^\]\n]*\]|\[Output trimmed: [^\]\n]*\]/g;

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
  const kept = cutText(text, max, { chars: true });
  return `${kept}\n[… ${text.length - kept.length} more characters cut${hint ? `; ${hint}` : ''}]`;
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
      const text = partText(part);
      if (text !== undefined) return [text];
      return isRecord(part) && part['type'] === 'image' && options.image !== undefined ? [options.image] : [];
    })
    .join(options.separator ?? '\n');
}

/** The text of a `{type:'text',text}` content part, else undefined (an image, a tool call, anything else). */
export function partText(part: unknown): string | undefined {
  return isRecord(part) && part['type'] === 'text' && typeof part['text'] === 'string' ? part['text'] : undefined;
}

/** `value` when it is a string, else an empty string: the untyped field a caller reads as text. */
export const stringOrEmpty = (value: unknown): string => (typeof value === 'string' ? value : '');

/** `value` when it is a string, else undefined: tells a missing or mistyped field from an empty one. */
export const optionalString = (value: unknown): string | undefined => (typeof value === 'string' ? value : undefined);

/** `value` when it is a finite number, else undefined (not 0, so a missing count is not read as zero). */
export const finiteNumber = (value: unknown): number | undefined => (typeof value === 'number' && Number.isFinite(value) ? value : undefined);

/** `value` when it is an array of strings, else undefined. */
export const stringList = (value: unknown): string[] | undefined => (Array.isArray(value) && value.every((item) => typeof item === 'string') ? (value as string[]) : undefined);

/** `text` as a regular-expression source that matches it literally. */
export const escapeRegExp = (text: string): string => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export const MINUTE_MS = 60_000;
export const HOUR_MS = 60 * MINUTE_MS;
export const DAY_MS = 24 * HOUR_MS;

/** A pid that may be signalled: 0, 1 and negative ones name a group, init or every process. */
export const validPid = (pid: unknown): pid is number => typeof pid === 'number' && Number.isInteger(pid) && pid > 1;

/** Pi's wording for an extension context used after its session was replaced or reloaded. */
const STALE_SESSION = 'stale after session replacement';

/** Whether `error` says a session context is stale (Pi gives no error class for it). */
export function isStaleSessionError(error: unknown): boolean {
  return errorMessage(error).includes(STALE_SESSION);
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

const num = (value: unknown) => (typeof value === 'number' && Number.isFinite(value) ? value : 0);

/** Add a model response's usage into `total`. Anything that is not a number counts as zero. */
export function addUsage(total: Usage, usage: unknown): void {
  if (!isRecord(usage)) return;
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

/**
 * Usage as the displays show it (footer, agents panel, `coordinate list`, subagent rows): `input` is every prompt token
 * (uncached input, cache reads and writes), `cacheRead` the part of it read from cache, `cost` the sum of Pi's per-reply
 * estimate, and `unpriced` whether any reply came from a model without pricing (its tokens are missing from `cost`).
 */
export interface UsageTotals {
  input: number;
  cacheRead: number;
  output: number;
  cost: number;
  unpriced: boolean;
}

export const emptyTotals = (): UsageTotals => ({ input: 0, cacheRead: 0, output: 0, cost: 0, unpriced: false });

/** A message that carries spend, shaped as a reply: `usage`, and `details.unpriced` when a subagent run judged its replies. */
export interface Spend {
  role: string;
  usage: Record<string, unknown>;
  details?: unknown;
  provider?: unknown;
  model?: unknown;
}

/**
 * The spend a message carries, or undefined: a model reply's `usage`, a tool result's (a foreground subagent's `agent`
 * result), or a custom message's `details.usage` (a background subagent's report: the run's total, with
 * `details.unpriced`). Every display and the session index count exactly these.
 */
export function spendOf(message: unknown): Spend | undefined {
  if (!isRecord(message) || typeof message['role'] !== 'string') return undefined;
  const role = message['role'];
  if ((role === 'assistant' || role === 'toolResult') && isRecord(message['usage'])) return message as unknown as Spend;
  const details = message['details'];
  if (role === 'custom' && isRecord(details) && isRecord(details['usage'])) return { role, usage: details['usage'], details: { unpriced: details['unpriced'] === true } };
  return undefined;
}

/** The message a session entry adds to the context (a `message` entry's, or a `custom_message` entry as a custom message), else undefined. */
export function entryMessage(entry: unknown): unknown {
  if (!isRecord(entry)) return undefined;
  if (entry['type'] === 'message') return entry['message'];
  if (entry['type'] === 'custom_message') return { role: 'custom', customType: entry['customType'], details: entry['details'] };
  return undefined;
}

/** Adds one reply's usage (anything that is not a number counts as zero); `unpriced` from `unpricedModel`. */
export function addReply(totals: UsageTotals, usage: unknown, unpriced: boolean): void {
  if (!isRecord(usage)) return;
  totals.input += num(usage['input']) + num(usage['cacheRead']) + num(usage['cacheWrite']);
  totals.cacheRead += num(usage['cacheRead']);
  totals.output += num(usage['output']);
  totals.cost += num(isRecord(usage['cost']) ? usage['cost']['total'] : undefined);
  if (unpriced) totals.unpriced = true;
}

/** A model's registry rates by provider and id (`ctx.modelRegistry.find`); undefined when the registry does not know it. */
export type ModelLookup = (provider: string, id: string) => { cost?: { input: number; output: number; cacheRead: number; cacheWrite: number } } | undefined;

export const modelLookup = (ctx: Pick<ExtensionContext, 'modelRegistry'>): ModelLookup => (provider, id) => ctx.modelRegistry?.find(provider, id);

/**
 * The `provider/model` of a reply (an assistant message, or a tool result carrying a subagent's usage) that spent tokens
 * on a model without pricing, else undefined. Pi gives a model with no `cost` in models.json all-zero rates, so its usage
 * silently costs $0: free local models, or a model nobody priced. A model the registry does not know (or a reply that does
 * not name one) is judged by the reply's own cost.
 */
export function unpricedModel(message: unknown, find?: ModelLookup): string | undefined {
  if (!isRecord(message) || !isRecord(message['usage'])) return undefined;
  const usage = message['usage'];
  if (num(usage['input']) + num(usage['output']) + num(usage['cacheRead']) + num(usage['cacheWrite']) === 0) return undefined;
  // A subagent's `agent` result names no model; its run already judged every reply (partly priced is still unpriced).
  const details = isRecord(message['details']) ? message['details'] : undefined;
  if (typeof details?.['unpriced'] === 'boolean' && typeof message['model'] !== 'string') {
    return details['unpriced'] ? (typeof details['model'] === 'string' ? details['model'] : 'a subagent model') : undefined;
  }
  const provider = typeof message['provider'] === 'string' ? message['provider'] : undefined;
  const model = typeof message['model'] === 'string' ? message['model'] : undefined;
  const rates = provider && model ? find?.(provider, model)?.cost : undefined;
  const free = rates ? rates.input === 0 && rates.output === 0 && rates.cacheRead === 0 && rates.cacheWrite === 0 : num(isRecord(usage['cost']) ? usage['cost']['total'] : undefined) === 0;
  if (!free) return undefined;
  return model ? (provider ? `${provider}/${model}` : model) : 'unknown model';
}
