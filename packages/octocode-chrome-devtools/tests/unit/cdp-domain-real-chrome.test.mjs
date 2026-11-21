// cdp-domain on a real isolated headless Chrome: dumps (small results together, large ones in their
// own file), events, hazards never sent, the policy skip, the unknown-domain refusal, and the env keys
// through the MCP `check` sandbox. The fixture site is a loopback child process. Skipped without Chrome.
import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { execFile, spawn } from 'node:child_process';
import { promisify } from 'node:util';
import { mkdtempSync, readFileSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  chromeBinary,
  startThrowawayChrome,
} from '../fixtures/throwaway-chrome.mjs';
import { CDP_DOMAIN_HAZARDS } from '../../dist/engine/cdp-checks/cdp-domain-hazards.mjs';
import { skip as noChrome, withSite } from './tool-depth-fixture.mjs';

const skip = !chromeBinary && 'no Chrome installed';
const dist = join(import.meta.dirname, '../../dist/engine');

const RUNNER = `
import { connectCDP } from ${JSON.stringify(join(dist, 'cdp-connection.mjs'))};
const targets = await (await fetch('http://127.0.0.1:' + process.env.PORT + '/json')).json();
const target = targets.find(t => t.type === 'page');
const cdp = await connectCDP(target.webSocketDebuggerUrl, { targetInfo: target });
cdp.outputDir = process.env.OUT;
const recipe = await import(${JSON.stringify(join(dist, 'cdp-checks/cdp-domain-check.mjs'))});
await recipe.run(cdp);
cdp.close();
process.exit(process.exitCode ?? 0);
`;

const page = `<!doctype html><title>domain</title>
<style>@keyframes slide { from { margin-left: 0 } to { margin-left: 40px } } #box { animation: slide 1s infinite }</style>
<div id=box>box</div>
<script>
  try { window.keep = new AudioContext(); } catch {}
  fetch('/api');
</script>`;
const serverSource = `
require('node:http').createServer((req, res) => {
  if (req.url === '/api') { res.setHeader('content-type', 'text/plain'); return res.end('ok'); }
  res.setHeader('content-type', 'text/html');
  res.end(${JSON.stringify(page)});
}).listen(0, '127.0.0.1', function () { console.log(this.address().port); });`;

describe('cdp-domain recipe', { skip, timeout: 240000 }, () => {
  let chrome, web, base, work;
  before(async () => {
    chrome = await startThrowawayChrome();
    web = spawn(process.execPath, ['-e', serverSource], {
      stdio: ['ignore', 'pipe', 'inherit'],
    });
    base = `http://127.0.0.1:${await new Promise(resolve =>
      web.stdout.once('data', chunk => resolve(String(chunk).trim()))
    )}/`;
    work = mkdtempSync(join(tmpdir(), 'octo-cdp-domain-'));
  });
  after(async () => {
    web?.kill();
    await chrome?.stop();
    if (work) rmSync(work, { recursive: true, force: true });
  });

  /** Runs the recipe in a child process (it reads its options from the environment). */
  const record = async env => {
    const out = mkdtempSync(join(work, 'out-'));
    const r = await promisify(execFile)(
      process.execPath,
      ['--input-type=module', '-e', RUNNER],
      {
        encoding: 'utf8',
        env: {
          ...process.env,
          PORT: String(chrome.port),
          OUT: out,
          CDPDOM_DOMAIN: '',
          CDPDOM_MODE: '',
          CDPDOM_MS: '',
          CDPDOM_METHODS: '',
          CDPDOM_URL: '',
          ...env,
        },
      }
    ).catch(error => error);
    const stdout = `${r.stdout ?? ''}${r.stderr ?? ''}`;
    const read = name => JSON.parse(readFileSync(join(out, name), 'utf8'));
    const name = env.CDPDOM_DOMAIN;
    const index = () => read(`domain-${name}-index.json`);
    const row = method => index().find(item => item.method === method);
    const events = () =>
      readFileSync(join(out, `domain-${name}.jsonl`), 'utf8')
        .trim()
        .split('\n')
        .filter(Boolean)
        .map(line => JSON.parse(line));
    return { stdout, out, code: r.code ?? 0, read, index, row, events };
  };

  test('Performance: the domain is enabled first, so the metrics are not empty', async () => {
    const run = await record({
      CDPDOM_DOMAIN: 'Performance',
      CDPDOM_MODE: 'dump',
    });
    assert.equal(run.code, 0, run.stdout);
    const metrics = run.read('domain-Performance.json')[
      'Performance.getMetrics'
    ].metrics;
    assert.ok(metrics.length > 0);
    assert.ok(metrics.some(metric => metric.name === 'JSHeapUsedSize'));
    assert.equal(run.row('Performance.getMetrics').status, 'ok');
    assert.match(
      run.stdout,
      /\[METRIC\] CDP_DOMAIN domain=Performance methods=1 ok=1 skipped=0 events=0/
    );
  });

  test('Memory: counters are saved, the leak-detection hazards are skipped and the page stays alive', async () => {
    const run = await record({ CDPDOM_DOMAIN: 'Memory', CDPDOM_MODE: 'dump' });
    assert.equal(run.code, 0, run.stdout);
    assert.equal(run.row('Memory.getDOMCounters').status, 'ok');
    const hazard = run.row('Memory.getDOMCountersForLeakDetection');
    assert.equal(hazard.status, 'skipped');
    assert.match(hazard.reason, /^hazard: /);
    assert.ok('Memory.getDOMCounters' in run.read('domain-Memory.json'));
    // Chrome still answers after the run.
    const version = await (
      await fetch(`http://127.0.0.1:${chrome.port}/json/version`)
    ).json();
    assert.ok(version.Browser);
  });

  test('Browser: the 200+ KB histograms get their own file whose size the index states; a needsAck getter is skipped', async () => {
    const run = await record({ CDPDOM_DOMAIN: 'Browser', CDPDOM_MODE: 'dump' });
    assert.equal(run.code, 0, run.stdout);
    const histograms = run.row('Browser.getHistograms');
    assert.equal(histograms.status, 'ok');
    assert.ok(histograms.bytes > 64 * 1024);
    assert.equal(histograms.file, 'domain-Browser/getHistograms.json');
    assert.equal(
      statSync(join(run.out, histograms.file)).size,
      histograms.bytes
    );
    JSON.parse(readFileSync(join(run.out, histograms.file), 'utf8'));
    const small = run.read('domain-Browser.json');
    assert.ok('Browser.getVersion' in small);
    assert.ok(
      !('Browser.getHistograms' in small),
      'large results stay out of the aggregate'
    );
    const row = run.row('Browser.getBrowserCommandLine');
    assert.equal(row.status, 'skipped');
    assert.match(row.reason, /^needsAck: /);
    assert.equal(row.bytes, 0);
    // A failing getter is a row and a finding with Chrome's text, never fatal.
    assert.match(
      run.stdout,
      /\[FINDING\] CDP_DOMAIN_METHOD_FAILED .*getGlobalPrivacyControl/
    );
    // stdout never carries a result.
    assert.ok(Buffer.byteLength(run.stdout) < 4000);
    assert.doesNotMatch(run.stdout, /"buckets"/);
    assert.match(run.stdout, /\[ARTIFACT\] CDP_DOMAIN_FILES /);
  });

  test('SystemInfo: browser-level getters run through scope routing from a page session', async () => {
    const run = await record({
      CDPDOM_DOMAIN: 'SystemInfo',
      CDPDOM_MODE: 'dump',
    });
    assert.equal(run.code, 0, run.stdout);
    const info = run.row('SystemInfo.getInfo');
    assert.equal(info.status, 'ok');
    assert.equal(info.scope, 'browser');
    assert.ok(
      'gpu' in run.read('domain-SystemInfo.json')['SystemInfo.getInfo']
    );
    assert.equal(run.row('SystemInfo.getProcessInfo').status, 'ok');
  });

  test('a getter the policy gates is skipped with needsAck, never sent', async () => {
    const run = await record({ CDPDOM_DOMAIN: 'Storage', CDPDOM_MODE: 'dump' });
    assert.equal(run.code, 0, run.stdout);
    for (const method of [
      'Storage.getTrustTokens',
      'Storage.getPrivateVerificationTokens',
    ]) {
      const row = run.row(method);
      assert.equal(row.status, 'skipped', method);
      assert.match(row.reason, /^needsAck: /, method);
    }
    assert.ok(
      !('Storage.getTrustTokens' in run.read('domain-Storage.json')),
      'a skipped method has no result'
    );
  });

  test('hazards named in CDPDOM_METHODS are skipped, not sent: the run does not hang', async () => {
    const run = await record({
      CDPDOM_DOMAIN: 'Page',
      CDPDOM_MODE: 'dump',
      CDPDOM_METHODS: JSON.stringify([
        'waitForDebugger',
        'reload',
        'stopLoading',
        'startScreencast',
        'nonexistent',
        'navigate',
      ]),
    });
    assert.equal(run.code, 0, run.stdout);
    for (const name of [
      'waitForDebugger',
      'reload',
      'stopLoading',
      'startScreencast',
    ]) {
      const row = run.row(`Page.${name}`);
      assert.equal(row.status, 'skipped', name);
      assert.match(row.reason, /^hazard: /, name);
    }
    assert.match(run.row('Page.nonexistent').reason, /not a command/);
    assert.match(run.row('Page.navigate').reason, /required parameters/);
    const storage = await record({
      CDPDOM_DOMAIN: 'Storage',
      CDPDOM_MODE: 'dump',
      CDPDOM_METHODS: JSON.stringify(['runBounceTrackingMitigations']),
    });
    assert.match(
      storage.row('Storage.runBounceTrackingMitigations').reason,
      /^hazard: /
    );
  });

  test('every hazard exists in the installed Chrome and is documented', () => {
    const live = new Set();
    for (const domain of chrome.protocol.domains)
      for (const command of domain.commands ?? [])
        live.add(`${domain.domain}.${command.name}`);
    const docs = readFileSync(
      join(import.meta.dirname, '../../docs/cdp-protocol.md'),
      'utf8'
    );
    const methods = new Set();
    for (const { method, reason } of CDP_DOMAIN_HAZARDS) {
      assert.ok(!methods.has(method), `duplicate ${method}`);
      methods.add(method);
      assert.ok(
        live.has(method),
        `${method} is not a command of ${chrome.version}`
      );
      assert.ok(reason.length > 0);
      assert.ok(
        docs.includes(`| \`${method}\` | ${reason} |`),
        `${method} row missing or stale in docs/cdp-protocol.md`
      );
    }
  });

  test('Network events with CDPDOM_URL: enabled before the load, every event whole in the jsonl', async () => {
    const run = await record({
      CDPDOM_DOMAIN: 'Network',
      CDPDOM_MODE: 'events',
      CDPDOM_MS: '1200',
      CDPDOM_URL: base,
    });
    assert.equal(run.code, 0, run.stdout);
    const events = run.events();
    assert.ok(events.length > 0);
    const sent = events.filter(
      event => event.method === 'Network.requestWillBeSent'
    );
    assert.ok(sent.length >= 1);
    for (const event of events) {
      assert.match(event.ts, /^\d{4}-\d\d-\d\dT/);
      assert.match(event.method, /^Network\./);
      assert.equal(typeof event.params, 'object');
    }
    assert.ok(sent.some(event => event.params.request.url === base));
    assert.ok(
      run.stdout.includes(
        `[EVENT] ${JSON.stringify({ event: 'Network.requestWillBeSent', count: sent.length })}`
      ),
      run.stdout
    );
    assert.match(run.stdout, new RegExp(`events=${events.length}`));
    assert.equal(
      statSync(join(run.out, 'domain-Network.jsonl')).mode & 0o777,
      0o600
    );
  });

  for (const [domain, event] of [
    ['Animation', 'Animation.animationStarted'],
    ['WebAudio', 'WebAudio.contextCreated'],
  ])
    test(`${domain} events appear after a load that triggers them`, async () => {
      const run = await record({
        CDPDOM_DOMAIN: domain,
        CDPDOM_MODE: 'events',
        CDPDOM_MS: '1500',
        CDPDOM_URL: base,
      });
      assert.equal(run.code, 0, run.stdout);
      const events = run.events();
      assert.ok(events.length >= 1, run.stdout);
      assert.ok(
        events.some(item => item.method === event),
        event
      );
    });

  test('both mode records events around the dump', async () => {
    const run = await record({
      CDPDOM_DOMAIN: 'Performance',
      CDPDOM_MODE: 'both',
      CDPDOM_MS: '400',
      CDPDOM_URL: base,
    });
    assert.equal(run.code, 0, run.stdout);
    assert.equal(run.row('Performance.getMetrics').status, 'ok');
    assert.ok(
      run.read('domain-Performance.json')['Performance.getMetrics'].metrics
        .length > 0
    );
    assert.match(
      run.stdout,
      /\[METRIC\] CDP_DOMAIN domain=Performance .*mode=both/
    );
  });

  test('an unknown domain is refused with the nearest names; bad options are refused', async () => {
    const unknown = await record({ CDPDOM_DOMAIN: 'Perfomance' });
    assert.equal(unknown.code, 1);
    const finding = /\[FINDING\] CDP_DOMAIN_UNKNOWN (.*)/.exec(unknown.stdout);
    assert.ok(finding, unknown.stdout);
    assert.ok(JSON.parse(finding[1]).nearest.includes('Performance'));
    const slow = await record({ CDPDOM_DOMAIN: 'Performance', CDPDOM_MS: '5' });
    assert.equal(slow.code, 2);
    assert.match(slow.stdout, /CDP_DOMAIN_INVALID .*CDPDOM_MS/);
    const mode = await record({
      CDPDOM_DOMAIN: 'Performance',
      CDPDOM_MODE: 'x',
    });
    assert.equal(mode.code, 2);
  });

  test('Fetch is never enabled automatically', async () => {
    const run = await record({
      CDPDOM_DOMAIN: 'Fetch',
      CDPDOM_MODE: 'events',
      CDPDOM_MS: '200',
    });
    assert.equal(run.code, 0, run.stdout);
    assert.match(run.stdout, /\[NOTE\] Fetch\.enable pauses requests/);
  });
});

test(
  'check cdp-domain forwards its env keys through the MCP sandbox',
  { skip: noChrome, timeout: 200000 },
  async () => {
    await withSite(async ({ base, connection, ok, cwd }) => {
      const stdoutOf = async reply =>
        reply.stdout ??
        readFileSync(
          JSON.parse(readFileSync(join(cwd, reply.evidence), 'utf8')).logs
            .stdout,
          'utf8'
        );
      const dump = await ok('check', {
        recipe: 'cdp-domain',
        connection,
        options: {
          CDPDOM_DOMAIN: 'Memory',
          CDPDOM_MODE: 'dump',
          CDPDOM_METHODS: ['prepareForLeakDetection'],
        },
      });
      const dumpOut = await stdoutOf(dump);
      assert.match(
        dumpOut,
        /\[METRIC\] CDP_DOMAIN domain=Memory methods=\d+ ok=[1-9]\d* skipped=[2-9] events=0/
      );
      const events = await ok('check', {
        recipe: 'cdp-domain',
        connection,
        options: {
          CDPDOM_DOMAIN: 'Network',
          CDPDOM_MODE: 'events',
          CDPDOM_MS: 800,
          CDPDOM_URL: `${base}/`,
        },
      });
      assert.match(
        await stdoutOf(events),
        /\[METRIC\] CDP_DOMAIN domain=Network .*events=[1-9]/
      );
    });
  }
);

test(
  'every policy entry names a command of the installed Chrome (removed ones are listed)',
  { skip, timeout: 60000 },
  async () => {
    const { CDP_POLICY_TABLE } = await import(join(dist, 'cdp-policy.mjs'));
    const chrome = await startThrowawayChrome();
    try {
      const live = new Set();
      for (const domain of chrome.protocol.domains)
        for (const command of domain.commands ?? [])
          live.add(`${domain.domain}.${command.name}`);
      // Removed from Chrome, kept classified for older builds.
      const removed = new Set(['Network.setRequestInterception']);
      for (const { method } of CDP_POLICY_TABLE)
        assert.ok(
          live.has(method) || removed.has(method),
          `${method} is not a command of ${chrome.version}`
        );
    } finally {
      await chrome.stop();
    }
  }
);
