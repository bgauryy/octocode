// The plan's snapshot and screenshot steps take the standalone tools' options: validated before
// any browser call, applied like the tool applies them, and absent options keep the default view.
import { spawnPageServer } from '../fixtures/page-server.mjs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, isAbsolute, join } from 'node:path';
import {
  validatePlan,
  run,
} from '../../dist/engine/cdp-checks/browser-execute.mjs';
import { chromeRecipeOptionNames } from '@octocodeai/config/chrome';
import { recipeEnv } from '../../dist/engine/cdp-checks/recipe-options.mjs';
import { fixture } from './plan-fixture.mjs';
import { freePort } from '../fixtures/free-port.mjs';

const bin = join(import.meta.dirname, '../../bin/octocode-chrome-devtools.mjs');
const RECIPES = { snapshot: 'page-snapshot', screenshot: 'page-screenshot' };
const chromeCandidates = [
  '/Applications/Google Chrome.app',
  ...process.env.PATH.split(delimiter).flatMap(dir =>
    ['google-chrome', 'chromium', 'chrome'].map(name => join(dir, name))
  ),
];
const skip = !chromeCandidates.some(existsSync) && 'no Chrome installed';

test('every option the contract declares for the two recipes is accepted by its op, and others are refused', () => {
  for (const [op, recipe] of Object.entries(RECIPES)) {
    const names = chromeRecipeOptionNames[recipe];
    assert.ok(names.length > 0);
    for (const name of names) {
      // Each name has a value rule; a value of the wrong kind never passes it.
      assert.throws(
        () => recipeEnv(recipe, { [name]: { nested: 1 } }),
        /Unsupported/
      );
    }
    assert.throws(
      () => validatePlan({ steps: [{ op, options: { NOT_AN_OPTION: 1 } }] }),
      /Unsupported .* option NOT_AN_OPTION/
    );
    assert.doesNotThrow(() => validatePlan({ steps: [{ op, options: {} }] }));
    assert.throws(
      () => validatePlan({ steps: [{ op, options: [] }] }),
      /options needs an object/
    );
  }
});

test('option values follow the tool schema: numbers, flags, enums and the annotate range', () => {
  const ok = (recipe, options) =>
    assert.doesNotThrow(
      () => recipeEnv(recipe, options),
      JSON.stringify(options)
    );
  const bad = (recipe, options) =>
    assert.throws(
      () => recipeEnv(recipe, options),
      /Unsupported/,
      JSON.stringify(options)
    );
  ok('page-screenshot', {
    SHOT_FULL: true,
    SHOT_SCALE: '0.5',
    SHOT_FORMAT: 'png',
  });
  ok('page-screenshot', { SHOT_ANNOTATE: 'e20-e80' });
  ok('page-screenshot', { SHOT_ANNOTATE: true });
  bad('page-screenshot', { SHOT_FORMAT: 'gif' });
  bad('page-screenshot', { SHOT_SCALE: 'big' });
  bad('page-screenshot', { SHOT_FULL: 'yes' });
  bad('page-screenshot', { SHOT_ANNOTATE: 'e20' });
  ok('page-snapshot', {
    SNAPSHOT_DEPTH: 2,
    SNAPSHOT_STDOUT: 'summary',
    SNAPSHOT_CONTEXT: false,
  });
  bad('page-snapshot', { SNAPSHOT_STDOUT: 'full' });
  bad('page-snapshot', { SNAPSHOT_MAX: 'many' });
  assert.deepEqual(
    recipeEnv('page-snapshot', { SNAPSHOT_CONTEXT: false, SNAPSHOT_MAX: 5 }),
    {
      SNAPSHOT_CONTEXT: '0',
      SNAPSHOT_MAX: '5',
    }
  );
});

test('screenshot options reach the capture and absent options keep the default view', async () => {
  const sendsFor = async step => {
    let sends;
    await fixture(async ({ cdp }) => {
      sends = [];
      cdp.send = async (method, params) => {
        sends.push({ method, params });
        if (method === 'Runtime.evaluate')
          return { result: { value: 'complete' } };
        if (method === 'Page.captureScreenshot')
          return { data: Buffer.from('image').toString('base64') };
        return {};
      };
      await run(cdp, { steps: [step] });
    });
    return sends.find(s => s.method === 'Page.captureScreenshot').params;
  };
  const plain = await sendsFor({ op: 'screenshot' });
  assert.equal(plain.format, 'jpeg');
  assert.equal(plain.clip, undefined);
  const png = await sendsFor({
    op: 'screenshot',
    options: { SHOT_FORMAT: 'png' },
  });
  assert.equal(png.format, 'png');
  assert.equal(png.quality, undefined);
});

test('a snapshot wait that is never met fails the step and resets the recipe exit code', async () =>
  fixture(async ({ cdp, read }) => {
    const before = process.exitCode;
    cdp.send = async method =>
      method === 'Runtime.evaluate'
        ? { result: { value: false } }
        : { root: { nodeId: 1 } };
    await run(cdp, {
      steps: [
        {
          op: 'snapshot',
          timeoutMs: 1000,
          options: { SNAPSHOT_WAIT_SELECTOR: '#never', SNAPSHOT_WAIT_MS: 200 },
        },
      ],
    });
    assert.equal(read('browser-result.json').steps[0].status, 'failed');
    process.exitCode = before;
  }));

const page = `<!doctype html><title>Tall</title><h1>Tall</h1><button>A</button><button>B</button>
<div id=box style="width:300px;height:120px;background:#c00">box</div>
<div style="height:20000px">spacer</div><button>Z</button>`;

test(
  'on real Chrome a plan screenshot tiles a tall page and clips an element, and snapshot options match the standalone tool',
  { skip, timeout: 180000 },
  async () => {
    const cwd = mkdtempSync(join(tmpdir(), 'octo-capture-options-'));
    const web = spawnPageServer(page);
    const webPort = await new Promise(resolve =>
      web.stdout.once('data', chunk => resolve(Number(chunk)))
    );
    const port = await freePort();
    const engine = (...args) =>
      spawnSync(process.execPath, [bin, '/raw', ...args], {
        cwd,
        encoding: 'utf8',
      });
    const plan = (...steps) =>
      engine(
        'run',
        '--port',
        String(port),
        '--json',
        JSON.stringify({ waitMs: 5000, steps })
      );
    const artifact = (out, name) => {
      const hit = new RegExp(`\\[ARTIFACT\\] ${name} (\\S+)`).exec(out.stdout);
      assert.ok(hit, `${name} in ${out.stdout}${out.stderr}`);
      return hit[1];
    };
    const manifestOf = out =>
      JSON.parse(
        readFileSync(artifact(out, 'screenshot-manifest.json'), 'utf8')
      );
    try {
      const opened = spawnSync(
        process.execPath,
        [bin, '/cli', 'open', '--input', '-', '--json'],
        {
          cwd,
          encoding: 'utf8',
          input: JSON.stringify({
            port,
            headless: true,
            url: `http://127.0.0.1:${webPort}/`,
          }),
        }
      );
      assert.match(opened.stdout, /BROWSER_READY/);

      // full: the tool's tiling, several tiles
      const full = plan({ op: 'screenshot', options: { SHOT_FULL: true } });
      assert.equal(full.status, 0, full.stdout + full.stderr);
      const fullManifest = manifestOf(full);
      assert.equal(fullManifest.mode, 'full');
      assert.ok(fullManifest.tiles.length > 1);
      assert.equal(fullManifest.complete, true);
      // Tile paths are workspace-relative and exist relative to the cwd.
      for (const tile of fullManifest.tiles) {
        assert.ok(!isAbsolute(tile.file), tile.file);
        assert.ok(existsSync(join(cwd, tile.file)), tile.file);
      }

      // selector: one image whose pixel size is the element's
      const el = plan({
        op: 'screenshot',
        options: { SHOT_SELECTOR: '#box', SHOT_FORMAT: 'png' },
      });
      assert.equal(el.status, 0, el.stdout + el.stderr);
      const elManifest = manifestOf(el);
      assert.equal(elManifest.mode, 'element');
      assert.equal(elManifest.tiles.length, 1);
      const png = readFileSync(join(cwd, elManifest.tiles[0].file));
      const [w, h] = [png.readUInt32BE(16), png.readUInt32BE(20)];
      const { width, height } = elManifest.tiles[0].clip;
      assert.equal(
        w / width,
        h / height,
        'pixels scale uniformly from the element size'
      );
      assert.ok(Math.abs(w / h - 300 / 120) < 0.02, `${w}x${h}`);

      // a missing element fails the step
      const missing = plan({
        op: 'screenshot',
        options: { SHOT_SELECTOR: '#absent' },
      });
      assert.notEqual(missing.status, 0);

      // snapshot options give the standalone tool's refs
      const options = {
        SNAPSHOT_MAX: 2,
        SNAPSHOT_PAGE: 2,
        SNAPSHOT_CONTEXT: false,
      };
      const inPlan = plan({ op: 'snapshot', options });
      const alone = engine(
        'check',
        'page-snapshot',
        '--port',
        String(port),
        '--options',
        JSON.stringify(options)
      );
      assert.equal(inPlan.status, 0, inPlan.stdout + inPlan.stderr);
      assert.equal(alone.status, 0, alone.stdout + alone.stderr);
      const lines = out =>
        out.stdout
          .split('\n')
          .filter(l => /^(\[SNAPSHOT\]|\[PAGE\]|\[METRIC\] SNAPSHOT)/.test(l));
      assert.ok(lines(inPlan).some(l => l.startsWith('[SNAPSHOT] [e3]')));
      assert.deepEqual(lines(inPlan), lines(alone));
      const refs = out =>
        Object.values(
          JSON.parse(readFileSync(artifact(out, 'PAGE_SNAPSHOT'), 'utf8'))
            .refs ?? {}
        ).map(r => [r.role, r.name]);
      assert.deepEqual(refs(inPlan), refs(alone));

      // no options: the default view, as before
      const def = plan({ op: 'snapshot' });
      assert.ok(lines(def).some(l => /\[e1\]/.test(l)));
    } finally {
      spawnSync(
        process.execPath,
        [bin, '/cli', 'cleanup', '--input', '-', '--json'],
        {
          cwd,
          input: JSON.stringify({ port }),
        }
      );
      web.kill();
      rmSync(cwd, { recursive: true, force: true });
    }
  }
);

test('an invalid option is refused before any browser connection', () => {
  const out = spawnSync(
    process.execPath,
    [
      bin,
      '/raw',
      'run',
      '--port',
      '1',
      '--json',
      JSON.stringify({
        steps: [{ op: 'screenshot', options: { SHOT_FORMAT: 'gif' } }],
      }),
    ],
    { encoding: 'utf8' }
  );
  assert.notEqual(out.status, 0);
  assert.match(
    out.stdout + out.stderr,
    /Unsupported page-screenshot option SHOT_FORMAT/
  );
  assert.doesNotMatch(out.stdout + out.stderr, /ECONNREFUSED|connect/i);
});

test(
  'through MCP a run screenshot step with options tiles a tall page',
  { skip, timeout: 180000 },
  async () => {
    const { connectStdio } = await import('../../dist/runtime.js');
    const cwd = mkdtempSync(join(tmpdir(), 'octo-capture-mcp-'));
    const web = spawnPageServer(page);
    const webPort = await new Promise(resolve =>
      web.stdout.once('data', chunk => resolve(Number(chunk)))
    );
    const port = await freePort();
    const client = await connectStdio({
      command: process.execPath,
      args: [bin],
      cwd,
    });
    const text = reply => (reply.content ?? []).map(p => p.text ?? '').join('');
    try {
      const opened = await client.callTool({
        name: 'open',
        arguments: {
          port,
          headless: true,
          url: `http://127.0.0.1:${webPort}/`,
        },
      });
      assert.notEqual(opened.isError, true, text(opened));
      const call = options => ({
        name: 'run',
        arguments: {
          connection: { port },
          plan: { waitMs: 5000, steps: [{ op: 'screenshot', options }] },
        },
      });
      for (const bad of [{ SHOT_FORMAT: 'gif' }, { BOGUS: 1 }]) {
        const refused = await client.callTool(call(bad));
        assert.equal(refused.isError, true, JSON.stringify(bad));
        assert.match(text(refused), /SHOT_FORMAT|Unsupported/);
      }
      const reply = await client.callTool(call({ SHOT_FULL: true }));
      assert.notEqual(reply.isError, true, text(reply));
      const manifests = [];
      const walk = dir => {
        for (const entry of readdirSync(dir, { withFileTypes: true })) {
          const path = join(dir, entry.name);
          if (entry.isDirectory()) walk(path);
          else if (entry.name === 'screenshot-manifest.json')
            manifests.push(path);
        }
      };
      walk(cwd);
      assert.ok(manifests.length > 0, 'a screenshot manifest was written');
      const manifest = JSON.parse(readFileSync(manifests[0], 'utf8'));
      assert.equal(manifest.mode, 'full');
      assert.ok(manifest.tiles.length > 1);
      assert.equal(manifest.complete, true);
    } finally {
      await client.close();
      spawnSync(
        process.execPath,
        [bin, '/cli', 'cleanup', '--input', '-', '--json'],
        { cwd, input: JSON.stringify({ port }) }
      );
      web.kill();
      rmSync(cwd, { recursive: true, force: true });
    }
  }
);
