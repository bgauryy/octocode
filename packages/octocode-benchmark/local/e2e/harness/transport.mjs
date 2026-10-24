// Fresh CLI processes and one stdio MCP server share the published contract.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { hermeticEnv } from '../../../../../test-utils/hermetic-env.ts';
import { checks, spawnCli, startServer } from './mcp-client.mjs';
import { nextHints, sourceLines } from './responses.mjs';

const { check, summary } = checks('transport');
const workspace = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-transport-'));
const isolated = hermeticEnv(process.env, 'transport');
const env = { ...isolated, OCTOCODE_STORAGE_MODE: 'persistent', OCTOCODE_ENABLE_LOCAL: 'true', WORKSPACE_ROOT: workspace, ALLOWED_PATHS: workspace };
const expected = Array.from({ length: 1500 }, (_, i) => `line ${i + 1}: immutable 界😀 evidence`);
fs.writeFileSync(path.join(workspace, 'source.txt'), expected.join('\n') + '\n');
let client;
const cli = async (tool, input) => {
  const run = await spawnCli([tool, JSON.stringify(input), '--json'], { cwd: workspace, env });
  assert.ok(run.stdout.trim(), JSON.stringify({ status: run.status, stderr: run.stderr }));
  return { code: run.status, output: JSON.parse(run.stdout), text: run.stdout + run.stderr };
};
function captureSource(output, actual) {
  for (const row of output.results) {
    assert.notEqual(row.status, 'error', JSON.stringify(output));
    if (row.data.content === undefined) continue;
    for (const [line, text] of sourceLines(row.data)) {
      assert.ok(!actual.has(line), `line ${line} emitted twice`);
      actual.set(line, text);
    }
  }
}
const invalid = [
  { tool: 'localSearch', input: { queries: [{ path: '.', matchString: 'line' }] } },
  { tool: 'localFetch', input: { queries: [{ path: 'source.txt', ranges: ['1-3'] }] } },
  { tool: 'localFetch', input: { queries: [{ path: 'source.txt', fullContent: true }] } },
  { tool: 'astSearch', input: { queries: [{ path: '.', operation: 'match', pattern: 'run()' }] } },
];
try {
  client = await startServer({ env, replaceEnv: true, cwd: workspace });
  for (const { tool, input } of invalid) {
    const shell = await cli(tool, input);
    const mcp = await client.raw(tool, input);
    check(`${tool} rejects removed or malformed inputs on both surfaces`, shell.code === 2 && mcp.isError, JSON.stringify({ shell: shell.output, mcp: mcp.sc }));
  }
  const initial = { queries: [{ path: 'source.txt' }], responseChars: 900 };
  for (const surface of ['cli', 'mcp']) {
    const execute = async input => {
      if (surface === 'cli') {
        const run = await cli('localFetch', input);
        assert.equal(run.code, 0, run.text);
        return run.output;
      }
      const run = await client.raw('localFetch', input);
      assert.ok(!run.isError && !run.rowErrors, run.text);
      return run.sc;
    };
    const pending = [await execute(initial)];
    const seen = new Set();
    const actual = new Map();
    while (pending.length) {
      assert.ok(seen.size < 200, 'cursor walk terminates');
      const output = pending.shift();
      captureSource(output, actual);
      for (const call of nextHints(output).filter(edge => edge.kind === 'continue' && edge.tool === 'localFetch')) {
        assert.deepEqual(Object.keys(call.input), ['cursor']);
        assert.ok(!seen.has(call.input.cursor), 'cursor emitted twice');
        seen.add(call.input.cursor);
        pending.push(await execute(call.input));
      }
    }
    assert.deepEqual([...actual].toSorted((a, b) => a[0] - b[0]), expected.map((text, i) => [i + 1, text]));
    check(`${surface} opaque cursors preserve every Unicode source line exactly once`, seen.size > 1, `${actual.size} lines, ${seen.size} replays`);
    const cursor = [...seen][0];
    const foreign = surface === 'cli' ? await client.raw('localFetch', { cursor }) : await cli('localFetch', { cursor });
    assert.ok(foreign.text.includes('responseSnapshotExpired'), foreign.text);
    check(`${surface} cursor is bound to its issuing surface`, true);
    for (const input of [{ cursor, queries: [{ path: 'source.txt' }] }, { cursor, responseChars: 4000 }]) {
      const shell = await cli('localFetch', input);
      const mcp = await client.raw('localFetch', input);
      check('resume rejects caller-supplied request mutations', shell.code === 2 && mcp.isError);
    }
  }
} finally {
  client?.close();
  fs.rmSync(workspace, { recursive: true, force: true });
  fs.rmSync(isolated.OCTOCODE_HOME, { recursive: true, force: true });
}
process.exitCode = summary().failed.length ? 1 : 0;
