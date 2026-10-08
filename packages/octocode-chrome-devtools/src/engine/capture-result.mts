import { capturePath } from './capture-path.mjs';
import { createReadStream, readFileSync, writeFileSync } from 'node:fs';
import { createInterface } from 'node:readline';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { helpers } from './cli-catalog.mjs';
import { flowPage, readerRoute } from './flow-page.mjs';

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
    findings: string[] = [];
  for await (const line of createInterface({
    input: createReadStream(stdout.file),
    crlfDelay: Infinity,
  })) {
    const match = line.match(/^\[ARTIFACT\] (.+?) ((?:\/|[A-Za-z]:[\\/]).+)$/);
    if (match) {
      const key = match[1] + '\0' + match[2];
      if (!seen.has(key)) {
        seen.add(key);
        artifacts.push({ label: match[1], path: match[2] });
      }
    }
    if (/^\[(FINDING|WARNING|ERROR)\]/.test(line)) findings.push(line);
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
        if (
          data?.next ||
          (Array.isArray(data?.rows) &&
            data.rows.some((row: any) => row.oversized && row.next))
        )
          result.isPartial = true;
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
  result.isPartial = true;
  result.next = { capture: read(file, 'json', 2000) };
  const flow = artifacts.find(row => row.path.endsWith('/browser-result.json'));
  if (flow && command === 'run') {
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
        hidden: true,
        noIgnore: true,
        defaultExcludes: false,
      },
    });
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
  } else result.next.stdout = read(stdout.file);
  if (findings.length)
    result.next.findings = {
      tool: 'query',
      query: {
        args: ['--file', file, '--pointer', '/findings', '--limit', '8'],
      },
    };
  result.logs = logs;
  if (stderr.bytes > 2000) result.next.stderr = read(stderr.file);
  if (artifacts.length) {
    const roots = [...new Set(artifacts.map(row => dirname(row.path)))];
    // Search scope is metadata, not a placeholder executable continuation.
    result.search = {
      paths: roots,
      hidden: true,
      noIgnore: true,
      defaultExcludes: false,
    };
    result.hints = {
      localFetch: { tool: 'localFetch', query: { path: file, length: 60 } },
    };
  }
  if (stderr.bytes && stderr.bytes <= 2000)
    result.error = readFileSync(stderr.file, 'utf8');
  if (Buffer.byteLength(JSON.stringify(result)) > 16000) {
    delete result.error;
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
