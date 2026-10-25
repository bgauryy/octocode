import fs from 'node:fs';
import net from 'node:net';
import path from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import { sourceLines } from '../responses.mjs';
import { addedLines } from './mock-model.mjs';

const fallback = /warm server (?:unavailable|failed)/;
const repositoryRows = data => (data.repositories ?? []).map(repository => repository.repo);
const responseRows = (responses, select) => responses.flatMap(response =>
  (response.json?.results ?? []).flatMap(row => select(row.data ?? {})));

function stopSocket(socketPath) {
  return new Promise(resolve => {
    const socket = net.createConnection(socketPath);
    socket.on('connect', () => socket.end('{"stop":true}\n'));
    socket.on('error', resolve);
    socket.on('close', resolve);
    socket.setTimeout(5000, () => { socket.destroy(); resolve(); });
  });
}

async function stopServers(directory) {
  if (!fs.statSync(directory, { throwIfNoEntry: false })?.isDirectory()) return;
  await Promise.all(fs.readdirSync(directory).filter(name => name.endsWith('.sock'))
    .map(name => stopSocket(path.join(directory, name))));
}

function socketScope() {
  const tmp = fs.mkdtempSync('/tmp/octocw-');
  const directory = path.join(tmp, `octocode-${process.getuid()}`);
  const block = async () => {
    await stopServers(directory);
    fs.rmSync(directory, { recursive: true, force: true });
    fs.writeFileSync(directory, 'warm socket directory unavailable');
  };
  const unblock = () => fs.unlinkSync(directory);
  const active = () => fs.statSync(directory, { throwIfNoEntry: false })?.isDirectory()
    && fs.readdirSync(directory).some(name => name.endsWith('.sock'));
  const close = async () => {
    await stopServers(directory);
    fs.rmSync(tmp, { recursive: true, force: true });
  };
  return { tmp, block, unblock, active, close };
}

async function transition(ctx, fixture, direction) {
  const scope = socketScope();
  const responses = [], modes = [];
  const coldFirst = direction === 'coldToWarm';
  const env = { TMPDIR: scope.tmp, PWD: ctx.cwd, OLDPWD: path.dirname(ctx.cwd) };
  let report;
  try {
    if (coldFirst) await scope.block();
    report = await ctx.driver.explore(fixture.tool, fixture.input, {
      surface: 'cli', maxCalls: 1, maxPages: 100, rows: () => fixture.rows,
      call: async (tool, input) => {
        const first = responses.length === 0;
        const cold = first ? coldFirst : !coldFirst;
        const response = await ctx.cliCall(tool, input, { env });
        responses.push(response);
        modes.push(cold ? fallback.test(response.stderr) : !fallback.test(response.stderr) && scope.active());
        if (first) {
          if (coldFirst) scope.unblock();
          else await scope.block();
        }
        return response;
      },
    });
  } finally { await scope.close(); }
  const actual = responseRows(responses, fixture.rows);
  const detail = JSON.stringify({ calls: responses.length, rows: actual.length, expected: fixture.expected.length, failures: report.failures });
  return [
    [`cursor.${direction}.cli.mode`, modes.every(Boolean) && responses.length > 1, detail],
    [`cursor.${direction}.cli.continues`, report.failures.length === 0 && responses.length > 1, detail],
    [`cursor.${direction}.cli.exact`, isDeepStrictEqual(actual, fixture.expected), detail],
  ];
}

async function transitions(ctx, fixture) {
  if (process.platform === 'win32') return [];
  const checks = [];
  for (const direction of ['coldToWarm', 'warmToCold']) {
    checks.push(...await ctx.scenario(direction, 'cli', () => transition(ctx, fixture, direction)));
  }
  return checks;
}

function readerReplay(ctx) {
  const source = ctx.mock.model.repos.find(repository => repository.name === 'demo-docs').files['guide.txt'];
  const expected = addedLines(source).map((line, index) => [index + 1, line]);
  return transitions(ctx, {
    tool: 'ghGetFileContent',
    input: { queries: [{ owner: 'octo', repo: 'demo-docs', path: 'guide.txt' }] },
    rows: sourceLines, expected,
  });
}

function repositoryReplay(ctx) {
  return transitions(ctx, {
    tool: 'ghSearchRepo',
    input: { queries: [{ operation: 'search', keywords: ['demo'] }], responseChars: 150 },
    rows: repositoryRows, expected: ['demo', 'demo-cli', 'demo-docs'],
  });
}

export const ADVANCED_CLI_REPLAY = { ghGetFileContent: readerReplay, ghSearchRepo: repositoryReplay };
