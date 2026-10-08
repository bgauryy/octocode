import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  cpSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
  existsSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { commands } from '../../dist/engine/cli-catalog.mjs';
const skill = join(dirname(fileURLToPath(import.meta.url)), '../..');
function fixture(fn) {
  const cwd = mkdtempSync(join(tmpdir(), 'octo-cdp-cli-'));
  const call = (args, options = {}) =>
    spawnSync(process.execPath, [join(skill, 'dist/engine/cli.mjs'), ...args], {
      cwd,
      encoding: 'utf8',
      timeout: 10000,
      ...options,
    });
  try {
    fn(call, cwd);
  } finally {
    rmSync(cwd, { recursive: true, force: true });
  }
}
test('help and schema discover generic operations without creating browser state', () =>
  fixture((call, cwd) => {
    assert.equal(call(['--help']).status, 0);
    const result = call(['schema', 'run']);
    assert.equal(result.status, 0, result.stderr);
    const schema = JSON.parse(result.stdout);
    assert(schema.plan.operations.listen);
    assert(schema.plan.operations.readStream);
    const launch = JSON.parse(call(['schema', 'open']).stdout);
    assert(launch.options.includes('--url'));
    assert.equal(launch.connection, undefined);
    assert(!existsSync(join(cwd, '.octocode')));
  }));
test('every registered command exposes help', () =>
  fixture(call => {
    for (const name of Object.keys(commands)) {
      const r = call([name, '--help']);
      assert.equal(r.status, 0, `${name}: ${r.stderr}`);
      assert(r.stdout.length + r.stderr.length > 0);
    }
  }));
test('all invalid plans and connection inputs fail before contacting Chrome', () =>
  fixture((call, cwd) => {
    const invalid = [
      ['run'],
      ['run', '--json', '{'],
      ['run', '--json', '{"steps":[]}'],
      [
        'run',
        '--json',
        '{"steps":[{"op":"goto","url":"https://example.com"},{"op":"unknown"}]}',
      ],
      ['step', '--json', '{"op":"act","action":"click"}'],
      ['step', '--json', '{"op":"waitEvent","listener":"future"}'],
      ['cdp', 'Runtime.evaluate', '--params', '[]'],
      ['cdp', 'bogus'],
      ['snapshot', '--options', '{"SECRET_TOKEN":"secret"}'],
      ['snapshot', '--port', '0'],
      ['snapshot', '--port', '65536'],
      ['snapshot', '--port', '9222', '--port', '9223'],
      ['snapshot', '--target', 'a', '--target-url', 'b'],
      ['snapshot', '--browser', '--target-type', 'page'],
      ['snapshot', '--new-tab', 'about:blank', '--target-type', 'iframe'],
      ['snapshot', '--keep-tab', '--close-tab'],
      ['snapshot', '--stealth', '--no-stealth'],
      ['snapshot', '--', '--new-tab', 'about:blank'],
      [
        'step',
        '--json',
        '{"op":"act","selector":"#q","action":"press","key":"NonexistentKey"}',
      ],
      ['snapshot', '--typo'],
      ['snapshot', '--json', '{}'],
      ['run', '--plan', 'absent.json'],
      ['check', 'missing'],
      ['schema', 'check', 'missing'],
      ['protocol', 'Runtime.evaluate.extra'],
    ];
    for (const args of invalid) {
      const r = call(args);
      assert.equal(r.status, 2, `${args}: ${r.stdout}${r.stderr}`);
      assert(!r.stderr.includes('Chrome not responding'));
    }
    assert(!existsSync(join(cwd, '.octocode')));
  }));
test('file, stdin and inline plans have identical validated execution', () =>
  fixture((call, cwd) => {
    const plan = {
      waitMs: 1000,
      steps: [
        {
          op: 'cdp',
          method: 'Runtime.evaluate',
          params: { expression: '1+1', returnByValue: true },
        },
      ],
    };
    const text = JSON.stringify(plan);
    writeFileSync(join(cwd, 'plan.json'), text);
    for (const [args, options] of [
      [['run', '--json', text], {}],
      [['run', '--plan', 'plan.json'], {}],
      [['run', '--plan', '-'], { input: text }],
    ]) {
      const r = call([...args, '--dry-run'], options);
      assert.equal(r.status, 0, r.stderr);
      const preview = JSON.parse(r.stdout);
      assert.equal(preview.executed, false);
      assert.deepEqual(JSON.parse(preview.env.BROWSER_PLAN), plan);
      assert(preview.args.includes('--strict-target'));
      assert(preview.args.includes('--keep-tab'));
      assert(preview.args.includes('--no-reload'));
    }
    assert(!existsSync(join(cwd, '.octocode')));
  }));
test('single operation, raw CDP and recipe options use the same engines', () =>
  fixture(call => {
    const r = call([
      'step',
      '--json',
      '{"op":"extract","selector":"main a"}',
      '--close-tab',
      '--dry-run',
    ]);
    assert.equal(r.status, 0, r.stderr);
    assert(!JSON.parse(r.stdout).args.includes('--keep-tab'));
    const raw = call(['cdp', 'Browser.getVersion', '--browser', '--dry-run']);
    assert.equal(raw.status, 0, raw.stderr);
    assert.equal(
      JSON.parse(JSON.parse(raw.stdout).env.BROWSER_PLAN).steps[0].method,
      'Browser.getVersion'
    );
    const capture = call([
      'snapshot',
      '--options',
      '{"SNAPSHOT_OUTLINE":true}',
      '--dry-run',
    ]);
    assert.equal(capture.status, 0, capture.stderr);
    assert.equal(JSON.parse(capture.stdout).env.SNAPSHOT_OUTLINE, '1');
  }));
test('standalone copied package works from another cwd; evidence continuations reconstruct all rows', () =>
  fixture((call, cwd) => {
    const copy = join(cwd, 'skill');
    cpSync(skill, copy, {
      recursive: true,
      filter: source => !source.split(/[\\/]/).includes('node_modules'),
    });
    const cli = args =>
      spawnSync(
        process.execPath,
        [join(copy, 'dist/engine/cli.mjs'), ...args],
        { cwd, encoding: 'utf8', timeout: 10000 }
      );
    for (const args of [
      ['--help'],
      ['schema'],
      ['step', '--json', '{"op":"wait","value":"Ready"}', '--dry-run'],
    ]) {
      const r = cli(args);
      assert.equal(r.status, 0, r.stderr);
    }
    const packageJson = JSON.parse(readFileSync(join(copy, 'package.json')));
    assert.equal(packageJson.bin, './bin/octocode-chrome-devtools.mjs');
    const rows = Array.from({ length: 23 }, (_, id) => ({
        id,
        text: 'item ' + id,
      })),
      file = join(cwd, 'rows.json');
    writeFileSync(file, JSON.stringify(rows));
    const r = cli(['query', '--file', file, '--limit', '3']);
    assert.equal(r.status, 0, r.stderr);
    let page = JSON.parse(r.stdout),
      found = [];
    for (;;) {
      found.push(...page.rows.map(row => row.value));
      if (!page.next) break;
      const n = page.next.continue;
      const next = spawnSync(n.command, n.args, {
        cwd,
        encoding: 'utf8',
        timeout: 10000,
      });
      assert.equal(next.status, 0, next.stderr);
      page = JSON.parse(next.stdout);
    }
    assert.deepEqual(found, rows);
  }));

test('large target inventories stay bounded and every target is reachable', async () => {
  const { createServer } = await import('node:http');
  const { execFile } = await import('node:child_process');
  const { promisify } = await import('node:util');
  const exec = promisify(execFile),
    cwd = mkdtempSync(join(tmpdir(), 'octo-targets-'));
  const targets = Array.from({ length: 35 }, (_, id) => ({
    id: String(id),
    type: 'page',
    url: 'http://fixture/' + id,
    title: 'evidence'.repeat(id === 0 ? 5000 : 400),
  }));
  const server = createServer((req, res) => {
    res.setHeader('content-type', 'application/json');
    res.end(
      JSON.stringify(
        req.url === '/json/version' ? { Browser: 'fixture' } : targets
      )
    );
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  try {
    const result = await exec(
      process.execPath,
      [
        join(skill, 'dist/engine/cli.mjs'),
        'targets',
        '--port',
        String(server.address().port),
      ],
      { cwd, timeout: 10000 }
    );
    assert(
      Buffer.byteLength(result.stdout) < 24000,
      'target stdout exceeded its response window'
    );
    const first = JSON.parse(result.stdout);
    assert.equal(first.count, targets.length);
    assert(first.next);
    let next = first.next.continue,
      found = [];
    while (next) {
      const r = await exec(next.command, next.args, { cwd, timeout: 10000 });
      const page = JSON.parse(r.stdout);
      assert(Buffer.byteLength(r.stdout) < 24000);
      for (const row of page.rows) {
        if (!row.oversized) found.push(row.value);
        else {
          let n = row.next.continue,
            text = '';
          while (n) {
            const v = JSON.parse(
              (await exec(n.command, n.args, { cwd, timeout: 10000 })).stdout
            );
            text += v.content;
            n = v.next?.continue;
          }
          found.push(JSON.parse(text).value);
        }
      }
      next = page.next?.continue;
    }
    assert.deepEqual(found, targets);
  } finally {
    await new Promise(resolve => server.close(resolve));
    rmSync(cwd, { recursive: true, force: true });
  }
});

test('operation discovery exposes extraction vocabulary and executable public examples', () =>
  fixture((call, cwd) => {
    const focused = call(['schema', 'run', 'extract']);
    assert.equal(focused.status, 0, focused.stderr);
    const data = JSON.parse(focused.stdout);
    assert.deepEqual(Object.keys(data.plan.operations), ['extract']);
    assert.deepEqual(data.plan.extraction.fields, [
      'text',
      'href',
      'value',
      'role',
      'name',
    ]);
    assert.deepEqual(data.plan.extraction.defaults, ['text', 'href']);
    assert(data.inputExamples.run.plan.steps.length);
    const invalid = call([
      'run',
      '--json',
      JSON.stringify({
        steps: [{ op: 'extract', selector: 'main', fields: ['html'] }],
      }),
      '--dry-run',
    ]);
    assert.equal(invalid.status, 2);
    assert.match(
      invalid.stderr,
      /supported fields: text, href, value, role, name/
    );
    assert(!existsSync(join(cwd, '.octocode')));
  }));
