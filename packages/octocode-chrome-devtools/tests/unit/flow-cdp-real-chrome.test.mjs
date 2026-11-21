// CDP built-in flows on a real isolated headless Chrome: each flow is expanded with `flow show`,
// spliced into a larger plan (renumbering $step) with a page check, and run with `run`.
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, join } from 'node:path';
import { freePort } from '../fixtures/free-port.mjs';

const bin = join(import.meta.dirname, '../../bin/octocode-chrome-devtools.mjs');
const chromeCandidates = [
  '/Applications/Google Chrome.app',
  ...process.env.PATH.split(delimiter).flatMap(dir =>
    ['google-chrome', 'chromium', 'chrome'].map(name => join(dir, name))
  ),
];
const skip = !chromeCandidates.some(existsSync) && 'no Chrome installed';
const bare =
  '<!doctype html><title>Bare</title><div id=box>no viewport meta</div>';
const page = `<!doctype html><title>Fixture</title><meta name=viewport content="width=device-width"><style>#box{display:flex}</style>
<div id=box>box</div><button id=go>Press me</button>
<script>document.getElementById('go').addEventListener('click', function clickHandler() {});</script>`;

/** Adds `by` to every $step reference: the renumbering a splice needs. */
const shift = (node, by) =>
  Array.isArray(node)
    ? node.map(item => shift(item, by))
    : node && typeof node === 'object'
      ? Object.fromEntries(
          Object.entries(node).map(([key, value]) => [
            key,
            key === '$step' ? value + by : shift(value, by),
          ])
        )
      : node;

test(
  'built-in CDP flows spliced into a plan act on a real Chrome',
  { skip, timeout: 240000 },
  async () => {
    const cwd = mkdtempSync(join(tmpdir(), 'octo-flow-cdp-'));
    const web = spawn(
      process.execPath,
      [
        '-e',
        `require('node:http').createServer((q, r) => { r.setHeader('content-type', 'text/html'); r.end(q.url === '/nometa' ? ${JSON.stringify(bare)} : ${JSON.stringify(page)}); }).listen(0, '127.0.0.1', function () { console.log(this.address().port); });`,
      ],
      { stdio: ['ignore', 'pipe', 'inherit'] }
    );
    const webPort = await new Promise(resolve =>
      web.stdout.once('data', chunk => resolve(Number(chunk)))
    );
    const url = `http://127.0.0.1:${webPort}/`;
    const port = await freePort();
    const cli = (command, input) => {
      writeFileSync(join(cwd, 'in.json'), JSON.stringify(input));
      const r = spawnSync(
        process.execPath,
        [bin, '/cli', command, '--input', 'in.json', '--json'],
        { cwd, encoding: 'utf8' }
      );
      const parsed = JSON.parse(r.stdout || r.stderr);
      return parsed.structuredContent ?? parsed;
    };
    const show = (name, params) => {
      const r = spawnSync(
        process.execPath,
        [bin, '/raw', 'flow', 'show', name, '--params', JSON.stringify(params)],
        { cwd, encoding: 'utf8' }
      );
      const out = JSON.parse(r.stdout);
      assert.equal(out.valid, true, r.stdout);
      assert.equal(out.spliceable, true);
      return out.steps;
    };
    /** goto + flow steps + optional extra steps; returns the saved JSON of each cdp step. */
    const run = (flowSteps, extra = [], path = '') => {
      const before = 1; // the goto
      const plan = {
        steps: [
          { op: 'goto', url: url + path },
          ...shift(flowSteps, before),
          ...extra,
        ],
      };
      const r = spawnSync(
        process.execPath,
        [bin, '/raw', 'run', '--plan', '-', '--port', String(port)],
        { cwd, encoding: 'utf8', input: JSON.stringify(plan) }
      );
      const saved = {};
      for (const [, file] of (r.stdout + r.stderr).matchAll(
        /\[ARTIFACT\] (?:\S+ )?(\S*(?:cdp-\d+\.json|events-[\w-]+\.jsonl))\n/g
      ))
        saved[file.split('/').pop()] = readFileSync(file, 'utf8');
      assert.match(r.stdout, /completed=\d+\/\d+/, r.stdout + r.stderr);
      return {
        step: n => JSON.parse(saved[`cdp-${n}.json`]),
        saved,
        out: r.stdout,
      };
    };
    const evaluate = expression => ({
      op: 'cdp',
      method: 'Runtime.evaluate',
      acknowledge: true,
      params: { expression, returnByValue: true },
    });
    try {
      assert.equal(cli('open', { port, headless: true, url }).ok, true);

      // emulate-device + check in the SAME plan
      const device = show('emulate-device', {
        width: 412,
        height: 800,
        mobile: true,
        touch: true,
      });
      const emulated = run(device, [
        evaluate(
          'JSON.stringify([innerWidth, matchMedia("(pointer: coarse)").matches])'
        ),
      ]);
      const last = Math.max(
        ...Object.keys(emulated.saved).map(k => Number(/\d+/.exec(k)?.[0]))
      );
      assert.deepEqual(JSON.parse(emulated.step(last).result.value), [
        412,
        true,
      ]);

      // The emulated viewport is WxH (innerWidth/innerHeight and visualViewport); with mobile true the
      // screen reports it too, also for a page without <meta name=viewport> (measured on this Chrome).
      const probe = evaluate(
        'JSON.stringify([innerWidth, innerHeight, visualViewport.width, visualViewport.height, screen.width, screen.height])'
      );
      const measured = (flowArgs, path) => {
        const out = run(show('emulate-device', flowArgs), [probe], path);
        const lastStep = Math.max(
          ...Object.keys(out.saved).map(k => Number(/\d+/.exec(k)?.[0]))
        );
        return JSON.parse(out.step(lastStep).result.value);
      };
      const desktop = measured({ width: 412, height: 800 }, '/nometa');
      // [innerWidth, innerHeight, visualViewport.width, visualViewport.height, screen.width, screen.height]
      assert.deepEqual(desktop.slice(0, 4), [412, 800, 412, 800]);
      assert.notDeepEqual(
        desktop.slice(4),
        [412, 800],
        'without mobile the screen keeps the window size'
      );
      const mobileBare = measured(
        { width: 412, height: 800, mobile: true },
        '/nometa'
      );
      // Mobile layout belongs to Chrome and varies by version (a page without a viewport meta may lay
      // out wider than requested): assert only that the probe measures a real viewport and the screen follows.
      for (const size of [
        mobileBare,
        measured({ width: 412, height: 800, mobile: true }),
      ]) {
        assert.equal(size.length, 6);
        assert.ok(
          size.every(n => Number.isFinite(n) && n > 0),
          JSON.stringify(size)
        );
        assert.deepEqual(
          size.slice(4),
          [412, 800],
          'with mobile the screen reports the emulated size'
        );
      }

      // emulate-media
      const media = run(show('emulate-media', { colorScheme: 'dark' }), [
        evaluate('matchMedia("(prefers-color-scheme: dark)").matches'),
      ]);
      assert.equal(media.step(3).result.value, true);

      // css-styles
      const css = run(show('css-styles', { selector: '#box' }));
      const display = css
        .step(6)
        .computedStyle.find(item => item.name === 'display');
      assert.equal(display.value, 'flex');
      assert.ok(Array.isArray(css.step(7).matchedCSSRules));

      // accessibility-tree
      const ax = run(show('accessibility-tree', {}));
      assert.ok(
        ax
          .step(3)
          .nodes.some(
            node =>
              node.role?.value === 'button' && node.name?.value === 'Press me'
          )
      );

      // page-metrics
      const metrics = run(show('page-metrics', {}));
      const heap = metrics
        .step(3)
        .metrics.find(item => item.name === 'JSHeapUsedSize');
      assert.equal(typeof heap.value, 'number');
      assert.ok(heap.value > 0);
      assert.equal(typeof metrics.step(4).nodes, 'number');

      // event-listeners
      const listeners = run(show('event-listeners', { selector: '#go' }));
      assert.ok(
        listeners.step(5).listeners.some(item => item.type === 'click')
      );

      // handle-dialog: the trigger opens an alert, the flow answers it
      const dialog = run(
        show('handle-dialog', { trigger: 'alert("hi")', accept: true }),
        [evaluate('1 + 1')]
      );
      assert.ok(dialog.saved['events-dialog.jsonl'].includes('"message":"hi"'));

      // layer-tree: layers arrive as an event
      const layers = run(show('layer-tree', {}));
      assert.match(layers.saved['events-layers.jsonl'], /"layers":\[/);
    } finally {
      cli('cleanup', { port });
      web.kill();
      rmSync(cwd, { recursive: true, force: true });
    }
  }
);
