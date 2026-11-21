import { capturePath } from './capture-path.mjs';
import {
  createReadStream,
  existsSync,
  readFileSync,
  realpathSync,
  writeFileSync,
} from 'node:fs';
import { createInterface } from 'node:readline';
import { dirname, isAbsolute, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { helpers } from './cli-catalog.mjs';
import { flowPage, readerRoute } from './flow-page.mjs';

// A snapshot page at most this large rides in the reply; larger pages stay behind `next`.
export const INLINE_SNAPSHOT_BYTES = 4000;

// Plain output with no artifacts at most this large rides in the reply whole (the built-in `flow list` fits).
export const INLINE_STDOUT_BYTES = 3000;

export type ExitStatus = {
  exitCode: number;
  signal?: NodeJS.Signals | null;
  timedOut?: boolean;
  cancelled?: boolean;
  outcomeUncertain?: boolean;
};
export type CapturedOutput = { file: string; bytes: number };
// Evidence shaping belongs to the engine package, shared by its CLI/MCP adapter.
export const readNext = (
  file: string,
  format = 'text',
  length = 1000,
  workspace = process.cwd()
) => ({
  tool: 'artifact',
  query: { file: capturePath(file, workspace), format, length },
});
const readerTools = new Map(
  Object.entries(helpers).map(([name, [file]]) => [
    file.split('/').at(-1),
    name,
  ])
);
function route(next: any, workspace: string) {
  const call = next?.continue;
  if (!call || call.command !== process.execPath) return next;
  const file = call.args?.[0],
    tool = file && readerTools.get(file.split(/[\\/]/).at(-1));
  if (!tool) return next;
  return ['query', 'artifact'].includes(tool)
    ? readerRoute(call.args.slice(1), tool, workspace)
    : { tool, query: { args: call.args.slice(1) } };
}
function readerPage(data: any, command: string, workspace: string) {
  if (!data || typeof data !== 'object') return data;
  if (data.next) data.next = route(data.next, workspace);
  for (const row of Array.isArray(data.rows) ? data.rows : [])
    if (row.oversized && row.next) row.next = route(row.next, workspace);
  if (command === 'query') {
    delete data.index;
    delete data.indexReused;
  }
  return data;
}
// Success replies carry at most this many warning lines; the rest sit behind next.findings.
export const REPLY_WARNINGS = 5;
// Success replies list at most this many artifact files; the rest sit behind next.artifacts.
export const REPLY_FILES = 8;
// A check reply carries its recipe's [METRIC]/[FINDING]/[WARNING] lines: at most this many lines and bytes;
// the rest stays in the stdout file behind next.stdout.
export const REPLY_SUMMARY_LINES = 12;
export const REPLY_SUMMARY_BYTES = 1500;
/**
 * The few [NOTE] lines that carry a next step the agent must see: they ride in the reply `warnings`
 * (a connection-bound override, a domain without getters, the protocol index hint).
 */
const REPLY_NOTE =
  /^\[NOTE\] (?:\S+ applies only to this connection|\S+ has no parameterless getters|use member:'\*')/;
type Artifact = { label: string; path: string };
/**
 * The path of an [ARTIFACT] line. A recipe may append prose (`<path> (note)`): when the whole text is
 * no file but a prefix before a ` (` is, that prefix is the path and the note stays out of `files`.
 */
export function artifactPath(text: string) {
  if (existsSync(text)) return text;
  for (let at = text.indexOf(' ('); at > 0; at = text.indexOf(' (', at + 1)) {
    const prefix = text.slice(0, at);
    if (existsSync(prefix)) return prefix;
  }
  return text;
}
/** A path relative to the workspace (the agent's cwd); absolute only when it lies outside it. */
export function workspacePath(file: string, workspace: string) {
  let path: string;
  try {
    path = capturePath(file, workspace);
  } catch {
    path = relative(workspace, file) || '.';
  }
  return path.startsWith('..') || isAbsolute(path) ? file : path;
}
/** Copies a reply value with every string that starts inside the workspace made workspace-relative. */
function relativeValues(value: any, roots: string[]): any {
  if (typeof value === 'string') {
    for (const root of roots) {
      if (value === root) return '.';
      if (value.startsWith(`${root}/`)) return value.slice(root.length + 1);
    }
    return value;
  }
  if (!value || typeof value !== 'object' || JSON.isRawJSON(value))
    return value;
  if (Array.isArray(value))
    return value.map(item => relativeValues(item, roots));
  return Object.fromEntries(
    Object.entries(value).map(([key, item]) => [
      key,
      relativeValues(item, roots),
    ])
  );
}
// The sandbox banner, Node warnings and the runner's tab/port line open every engine stderr; they are not diagnostics.
const BANNER =
  /sandboxed \(node |ExperimentalWarning|--trace-warnings|^\[CDP_SANDBOX\]|^\[CDP_RUNNER\] Using /;
/** The stderr file when the call wrote a line beyond the banner, else nothing. */
const diagnostics = (stderr: CapturedOutput) =>
  stderr.bytes &&
  readFileSync(stderr.file, 'utf8')
    .split('\n')
    .some(line => line.trim() && !BANNER.test(line.trim()))
    ? stderr.file
    : undefined;
// Plan bookkeeping (plan, step stream, result) stays behind `evidence` and `flow`.
const isBookkeeping = (row: Artifact) =>
  /(^|[\\/])browser-(result\.json|plan\.json|steps\.jsonl)$/.test(row.path);
/**
 * The compact success reply: `ok` and the useful content flattened (`data`, `snapshot`, `flow`,
 * `stdout` when it is the answer), then `files` (artifacts to read next), `evidence` (the saved
 * capture manifest), `warnings`, `stderr` (path) and `next` (only continuations that read more).
 */
function successReply(
  parts: {
    data?: unknown;
    snapshot?: string;
    summary?: { lines: string[]; truncated: boolean };
    flow?: unknown;
    stdout?: string;
    artifacts: Artifact[];
    findings: string[];
    manifest?: string;
    stderr?: string;
    next?: Record<string, any>;
  },
  workspace: string
) {
  const rel = (file: string) => workspacePath(file, workspace);
  let real = workspace;
  try {
    real = realpathSync(workspace);
  } catch {
    // A workspace that cannot be resolved keeps its given path as the only root.
  }
  const roots = [...new Set([real, workspace])];
  const reply: Record<string, any> = { ok: true };
  if (parts.data !== undefined) reply.data = relativeValues(parts.data, roots);
  if (parts.snapshot) reply.snapshot = parts.snapshot;
  if (parts.summary?.lines.length) {
    reply.summary = parts.summary.lines;
    if (parts.summary.truncated) reply.summaryTruncated = true;
  }
  if (parts.flow) reply.flow = relativeValues(parts.flow, roots);
  const next: Record<string, any> = { ...parts.next };
  const listed: Artifact[] = [];
  let at = 0;
  for (; at < parts.artifacts.length && listed.length < REPLY_FILES; at++) {
    const row = parts.artifacts[at];
    if (isBookkeeping(row)) continue;
    const entry = { label: row.label, path: rel(row.path) };
    if (Buffer.byteLength(JSON.stringify([...listed, entry])) > 4000) break;
    listed.push(entry);
  }
  // Always plain paths: the file name says what it is, and one shape is one parser.
  if (listed.length) reply.files = listed.map(row => row.path);
  if (parts.manifest && at < parts.artifacts.length)
    next.artifacts = {
      tool: 'query',
      query: {
        file: rel(parts.manifest),
        pointer: '/artifacts',
        cursor: at,
        limit: 8,
      },
    };
  if (parts.manifest && (listed.length || parts.flow || parts.findings.length))
    reply.evidence = rel(parts.manifest);
  // A finding the summary already carries verbatim is not repeated as a warning.
  const warned = parts.findings.filter(
    line => !parts.summary?.lines.includes(line)
  );
  if (warned.length) {
    reply.warnings = warned.slice(0, REPLY_WARNINGS);
    if (parts.manifest && warned.length > REPLY_WARNINGS)
      next.findings = {
        tool: 'query',
        query: {
          file: rel(parts.manifest),
          pointer: '/findings',
          cursor: REPLY_WARNINGS,
          limit: 8,
        },
      };
  }
  if (
    parts.stdout !== undefined &&
    parts.data === undefined &&
    !parts.snapshot &&
    !listed.length
  )
    reply.stdout = parts.stdout;
  if (parts.stderr) reply.stderr = rel(parts.stderr);
  const seen = new Set<string>();
  for (const [key, call] of Object.entries(next)) {
    const text = JSON.stringify(call);
    if (seen.has(text)) delete next[key];
    else seen.add(text);
  }
  if (Object.keys(next).length) reply.next = next;
  return reply;
}
/** The first lines of a check's [METRIC]/[FINDING]/[WARNING] output within the reply ceilings; whole lines only. */
export function replySummary(all: string[]) {
  const lines: string[] = [];
  let bytes = 0;
  for (const line of all) {
    bytes += Buffer.byteLength(line);
    if (lines.length >= REPLY_SUMMARY_LINES || bytes > REPLY_SUMMARY_BYTES)
      break;
    lines.push(line);
  }
  return { lines, truncated: lines.length < all.length };
}
export async function captureResult(
  status: ExitStatus,
  outputs: Record<'stdout' | 'stderr', CapturedOutput>,
  directory: string,
  command: string,
  engineDirectory = dirname(fileURLToPath(import.meta.url)),
  workspace = process.cwd()
) {
  const read = (file: string, format = 'text', length = 1000) =>
    readNext(file, format, length, workspace);
  const result: ExitStatus & { ok: boolean; [key: string]: any } = {
    ok: status.exitCode === 0,
    ...status,
  };
  const stdout = outputs.stdout,
    stderr = outputs.stderr;
  const artifacts: { label: string; path: string }[] = [],
    seen = new Set<string>(),
    findings: string[] = [],
    pageLines: string[] = [],
    metricLines: string[] = [];
  for await (const line of createInterface({
    input: createReadStream(stdout.file),
    crlfDelay: Infinity,
  })) {
    const match = line.match(/^\[ARTIFACT\] (.+?) ((?:\/|[A-Za-z]:[\\/]).+)$/);
    if (match) {
      const path = artifactPath(match[2]);
      const key = `${match[1]}\0${path}`;
      if (!seen.has(key)) {
        seen.add(key);
        artifacts.push({ label: match[1], path });
      }
    }
    if (/^\[(FINDING|WARNING|ERROR)\]/.test(line) || REPLY_NOTE.test(line))
      findings.push(line);
    if (/^\[(METRIC|FINDING|WARNING)\]/.test(line) || REPLY_NOTE.test(line))
      metricLines.push(line);
    // A plan may snapshot more than once: the last page is the one the agent acts on.
    if (line.startsWith('[PAGE] "')) pageLines.length = 0;
    if (/^\[(PAGE|SNAPSHOT)\]|^\[METRIC\] SNAPSHOT /.test(line))
      pageLines.push(line);
  }
  // JSON-returning commands/readers carry their complete page directly, avoiding
  // an escaped JSON string nested in the transport envelope.
  if (!artifacts.length && stdout.bytes <= 16000) {
    const text = readFileSync(stdout.file, 'utf8');
    let data;
    try {
      data = JSON.parse(text, (_key, value, context) =>
        typeof value === 'number' && context?.source
          ? JSON.rawJSON(context.source)
          : value
      );
    } catch (error) {
      if (!(error instanceof SyntaxError)) throw error;
    }
    if (data !== undefined) {
      data = readerPage(data, command, workspace);
      if (Buffer.byteLength(JSON.stringify({ ...result, data })) <= 16000) {
        result.data = data;
        // A success keeps benign diagnostics in a stderr file; a failure's stderr is its reason.
        if (result.ok)
          return successReply(
            {
              data,
              artifacts,
              findings,
              stderr: diagnostics(stderr),
            },
            workspace
          );
        if (!stderr.bytes) return result;
      }
    }
  }
  const logs = {
    stdout: stdout.file,
    ...(stderr.bytes ? { stderr: stderr.file } : {}),
  };
  const manifest = { ...result, artifacts, findings, logs };
  const file = join(directory, 'capture.json');
  writeFileSync(file, JSON.stringify(manifest, null, 2), {
    mode: 0o600,
    flag: 'wx',
  });
  result.capture = {
    file,
    artifacts: artifacts.length,
    findings: findings.length,
  };
  result.next = { capture: read(file, 'json', 2000) };
  // A complete, small snapshot page is the answer itself: inline its refs so acting on a
  // control costs no further read. A paged or large one keeps the continuation below.
  const snapshot = pageLines.join('\n');
  const inlineSnapshot =
    (command === 'snapshot' || command === 'run') &&
    /refs=\d+ showing=\S+ page=(\d+)\/\1 /.test(snapshot) &&
    Buffer.byteLength(snapshot) <= INLINE_SNAPSHOT_BYTES
      ? snapshot
      : undefined;
  const flow = artifacts.find(row => row.path.endsWith('/browser-result.json'));
  // Every plan result (run, step, cdp, protocol Domain.member) carries its flow page.
  if (flow) {
    const page = await flowPage(
      flow.path,
      read,
      join(engineDirectory, 'evidence-query.mjs'),
      workspace
    );
    Object.assign(result, {
      flow: page.flow,
      search: {
        paths: [
          ...new Set(
            artifacts.map(row => capturePath(dirname(row.path), workspace))
          ),
        ],
        includeHidden: true,
        includeIgnored: true,
        defaultExcludes: false,
      },
    });
    if (result.ok)
      return successReply(
        {
          flow: page.flow,
          snapshot: inlineSnapshot,
          artifacts,
          findings,
          manifest: file,
          stderr: diagnostics(stderr),
          next: page.next,
        },
        workspace
      );
    Object.assign(result.next, page.next);
    if (findings.length)
      result.next.findings = {
        tool: 'query',
        query: {
          args: ['--file', file, '--pointer', '/findings', '--limit', '8'],
        },
      };
    // stderr is preserved in logs and the full manifest; avoid repeating its path.
    return result;
  }
  if (inlineSnapshot) result.snapshot = inlineSnapshot;
  if (result.ok) {
    const small = !artifacts.length && stdout.bytes <= INLINE_STDOUT_BYTES;
    const summary = command === 'check' ? replySummary(metricLines) : undefined;
    return successReply(
      {
        snapshot: result.snapshot,
        summary,
        stdout: small ? readFileSync(stdout.file, 'utf8') : undefined,
        artifacts,
        findings,
        manifest: file,
        stderr: diagnostics(stderr),
        next:
          (!artifacts.length && !small) || summary?.truncated
            ? { stdout: read(stdout.file) }
            : undefined,
      },
      workspace
    );
  }
  if (artifacts.length) {
    result.root = dirname(artifacts[0].path);
    result.artifacts = [];
    for (const row of artifacts.slice(0, 8)) {
      const entry = { label: row.label, path: relative(result.root, row.path) };
      if (
        Buffer.byteLength(JSON.stringify([...result.artifacts, entry])) > 4000
      )
        break;
      result.artifacts.push(entry);
    }
    const cursor = result.artifacts.length;
    if (cursor < artifacts.length)
      result.next.artifacts = {
        tool: 'query',
        query: {
          args: [
            '--file',
            file,
            '--pointer',
            '/artifacts',
            '--cursor',
            String(cursor),
            '--limit',
            '8',
          ],
        },
      };
  } else if (stdout.bytes <= INLINE_STDOUT_BYTES)
    // A short whole output (a script's printed result) is the answer: no read call needed.
    result.stdout = readFileSync(stdout.file, 'utf8');
  else result.next.stdout = read(stdout.file);
  if (findings.length)
    result.next.findings = {
      tool: 'query',
      query: {
        args: ['--file', file, '--pointer', '/findings', '--limit', '8'],
      },
    };
  // The recipe's own findings (with their codes) name the cause when stderr only has a banner.
  if (findings.length) result.summary = replySummary(findings).lines;
  result.logs = logs;
  if (stderr.bytes > 2000) result.next.stderr = read(stderr.file);
  if (artifacts.length) {
    const roots = [...new Set(artifacts.map(row => dirname(row.path)))];
    // Search scope is metadata, not a placeholder executable continuation.
    result.search = {
      paths: roots,
      includeHidden: true,
      includeIgnored: true,
      defaultExcludes: false,
    };
    result.hints = {
      localFetch: { tool: 'localFetch', query: { path: file, length: 60 } },
    };
  }
  // A failure's stderr is its reason; a success's stderr is diagnostics kept in logs.
  if (!result.ok && stderr.bytes && stderr.bytes <= 2000)
    result.error = readFileSync(stderr.file, 'utf8');
  if (Buffer.byteLength(JSON.stringify(result)) > 16000) {
    delete result.data;
    delete result.hints;
    delete result.search;
    delete result.root;
    delete result.artifacts;
    result.next.artifacts = {
      tool: 'query',
      query: {
        args: ['--file', file, '--pointer', '/artifacts', '--limit', '8'],
      },
    };
  }
  return result;
}
