import test from 'node:test';
import assert from 'node:assert/strict';
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  readFileSync,
  rmSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { captureResult } from '../../dist/engine/capture-result.mjs';
import {
  fixture,
  shape,
  call,
  typedCli,
  writeOutputs,
} from './capture-fixture.mjs';
test('native Octocode can search the capture files and fetch exact evidence', () =>
  fixture(async work => {
    const octo = resolve(
      import.meta.dirname,
      '../../../octocode/out/octocode.js'
    );
    const file = join(work, 'response.json');
    writeFileSync(
      file,
      JSON.stringify({ body: 'Needle in captured web content' }, null, 2)
    );
    const result = await shape(work, `[ARTIFACT] RESPONSE ${file}`);
    // Replies carry workspace-relative `files`; they feed localSearch/localFetch unchanged.
    assert.deepEqual(result.files, ['response.json']);
    const scope = {
      path: result.files[0],
      includeHidden: true,
      includeIgnored: true,
      matchString: ['Needle'],
      regexEngine: 'literal',
    };
    const found = spawnSync(
      process.execPath,
      [octo, 'localSearch', JSON.stringify({ queries: [scope] })],
      { cwd: work, encoding: 'utf8' }
    );
    assert.equal(found.status, 0, found.stderr + found.stdout);
    assert.match(found.stdout, /Needle in captured web content/);
    const read = spawnSync(
      process.execPath,
      [
        octo,
        'localFetch',
        JSON.stringify({ queries: [{ path: result.files[0] }] }),
      ],
      { cwd: work, encoding: 'utf8' }
    );
    assert.equal(read.status, 0, read.stderr + read.stdout);
    assert.match(read.stdout, /Needle in captured web content/);
  }));

test('input acknowledgement omits event payloads but keeps exact evidence reachable', () =>
  fixture(async (work, _client) => {
    const action = join(work, 'action.json');
    const flow = join(work, 'browser-result.json');
    writeFileSync(
      action,
      JSON.stringify({ events: [{ value: 'private typed value' }] })
    );
    writeFileSync(
      flow,
      JSON.stringify({
        ok: true,
        requestedSteps: 1,
        completedSteps: 1,
        steps: [
          {
            index: 1,
            op: 'act',
            status: 'complete',
            artifact: action,
            condition: { verified: true, text: 'Saved' },
          },
        ],
        eventCoverage: [],
      })
    );
    const result = await shape(
      work,
      `[ARTIFACT] ACTION ${action}\n[ARTIFACT] browser-result.json ${flow}`
    );
    assert.equal(result.flow.steps[0].status, 'complete');
    assert.deepEqual(result.flow.steps[0].condition, {
      verified: true,
      text: 'Saved',
    });
    assert(!JSON.stringify(result).includes('private typed value'));
    assert(!result.artifacts);
    assert(Buffer.byteLength(JSON.stringify(result)) < 1800);
    // The acknowledged input's exact record stays reachable as a listed file.
    assert.deepEqual(result.files, ['action.json']);
    assert.equal(result.evidence, 'capture/capture.json');
    assert.equal(
      JSON.parse(readFileSync(action, 'utf8')).events[0].value,
      'private typed value'
    );
  }));

test('a completed input without a postcondition returns only an acknowledgement and full-evidence routes', () =>
  fixture(async (work, _client) => {
    const artifact = join(work, 'input.json'),
      flow = join(work, 'browser-result.json');
    writeFileSync(
      artifact,
      JSON.stringify({ events: [{ value: 'Do not echo this input' }] })
    );
    writeFileSync(
      flow,
      JSON.stringify({
        ok: true,
        requestedSteps: 1,
        completedSteps: 1,
        steps: [{ index: 1, op: 'act', status: 'complete', artifact }],
        eventCoverage: [],
      })
    );
    const result = await shape(
      work,
      `[ARTIFACT] INPUT ${artifact}\n[ARTIFACT] browser-result.json ${flow}`
    );
    assert.equal(result.flow.completedSteps, 1);
    assert.deepEqual(result.flow.steps, []);
    assert(!JSON.stringify(result).includes('Do not echo this input'));
    assert(Buffer.byteLength(JSON.stringify(result)) < 1000);
    assert.deepEqual(result.files, ['input.json']);
    assert.equal(
      JSON.parse(readFileSync(join(work, result.files[0]), 'utf8')).events[0]
        .value,
      'Do not echo this input'
    );
  }));

test('target inventories expose executable typed continuations through CLI and MCP', () =>
  fixture(async (work, client) => {
    const file = join(work, 'targets.json');
    const rows = Array.from({ length: 35 }, (_, index) => ({
      id: String(index),
      title: `target ${index}`,
    }));
    writeFileSync(file, JSON.stringify(rows));
    const result = await shape(
      work,
      JSON.stringify({
        rows: [],
        next: {
          continue: {
            command: process.execPath,
            args: [
              resolve(
                import.meta.dirname,
                '../../dist/engine/evidence-query.mjs'
              ),
              '--file',
              file,
              '--limit',
              '7',
            ],
          },
        },
      }),
      '',
      'targets'
    );
    assert.equal(result.data.next.tool, 'query');
    const reconstructed = [];
    let next = result.data.next;
    while (next) {
      const page = (await call(client, next)).data;
      reconstructed.push(...page.rows.map(row => row.value));
      next = page.next;
    }
    assert.deepEqual(reconstructed, rows);
    const cli = typedCli(work, ['query', '--input', '-', '--json'], {
      input: JSON.stringify(result.data.next.query),
    });
    assert.equal(cli.status, 0, cli.stderr);
    assert.deepEqual(
      JSON.parse(cli.stdout).structuredContent.data.rows.map(row => row.value),
      rows.slice(0, 7)
    );
  }));
test('cdp and protocol plans return the reply value and Chrome error inline', () =>
  fixture(async work => {
    const reply = join(work, 'cdp-1.json'),
      flow = join(work, 'browser-result.json');
    writeFileSync(
      reply,
      JSON.stringify({ result: { type: 'number', value: 2 } })
    );
    writeFileSync(
      flow,
      JSON.stringify({
        ok: false,
        requestedSteps: 2,
        completedSteps: 1,
        failure: {
          step: 2,
          error: "CDP error [-32601]: 'Runtime.nope' wasn't found",
        },
        steps: [
          { index: 1, op: 'cdp', status: 'complete', artifact: reply },
          {
            index: 2,
            op: 'cdp',
            status: 'failed',
            error: "CDP error [-32601]: 'Runtime.nope' wasn't found",
          },
        ],
        eventCoverage: [],
      })
    );
    for (const command of ['cdp', 'protocol']) {
      const result = await shape(
        mkdtempSync(join(work, `${command}-`)),
        `[ARTIFACT] cdp-1.json ${reply}\n[ARTIFACT] browser-result.json ${
          flow
        }`,
        '[CDP_SANDBOX] diagnostic',
        command
      );
      assert.match(result.flow.failure.error, /-32601/);
      assert.equal(JSON.stringify(result.flow.steps[0].data.result.value), '2');
      assert(!result.error);
    }
  }));
test('stderr is the error only for failures; success diagnostics stay behind a stderr path', async () => {
  const work = mkdtempSync(join(tmpdir(), 'octo-capture-'));
  try {
    const artifact = join(work, 'snapshot.json');
    writeFileSync(artifact, '{}');
    for (const exitCode of [0, 2]) {
      const directory = join(work, `capture-${exitCode}`);
      mkdirSync(directory);
      const outputs = writeOutputs(directory, {
        stdout: `[ARTIFACT] snapshot ${artifact}`,
        stderr: '[CDP_SANDBOX] diagnostic',
      });
      if (exitCode === 0) {
        // Inline JSON data tolerates stderr: a banner leaves no trace, a real diagnostic keeps a path.
        for (const [stderr, expected] of [
          ['[CDP_SANDBOX] sandboxed (node 1, fs scoped)\n', undefined],
          ['[BROWSER] note\n', 'capture-0/stderr.txt'],
        ]) {
          const inline = join(work, 'inline');
          mkdirSync(inline, { recursive: true });
          const reply = await captureResult(
            { exitCode },
            writeOutputs(inline, { stdout: '{"a":1}', stderr }),
            inline,
            'cleanup',
            undefined,
            work
          );
          assert.equal(JSON.stringify(reply.data), '{"a":1}');
          assert.equal(
            reply.stderr,
            expected && expected.replace('capture-0', 'inline')
          );
          assert(!('capture' in reply) && !('next' in reply));
        }
      }
      const result = await captureResult(
        { exitCode },
        outputs,
        directory,
        'snapshot',
        undefined,
        work
      );
      assert.equal(result.ok, exitCode === 0);
      if (exitCode === 0) {
        assert(!('logs' in result) && !('stderr' in result), 'banner only');
      } else assert.equal(result.logs.stderr, outputs.stderr.file);
      assert.equal(
        result.error,
        exitCode === 0 ? undefined : '[CDP_SANDBOX] diagnostic'
      );
    }
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
});
