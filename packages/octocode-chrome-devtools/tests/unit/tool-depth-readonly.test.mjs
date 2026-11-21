// Annotation honesty: every tool the adapter classifies readOnly really is. Each one runs against a
// real Chrome and leaves the browser's tabs and cookies unchanged and writes nothing outside the
// capture evidence tree of the workspace.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync } from 'node:fs';
import { join, relative } from 'node:path';
import { WebSocket } from 'ws';
import { annotationRules } from '../../dist/runtime.js';
import { skip, withSite } from './tool-depth-fixture.mjs';

const EVIDENCE_TREE = '.octocode/octocode-chrome-devtools/';
const filesUnder = (dir, root = dir) =>
  readdirSync(dir, { withFileTypes: true }).flatMap(entry =>
    entry.isDirectory()
      ? filesUnder(join(dir, entry.name), root)
      : [relative(root, join(dir, entry.name))]
  );
// Page tabs only: Chrome's own helper targets (service workers, UI, blank stubs) come and go.
const tabs = async port =>
  (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json())
    .filter(row => row.type === 'page')
    .map(row => `${row.id} ${row.type} ${row.url}`)
    .sort();
const cookies = async port => {
  const { webSocketDebuggerUrl } = await (
    await fetch(`http://127.0.0.1:${port}/json/version`)
  ).json();
  const socket = new WebSocket(webSocketDebuggerUrl);
  await new Promise(resolve => socket.once('open', resolve));
  const answer = new Promise(resolve =>
    socket.once('message', data => resolve(JSON.parse(String(data))))
  );
  socket.send(JSON.stringify({ id: 1, method: 'Storage.getCookies' }));
  const { result } = await answer;
  socket.close();
  return JSON.stringify(result.cookies);
};

test(
  'every readOnly tool leaves tabs, cookies and the workspace outside the evidence tree untouched',
  { skip, timeout: 200000 },
  async () => {
    await withSite(async ({ cwd, port, connection, client, ok }) => {
      const { tools } = await client.listTools();
      // The CLI-only snapshot and screenshot are readOnly in the table but not exported.
      const readOnly = Object.entries(annotationRules)
        .filter(([, rule]) => rule.kind === 'readOnly')
        .map(([name]) => name)
        .filter(name => tools.some(tool => tool.name === name))
        .sort();
      for (const tool of tools)
        assert.equal(
          tool.annotations?.readOnlyHint,
          readOnly.includes(tool.name),
          `${tool.name} readOnlyHint follows annotationRules`
        );

      // A first snapshot gives query and artifact a real evidence file to read.
      const seed = await ok('check', { connection, recipe: 'page-snapshot' });
      const calls = {
        targets: () => ok('targets', { connection }),
        protocol: () =>
          ok('protocol', { member: 'DOM.querySelector', connection }),
        schema: () => ok('schema', { command: 'run' }),
        skill: () => ok('skill', { topic: 'recovery', length: 500 }),
        query: () =>
          ok('query', {
            file: seed.evidence,
            format: 'json',
            pointer: '/artifacts',
          }),
        artifact: () =>
          ok('artifact', { file: seed.evidence, format: 'json', length: 500 }),
      };
      assert.deepEqual(
        Object.keys(calls).sort(),
        readOnly,
        'a new readOnly tool needs a case here'
      );

      const tabsBefore = await tabs(port);
      const cookiesBefore = await cookies(port);
      assert.ok(
        cookiesBefore.includes('sessname'),
        'the fixture cookie is in the jar'
      );
      const filesBefore = new Set(filesUnder(cwd));
      for (const [name, run] of Object.entries(calls)) {
        await run();
        assert.deepEqual(
          await tabs(port),
          tabsBefore,
          `${name} changed the tab list`
        );
        assert.equal(
          await cookies(port),
          cookiesBefore,
          `${name} changed the cookies`
        );
        const outside = filesUnder(cwd).filter(
          file => !filesBefore.has(file) && !file.startsWith(EVIDENCE_TREE)
        );
        assert.deepEqual(
          outside,
          [],
          `${name} wrote outside the evidence tree`
        );
      }
    });
  }
);
