/**
 * Console log: CONSOLE_URL, CONSOLE_MS (required), CONSOLE_LEVELS (JSON array log|info|warning|error|debug).
 * Listens CONSOLE_MS (the page loads after the listeners attach) and writes console.jsonl, one event per line:
 * {ts, level, source, text, url, line, column, stack?}. Stdout carries counts and the first lines.
 */
import type { Payload, Session } from '../cdp-connection.mjs';
import { errorMessage } from '../error-message.mjs';
import { attachFrameSessions } from './evidence-frames.mjs';
import { MAX_STDOUT_ITEMS, out, saveEvidence } from './evidence-io.mjs';
import {
  countOption,
  sleep,
  stringsOption,
  textOption,
} from './stream-options.mjs';

const LEVELS = ['log', 'info', 'warning', 'error', 'debug'] as const;
type Level = (typeof LEVELS)[number];

type Frame = { url: string; line: number; column: number; function?: string };
type ConsoleEvent = {
  ts: string;
  level: Level;
  source: string;
  type?: string;
  text: string;
  url: string;
  line: number | null;
  column: number | null;
  stack?: Frame[];
};

const API_LEVEL: Record<string, Level> = {
  log: 'log',
  info: 'info',
  warning: 'warning',
  warn: 'warning',
  error: 'error',
  assert: 'error',
  debug: 'debug',
};
const LOG_LEVEL: Record<string, Level> = {
  verbose: 'debug',
  info: 'info',
  warning: 'warning',
  error: 'error',
};

/** One console argument as text: a primitive's value, or an object's description with its preview. */
function showArg(arg: Payload): string {
  if (arg.value !== undefined)
    return typeof arg.value === 'string'
      ? arg.value
      : JSON.stringify(arg.value);
  if (arg.unserializableValue) return String(arg.unserializableValue);
  if (arg.type === 'undefined') return 'undefined';
  const head = String(
    arg.description ?? arg.className ?? arg.subtype ?? arg.type
  );
  const props = arg.preview?.properties as Payload[] | undefined;
  if (!props?.length) return head;
  const body = props.map(p => `${p.name}: ${p.value}`).join(', ');
  return `${head} {${body}${arg.preview?.overflow ? ', ...' : ''}}`;
}

const frames = (trace?: Payload): Frame[] =>
  (trace?.callFrames ?? []).map((f: Payload) => ({
    url: f.url,
    line: f.lineNumber + 1,
    column: f.columnNumber + 1,
    ...(f.functionName ? { function: f.functionName } : {}),
  }));

const stamp = (ms: unknown) =>
  new Date(typeof ms === 'number' && ms > 0 ? ms : Date.now()).toISOString();

export async function run(cdp: Session) {
  const ms = countOption('CONSOLE_MS');
  if (!ms)
    throw new Error('CONSOLE_MS is required and must be a positive integer');
  const url = textOption('CONSOLE_URL');
  const wanted = stringsOption('CONSOLE_LEVELS');
  for (const level of wanted ?? [])
    if (!(LEVELS as readonly string[]).includes(level))
      throw new Error(
        `CONSOLE_LEVELS: ${level} is not one of ${LEVELS.join(', ')}`
      );
  const events: ConsoleEvent[] = [];
  const keep = (event: ConsoleEvent) => {
    if (!wanted || wanted.includes(event.level)) events.push(event);
  };

  cdp.on('Runtime.consoleAPICalled', (p: Payload) => {
    const stack = frames(p.stackTrace);
    keep({
      ts: stamp(p.timestamp),
      level: API_LEVEL[p.type] ?? 'log',
      source: 'console-api',
      type: p.type,
      text: (p.args ?? []).map(showArg).join(' '),
      url: stack[0]?.url ?? '',
      line: stack[0]?.line ?? null,
      column: stack[0]?.column ?? null,
      ...(stack.length ? { stack } : {}),
    });
  });
  cdp.on('Runtime.exceptionThrown', (p: Payload) => {
    const d = p.exceptionDetails as Payload;
    const stack = frames(d.stackTrace);
    keep({
      ts: stamp(p.timestamp),
      level: 'error',
      source: 'exception',
      text: String(d.exception?.description ?? d.exception?.value ?? d.text),
      url: d.url ?? stack[0]?.url ?? '',
      line: typeof d.lineNumber === 'number' ? d.lineNumber + 1 : null,
      column: typeof d.columnNumber === 'number' ? d.columnNumber + 1 : null,
      ...(stack.length ? { stack } : {}),
    });
  });
  cdp.on('Log.entryAdded', (p: Payload) => {
    const e = p.entry as Payload;
    const stack = frames(e.stackTrace);
    keep({
      ts: stamp(e.timestamp),
      level: LOG_LEVEL[e.level] ?? 'log',
      source: String(e.source),
      text: String(e.text),
      url: e.url ?? '',
      line: typeof e.lineNumber === 'number' ? e.lineNumber + 1 : null,
      column: null,
      ...(stack.length ? { stack } : {}),
    });
  });

  const domains = ['Runtime', 'Log'];
  const attached = await attachFrameSessions(cdp, domains);
  for (const domain of domains) await cdp.send(`${domain}.enable`);
  await cdp.send('Page.enable');
  if (url) {
    const nav: Payload = await cdp.send('Page.navigate', { url });
    if (nav.errorText) {
      out(
        `[FINDING] CONSOLE_NAVIGATION_FAILED ${JSON.stringify({ url, error: nav.errorText })}`
      );
      process.exitCode = 1;
    }
  }
  await sleep(ms);
  await attached.stop();

  const body = events.map(e => JSON.stringify(e)).join('\n');
  const saved = saveEvidence(
    cdp.outputDir,
    'console.jsonl',
    body ? `${body}\n` : ''
  );
  const counts = Object.fromEntries(
    LEVELS.map(l => [l, events.filter(e => e.level === l).length])
  );
  out(
    `[METRIC] CONSOLE events=${events.length} ${LEVELS.map(l => `${l}=${counts[l]}`).join(' ')} ms=${ms}`
  );
  for (const e of events.slice(0, MAX_STDOUT_ITEMS))
    out(
      `[CONSOLE] ${e.level} ${e.source} ${e.url}:${e.line ?? 0} ${JSON.stringify(e.text.slice(0, 200))}${e.text.length > 200 ? ` (first 200 of ${e.text.length} chars; the full text is in console.jsonl)` : ''}`
    );
  if (events.length > MAX_STDOUT_ITEMS)
    out(
      `[CONSOLE] ${MAX_STDOUT_ITEMS} of ${events.length} events shown; every event is in ${saved.file}`
    );
  for (const error of attached.errors)
    out(`[FINDING] CONSOLE_FRAME_COVERAGE_UNAVAILABLE ${errorMessage(error)}`);
  if (!events.length)
    out(
      `[FINDING] CONSOLE_NONE no console event in ${ms}ms${wanted ? ` for levels ${wanted.join(',')}` : ''}; pass CONSOLE_URL to capture a fresh load or a longer CONSOLE_MS`
    );
  out(`[ARTIFACT] CONSOLE ${cdp.outputDir}/${saved.file}`);
}
