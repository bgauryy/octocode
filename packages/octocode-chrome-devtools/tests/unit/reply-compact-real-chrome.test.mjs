// Success replies are compact AND still lead to every piece of evidence. Each tool answers once in
// a temporary workspace (real headless Chrome where the tool needs a browser). Size is measured as
// JSON chars / 4 (a stable token estimate), against the ceilings below, never against copied output.
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';
import { freePort } from '../fixtures/free-port.mjs';
import { connectStdio } from '../../dist/runtime.js';
import { har, harEntry } from './helper-fixture.mjs';
import {
  INLINE_SNAPSHOT_BYTES,
  REPLY_SUMMARY_BYTES,
  REPLY_SUMMARY_LINES,
} from '../../dist/engine/capture-result.mjs';

const bin = join(import.meta.dirname, '../../bin/octocode-chrome-devtools.mjs');
const chromeCandidates = [
  '/Applications/Google Chrome.app',
  ...process.env.PATH.split(delimiter).flatMap(dir =>
    ['google-chrome', 'chromium', 'chrome'].map(name => join(dir, name))
  ),
];
const skip = !chromeCandidates.some(existsSync) && 'no Chrome installed';

const tokens = value => Math.ceil(JSON.stringify(value).length / 4);
/*
 * Ceilings, in tokens. A success reply is `ok` + the engine's own answer (`data`/`flow`/`snapshot`)
 * + at most a few workspace-relative paths. So each ceiling is "the answer's own size + a bounded
 * envelope", never a bound on the answer:
 * - ENVELOPE: everything outside data/flow/snapshot (ok, files, evidence, stderr path, next). A
 *   path is ~25 tokens, a labelled file ~40; a reply carries `evidence`, `stderr` and two or three
 *   `files` (the snapshot's page-snapshot.json and page-nodes.txt), so 130.
 * - DATA_ONLY_ENVELOPE: a reply with only `data` (+ a diagnostics path) is `ok` plus one path: 30.
 * - OPEN: the launch record the engine returns (ws url, profile, flags) is ~150 tokens; 220 total.
 * - CLEANUP: a four-field status record plus one stderr path; 100 total.
 * - NO_EVIDENCE_RUN: a plan whose only result is its flow summary (coverage, one source, one step)
 *   plus `evidence`; 200 total.
 */
const ENVELOPE = 130;
const DATA_ONLY_ENVELOPE = 30;
const OPEN = 220;
const CLEANUP = 100;
const NO_EVIDENCE_RUN = 200;

const removed = [
  'exitCode',
  'signal',
  'isPartial',
  'capture',
  'logs',
  'search',
  'root',
  'artifacts',
  'hints',
];
const answer = reply => ({
  ...(reply.data === undefined ? {} : { data: reply.data }),
  ...(reply.flow ? { flow: reply.flow } : {}),
  ...(reply.snapshot ? { snapshot: reply.snapshot } : {}),
  ...(reply.summary ? { summary: reply.summary } : {}),
});
const envelope = reply => {
  const { data, flow, snapshot, summary, ...rest } = reply;
  return tokens(rest);
};

test(
  'success replies are compact, workspace-relative and lead to the evidence',
  { skip, timeout: 240000 },
  async () => {
    const cwd = realpathSync(mkdtempSync(join(tmpdir(), 'octo-reply-')));
    mkdirSync(join(cwd, '.octocode'), { recursive: true });
    const web = spawn(
      process.execPath,
      [
        '-e',
        `require('node:http').createServer((q,r)=>{r.setHeader('content-type','text/html');r.end('<!doctype html><title>reply</title><h1>Reply</h1><button>Go</button><ul><li>a</li><li>b</li></ul>')}).listen(0,'127.0.0.1',function(){console.log(this.address().port)});`,
      ],
      { stdio: ['ignore', 'pipe', 'inherit'] }
    );
    const base = `http://127.0.0.1:${await new Promise(resolve =>
      web.stdout.once('data', chunk => resolve(String(chunk).trim()))
    )}/`;
    const port = await freePort();
    const connection = { port };
    const client = await connectStdio({
      command: process.execPath,
      args: [bin, '--mcp', '--preset', 'full'],
      cwd,
    });
    const read = file => readFileSync(join(cwd, file), 'utf8');
    const pathOf = entry => (typeof entry === 'string' ? entry : entry.path);
    const manifestOf = reply => JSON.parse(read(reply.evidence));
    // Every compact reply: small shape, text = structured content, no absolute workspace paths,
    // every named file exists relative to the workspace cwd, every next is an executable call.
    const call = async (name, args) => {
      const response = await client.callTool({ name, arguments: args });
      const reply = response.structuredContent;
      assert.ok(!response.isError, JSON.stringify(response.content));
      assert.equal(reply.ok, true);
      assert.deepEqual(JSON.parse(response.content[0].text), reply);
      for (const key of removed) assert(!(key in reply), `${name}: ${key}`);
      assert(!JSON.stringify(reply).includes(cwd), `${name}: absolute path`);
      for (const entry of reply.files ?? []) {
        assert(existsSync(join(cwd, pathOf(entry))), `${name}: ${entry}`);
        assert(!pathOf(entry).startsWith('/'));
      }
      for (const key of ['evidence', 'stderr'])
        if (reply[key]) assert(existsSync(join(cwd, reply[key])), key);
      for (const next of Object.values(reply.next ?? {})) {
        assert.equal(typeof next.tool, 'string');
        assert.equal(typeof next.query, 'object');
      }
      assert(
        envelope(reply) <= ENVELOPE,
        `${name}: envelope ${envelope(reply)}`
      );
      return reply;
    };
    // The saved manifest lists every file the reply names (the manifest is the whole inventory).
    const inManifest = reply => {
      const listed = manifestOf(reply).artifacts.map(artifact =>
        artifact.path.replace(`${cwd}/`, '')
      );
      for (const entry of reply.files ?? [])
        assert(listed.includes(pathOf(entry)), pathOf(entry));
      return listed;
    };
    try {
      const open = await call('open', { port, headless: true, url: base });
      assert.equal(open.data.status, 'BROWSER_READY');
      assert(tokens(open) <= OPEN, `open ${tokens(open)}`);
      assert(envelope(open) <= DATA_ONLY_ENVELOPE, `open ${envelope(open)}`);
      assert.match(read(open.stderr), /Launching Chrome/);

      const targets = await call('targets', { connection });
      assert(targets.data.some(entry => entry.type === 'page'));
      assert(envelope(targets) <= DATA_ONLY_ENVELOPE);
      // One page row costs far less than the 150-token ceiling for a one-tab reply.
      const pageRow = targets.data.find(entry => entry.type === 'page');
      assert(tokens({ ok: true, data: [pageRow] }) <= 150);

      const snapshot = await call('run', {
        connection,
        plan: { steps: [{ op: 'snapshot' }] },
      });
      assert.match(snapshot.snapshot, /\[e2\] button "Go"/);
      const nodes = snapshot.files
        .map(pathOf)
        .find(f => f.endsWith('page-nodes.txt'));
      assert.match(read(nodes), /button/);
      assert(inManifest(snapshot).length >= snapshot.files.length);
      assert.match(
        readFileSync(manifestOf(snapshot).logs.stdout, 'utf8'),
        /\[SNAPSHOT\] \[e2\]/
      );

      const screenshot = await call('check', {
        connection,
        recipe: 'page-screenshot',
      });
      assert.equal(screenshot.files.length, 1);
      assert(JSON.parse(read(pathOf(screenshot.files[0]))));
      inManifest(screenshot);

      const planned = await call('run', {
        connection,
        plan: { steps: [{ op: 'snapshot' }] },
      });
      assert(planned.flow.completedSteps >= 1);
      assert(planned.evidence);
      const listed = inManifest(planned);
      assert(listed.some(file => file.endsWith('browser-result.json')));
      assert(listed.some(file => file.endsWith('page-nodes.txt')));

      // A plan that snapshots inlines the page refs, like the standalone tool.
      const acted = await call('run', {
        connection,
        plan: {
          steps: [{ op: 'wait', selector: 'button' }, { op: 'snapshot' }],
        },
      });
      assert.match(acted.snapshot, /\[e2\] button "Go"/);
      assert(acted.snapshot.length <= INLINE_SNAPSHOT_BYTES);

      const extracted = await call('run', {
        connection,
        plan: { steps: [{ op: 'extract', selector: 'li' }] },
      });
      const step = extracted.flow.steps.find(entry => entry.op === 'extract');
      assert.deepEqual(
        step.data.rows.map(entry => entry.value.text),
        ['a', 'b']
      );
      // The same rows are on disk behind the manifest: the reply lost nothing.
      const file = manifestOf(extracted).artifacts.find(artifact =>
        /extract-\d+\.json$/.test(artifact.path)
      ).path;
      assert.deepEqual(
        JSON.parse(readFileSync(file, 'utf8')).map(entry => entry.text),
        ['a', 'b']
      );

      const goto = await call('run', {
        connection,
        plan: { steps: [{ op: 'goto', url: base }] },
      });
      assert(tokens(goto) <= NO_EVIDENCE_RUN, `run ${tokens(goto)}`);
      assert(!('files' in goto));

      const cdp = await call('cdp', {
        method: 'Browser.getVersion',
        connection,
      });
      assert.match(cdp.flow.steps[0].data.product, /Chrome/);

      const check = await call('check', {
        recipe: 'page-snapshot',
        connection,
      });
      assert(check.files.length >= 1);
      inManifest(check);
      assert.match(
        readFileSync(manifestOf(check).logs.stdout, 'utf8'),
        /\[ARTIFACT\]/
      );

      // A check reply carries the recipe's own metric lines within the summary ceilings.
      for (const [recipe, options] of [
        ['console-log', { CONSOLE_MS: 300 }],
        ['storage-dump', undefined],
      ]) {
        const checked = await call('check', { recipe, connection, options });
        assert(Array.isArray(checked.summary), recipe);
        assert(checked.summary.length <= REPLY_SUMMARY_LINES, recipe);
        assert(
          Buffer.byteLength(checked.summary.join('\n')) <= REPLY_SUMMARY_BYTES,
          recipe
        );
        assert(
          checked.summary.every(line =>
            /^\[(METRIC|FINDING|WARNING)\]/.test(line)
          )
        );
        const stdout = readFileSync(manifestOf(checked).logs.stdout, 'utf8');
        for (const line of checked.summary) assert(stdout.includes(line));
        assert.equal(checked.summaryTruncated, undefined);
      }

      const empty = await call('group', { action: 'list', port });
      assert.deepEqual(empty.data.groups, []);
      assert(envelope(empty) <= DATA_ONLY_ENVELOPE);

      // A group reply carries the answer in data and nothing else but a diagnostics path.
      const opened = await call('group', {
        action: 'open',
        name: 'compact',
        urls: [base, `${base}?b`],
        port,
      });
      assert.equal(opened.data.tabs.length, 2);
      assert(
        envelope(opened) <= DATA_ONLY_ENVELOPE,
        `group open ${envelope(opened)}`
      );
      assert.match(read(opened.stderr), /\S/);
      const listedGroups = await call('group', { action: 'list', port });
      const alive = listedGroups.data.groups.find(g => g.name === 'compact');
      assert.equal(alive.tabCount, opened.data.tabs.length);
      assert(envelope(listedGroups) <= DATA_ONLY_ENVELOPE);
      // Tab rows are the only per-tab cost: a two-tab group stays far under the open record.
      assert(
        tokens(listedGroups) <= OPEN,
        `group list ${tokens(listedGroups)}`
      );
      const closedGroup = await call('group', {
        action: 'close',
        name: 'compact',
        port,
      });
      assert(envelope(closedGroup) <= DATA_ONLY_ENVELOPE);
      const afterClose = await call('group', { action: 'list', port });
      assert.deepEqual(afterClose.data.groups, []);

      // A helper reply is its parsed answer plus a diagnostics path, nothing more.
      writeFileSync(
        join(cwd, 'tiny.har'),
        JSON.stringify(
          har(
            harEntry({ url: 'https://a.test/ok', status: 200, body: '{}' }),
            harEntry({ url: 'https://a.test/bad', status: 500, body: 'x' })
          )
        )
      );
      const helped = await call('helper', {
        name: 'har-pager',
        args: ['tiny.har', '--filter', 'failures', '--format', 'json'],
      });
      assert.equal(helped.data.pageRows.length, 1);
      assert(
        envelope(helped) <= DATA_ONLY_ENVELOPE,
        `helper ${envelope(helped)}`
      );

      const cleanup = await call('cleanup', { port });
      assert.equal(cleanup.data.status, 'CLEANED_UP');
      assert(tokens(cleanup) <= CLEANUP, `cleanup ${tokens(cleanup)}`);
      assert(envelope(cleanup) <= DATA_ONLY_ENVELOPE);
    } finally {
      await client
        .callTool({ name: 'cleanup', arguments: { port } })
        .catch(() => undefined);
      await client.close();
      web.kill();
      rmSync(cwd, { recursive: true, force: true });
    }
  }
);

test('browser-free success replies are compact and name only existing workspace paths', async () => {
  const cwd = realpathSync(mkdtempSync(join(tmpdir(), 'octo-reply-free-')));
  const client = await connectStdio({
    command: process.execPath,
    args: [bin, '--mcp', '--preset', 'full'],
    cwd,
  });
  try {
    for (const { name, args } of [
      { name: 'cleanup', args: { port: await freePort() } },
      { name: 'helper', args: { name: 'prune', args: ['--dry-run'] } },
      { name: 'flow', args: { action: 'list' } },
    ]) {
      const response = await client.callTool({ name, arguments: args });
      const reply = response.structuredContent;
      assert.ok(!response.isError, JSON.stringify(response.content));
      assert.equal(reply.ok, true);
      assert.deepEqual(JSON.parse(response.content[0].text), reply);
      for (const key of removed) assert(!(key in reply), `${name}: ${key}`);
      assert(!JSON.stringify(reply).includes(cwd), `${name}: absolute path`);
      for (const key of ['evidence', 'stderr'])
        if (reply[key]) assert(existsSync(join(cwd, reply[key])), key);
      assert(envelope(reply) <= DATA_ONLY_ENVELOPE, name);
      assert.deepEqual(Object.keys(answer(reply)), ['data']);
    }
  } finally {
    await client.close();
    rmSync(cwd, { recursive: true, force: true });
  }
});
