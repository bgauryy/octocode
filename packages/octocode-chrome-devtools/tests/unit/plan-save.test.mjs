// `save` writes a step's raw result under <run folder>/saved/<name>.
import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { run } from '../../dist/engine/cdp-checks/browser-execute.mjs';
import {
  commitSave,
  stageSave,
} from '../../dist/engine/cdp-checks/plan-save.mjs';
import { call, fixture } from './plan-fixture.mjs';

const HTML =
  '<!doctype html><html><body><p id="needle-42">hello</p></body></html>';

/** Answers Runtime.evaluate by expression; any other method returns `others[method]` or {}. */
const answer = (cdp, byExpression, others = {}) => {
  cdp.send = async (method, params) => {
    if (method === 'Runtime.evaluate') {
      const value = byExpression[params.expression];
      if (value instanceof Error)
        return {
          exceptionDetails: {
            text: 'Uncaught',
            exception: { description: `Error: ${value.message}` },
          },
        };
      return { result: { value } };
    }
    return others[method] ?? {};
  };
};

/** Runs a plan and returns the stdout lines it printed. */
async function runLogged(cdp, plan) {
  const lines = [];
  const log = console.log;
  console.log = (...a) => lines.push(a.join(' '));
  try {
    await run(cdp, plan);
  } finally {
    console.log = log;
  }
  return lines;
}

test('evaluate save keeps HTML as HTML, lists the file and a substring search finds it', async () =>
  fixture(async ({ cdp, dir, read }) => {
    answer(cdp, { 'document.documentElement.outerHTML': HTML });
    const lines = await runLogged(cdp, {
      steps: [
        {
          op: 'evaluate',
          expression: 'document.documentElement.outerHTML',
          save: 'page.html',
        },
      ],
    });
    const file = join(dir, 'saved', 'page.html');
    const text = readFileSync(file, 'utf8');
    assert.equal(text, HTML);
    assert.ok(text.includes('needle-42'));
    assert.equal(statSync(file).mode & 0o777, 0o600);
    assert.equal(statSync(join(dir, 'saved')).mode & 0o777, 0o700);
    assert.ok(lines.includes(`[ARTIFACT] SAVED_page.html ${file}`));
    assert.equal(read('browser-result.json').steps[0].saved, file);
  }));

test('cdp save writes the result object as pretty JSON; extract save writes its rows', async () =>
  fixture(async ({ cdp, dir }) => {
    const rows = [{ text: 'one', href: null }];
    cdp.send = async method =>
      method === 'Runtime.evaluate'
        ? { result: { value: { rows, url: 'about:blank', title: 't' } } }
        : { root: { nodeId: 1 } };
    await runLogged(cdp, {
      steps: [
        call('DOM.getDocument', {}, { save: 'dom.json' }),
        { op: 'extract', selector: 'a', save: 'rows.json' },
      ],
    });
    assert.equal(
      readFileSync(join(dir, 'saved', 'dom.json'), 'utf8'),
      `${JSON.stringify({ root: { nodeId: 1 } }, null, 2)}\n`
    );
    assert.deepEqual(
      JSON.parse(readFileSync(join(dir, 'saved', 'rows.json'), 'utf8')),
      rows
    );
  }));

test('a failed step and an optional-failed step save nothing', async () =>
  fixture(async ({ cdp, dir }) => {
    answer(cdp, { ok: 'fine', boom: new Error('nope') });
    await runLogged(cdp, {
      steps: [
        {
          op: 'evaluate',
          expression: 'boom',
          save: 'soft.txt',
          optional: true,
        },
        { op: 'evaluate', expression: 'ok', save: 'ok.txt' },
        { op: 'evaluate', expression: 'boom', save: 'hard.txt' },
      ],
    });
    assert.ok(existsSync(join(dir, 'saved', 'ok.txt')));
    assert.ok(!existsSync(join(dir, 'saved', 'soft.txt')));
    assert.ok(!existsSync(join(dir, 'saved', 'hard.txt')));
  }));

test('typed fill values are scrubbed from saved text, strings and JSON alike', () =>
  fixture(async ({ dir }) => {
    const secret = 'hunter2-secret';
    const state = {
      plan: {
        steps: [{ op: 'act', action: 'fill', selector: '#p', value: secret }],
      },
      connection: { outputDir: dir },
      log: () => {},
    };
    for (const [name, value] of [
      ['a.html', `<input value="${secret}">`],
      ['b.json', { v: [`x ${secret} y`] }],
    ]) {
      const current = { step: { op: 'evaluate', save: name }, row: {} };
      stageSave(current, value);
      commitSave(state, current);
      assert.ok(
        !readFileSync(join(dir, 'saved', name), 'utf8').includes(secret)
      );
    }
  }));

test('a save name can never leave saved/', () =>
  fixture(async ({ dir }) => {
    const state = {
      plan: { steps: [] },
      connection: { outputDir: dir },
      log: () => {},
    };
    for (const name of ['../x', 'a/b', '..', '']) {
      const current = { step: { op: 'evaluate', save: name }, row: {} };
      stageSave(current, 'x');
      assert.throws(() => commitSave(state, current), /outside saved/);
    }
  }));

const PNG = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  Buffer.from('payload-bytes'),
]);
const JPG = Buffer.from([0xff, 0xd8, 0xff, 0xe0, 1, 2, 3, 4, 5]);

test('a cdp save with an image or pdf extension writes decoded bytes, not base64 JSON', async () =>
  fixture(async ({ cdp, dir }) => {
    const byMethod = {
      'Page.captureScreenshot': { data: PNG.toString('base64') },
      'Page.printToPDF': { data: JPG.toString('base64') },
    };
    cdp.send = async method => byMethod[method] ?? {};
    await runLogged(cdp, {
      steps: [
        call('Page.captureScreenshot', {}, { save: 'shot.png' }),
        call('Page.printToPDF', {}, { save: 'shot.jpg' }),
        call('Page.captureScreenshot', {}, { save: 'raw.json' }),
      ],
    });
    const png = readFileSync(join(dir, 'saved', 'shot.png'));
    assert.deepEqual([...png.subarray(0, 4)], [0x89, 0x50, 0x4e, 0x47]);
    assert.equal(png.length, PNG.length);
    assert.equal(statSync(join(dir, 'saved', 'shot.png')).mode & 0o777, 0o600);
    const jpg = readFileSync(join(dir, 'saved', 'shot.jpg'));
    assert.deepEqual([...jpg.subarray(0, 2)], [0xff, 0xd8]);
    // a name without a binary extension keeps the JSON form
    assert.deepEqual(
      JSON.parse(readFileSync(join(dir, 'saved', 'raw.json'), 'utf8')),
      byMethod['Page.captureScreenshot']
    );
  }));

test('captureSnapshot data saves as mhtml text; non-base64 data for a binary extension is refused', async () =>
  fixture(async ({ cdp, dir, read }) => {
    cdp.send = async method =>
      method === 'Page.captureSnapshot'
        ? { data: 'MIME-Version: 1.0\r\n--boundary' }
        : { data: 'not base64!' };
    await runLogged(cdp, {
      steps: [
        call('Page.captureSnapshot', {}, { save: 'page.mhtml' }),
        call('Page.captureScreenshot', {}, { save: 'bad.png' }),
      ],
    });
    assert.match(
      readFileSync(join(dir, 'saved', 'page.mhtml'), 'utf8'),
      /^MIME-Version/
    );
    assert.ok(!existsSync(join(dir, 'saved', 'bad.png')));
    assert.match(read('browser-result.json').steps[1].error, /not base64/);
  }));
