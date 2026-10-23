import assert from 'node:assert/strict';
import { copyFileSync, readdirSync, realpathSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath, pathToFileURL } from 'node:url';

const scripts = resolve(dirname(fileURLToPath(import.meta.url)), '..');

function fixture(failedChecks) {
  // Spaces and quotes catch unsafe interpolation into the generated module.
  const root = realpathSync(mkdtempSync(join(tmpdir(), "octocode-cdp 'runner ")));
  const lib = join(root, 'skills/octocode-scraping/scripts/lib');
  const bin = join(root, 'packages/octocode-chrome-devtools/bin');
  mkdirSync(lib, { recursive: true });
  mkdirSync(bin, { recursive: true });
  mkdirSync(join(root, 'node_modules'));
  for (const file of ['client.mjs', 'providers.mjs', 'net-guard.mjs']) copyFileSync(join(scripts, 'lib', file), join(lib, file));
  // Only the Chrome package's public executable is a fixture. The client generates and
  // executes its real runner through real child processes; the fake executable plays the
  // package's `/raw open|script|cleanup` contract, including the --stealth gate.
  mkdirSync(join(root, '.octocode/octocode-chrome-devtools/tmp/helpers'), { recursive: true });
  writeFileSync(join(root, '.octocode/octocode-chrome-devtools/tmp/helpers/dom-actionability.mjs'), 'export async function waitForPageReady() { return true; }\n');
  const workspace = join(root, 'caller-workspace');
  mkdirSync(workspace);
  const tracePath = join(root, 'trace.json');
  const commandsPath = join(root, 'commands.json');
  writeFileSync(join(bin, 'octocode-chrome-devtools.mjs'), `
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
const [, , mode, command, ...args] = process.argv;
if (process.cwd() !== ${JSON.stringify(workspace)}) throw new Error('Chrome escaped the caller workspace');
const log = ${JSON.stringify(commandsPath)};
writeFileSync(log, JSON.stringify([...(existsSync(log) ? JSON.parse(readFileSync(log, 'utf8')) : []), [mode, command, ...args]]));
if (command === 'open') { console.log(JSON.stringify({ status: 'BROWSER_READY', reused: false })); process.exit(0); }
if (command === 'cleanup') { console.log(JSON.stringify({ status: 'CLEANED_UP' })); process.exit(0); }
// The real sandbox points TMPDIR at .octocode/octocode-chrome-devtools/tmp/tmp, the only place the runner may write.
process.env.TMPDIR = ${JSON.stringify(join(workspace, '.octocode/octocode-chrome-devtools/tmp/tmp'))};
mkdirSync(process.env.TMPDIR, { recursive: true });
const { run } = await import(pathToFileURL(args[0]));
const listeners = new Map();
const calls = [];
const cdp = {
  on(name, callback) { listeners.set(name, callback); },
  async send(method, params) {
    calls.push(method);
    if (method === 'Page.navigate') {
      // A page subresource aimed at cloud metadata must be failed, the public document continued.
      await listeners.get('Fetch.requestPaused')?.({ requestId: 'r1', request: { url: 'http://169.254.169.254/latest/meta-data/' } });
      await listeners.get('Fetch.requestPaused')?.({ requestId: 'r2', request: { url: 'data:text/plain,ok' } });
      listeners.get('Network.responseReceived')?.({ type: 'Document', frameId: 'main', response: { status: 200 } });
    }
    if (method === 'Page.getFrameTree') return { frameTree: { frame: { id: 'main' } } };
    return method === 'Runtime.evaluate' ? { result: { value: '<html>verified CDP fixture</html>' } } : {};
  },
};
try {
  if (args.includes('--stealth')) {
    calls.push('stealth.apply', 'stealth.verify');
    if (${failedChecks}) throw new Error('[STEALTH_GATE] ${failedChecks}/12 stealth checks failed');
  }
  await run(cdp);
} catch (error) { console.error('[CDP_RUNNER] ' + error.message); process.exitCode = 1; }
finally { writeFileSync(${JSON.stringify(tracePath)}, JSON.stringify(calls)); }
`);
  mkdirSync(join(workspace, '.octocode/octocode-chrome-devtools/tmp/helpers'), { recursive: true });
  copyFileSync(join(root, '.octocode/octocode-chrome-devtools/tmp/helpers/dom-actionability.mjs'), join(workspace, '.octocode/octocode-chrome-devtools/tmp/helpers/dom-actionability.mjs'));
  return { root, workspace, lib, tracePath, commandsPath };
}

test('explicit CDP provider executes the generated runner through navigation and body capture', async () => {
  const f = fixture(0), cwd = process.cwd();
  try {
    process.chdir(f.workspace);
    const { resolveProvider } = await import(pathToFileURL(join(f.lib, 'providers.mjs')));
    const provider = resolveProvider('cdp');
    assert.equal(provider.name, 'cdp');
    const result = await provider.fetch({ url: 'https://fixture.invalid/', pageId: 'success', config: { cdpWaitMs: 100 } });
    assert.equal(result.fetchError, null);
    assert.equal(result.status, 200);
    assert.equal(result.body, '<html>verified CDP fixture</html>');
    assert.deepEqual(JSON.parse(readFileSync(f.tracePath, 'utf8')), [
      'Fetch.enable', 'Page.enable', 'Network.enable', 'Page.getFrameTree', 'Page.navigate', 'Fetch.failRequest', 'Fetch.continueRequest', 'Runtime.evaluate',
    ]);
    assert.deepEqual(result.refusedRequests, ['http://169.254.169.254/latest/meta-data/']);
    await provider.cleanup({});
    const commands = JSON.parse(readFileSync(f.commandsPath, 'utf8'));
    assert.deepEqual(commands.map(([mode, command]) => [mode, command]), [['/raw', 'open'], ['/raw', 'script'], ['/raw', 'cleanup']]);
    assert(!commands[1].includes('--stealth'));
    assert.deepEqual(readdirSync(join(f.workspace, '.octocode/octocode-scraping/tmp/cdp-provider')), []);
  } finally { process.chdir(cwd); rmSync(f.root, { recursive: true, force: true }); }
});

test('explicit stealth failure prevents navigation and never returns a successful body', async () => {
  const f = fixture(1), cwd = process.cwd();
  try {
    process.chdir(f.workspace);
    const { fetchCdp } = await import(pathToFileURL(join(f.lib, 'client.mjs')));
    const result = await fetchCdp({ url: 'https://fixture.invalid/', pageId: 'blocked', config: { cdpWaitMs: 100, cdpStealth: true } });
    assert.match(result.fetchError, /\[STEALTH_GATE\] 1\/12 stealth checks failed/);
    assert.equal(result.status, 0);
    assert.equal(result.body, '');
    assert.deepEqual(JSON.parse(readFileSync(f.tracePath, 'utf8')), [
      'stealth.apply', 'stealth.verify',
    ]);
    assert(JSON.parse(readFileSync(f.commandsPath, 'utf8'))[1].includes('--stealth'));
    assert.equal(existsSync(join(f.root, '.octocode/octocode-scraping/tmp/cdp-provider/blocked-runner.mjs')), false);
    assert.equal(existsSync(join(f.root, '.octocode/octocode-scraping/tmp/cdp-provider/blocked-body.html')), false);
  } finally { process.chdir(cwd); rmSync(f.root, { recursive: true, force: true }); }
});
