import assert from 'node:assert/strict';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath, pathToFileURL } from 'node:url';

const scripts = resolve(dirname(fileURLToPath(import.meta.url)), '..');

function fixture(failedChecks) {
  // Spaces and quotes catch unsafe interpolation into the generated module.
  const root = mkdtempSync(join(tmpdir(), "octocode-cdp 'runner "));
  const lib = join(root, 'skills/octocode-scraping/scripts/lib');
  const chrome = join(root, 'packages/octocode-chrome-devtools/scripts');
  mkdirSync(lib, { recursive: true });
  mkdirSync(chrome, { recursive: true });
  mkdirSync(join(root, 'node_modules'));
  for (const file of ['client.mjs', 'providers.mjs']) copyFileSync(join(scripts, 'lib', file), join(lib, file));
  // Only browser/CDP boundaries are fixtures. The client generates and executes
  // its real runner through real child processes, with native defaults.
  writeFileSync(join(chrome, 'open-browser.mjs'), 'console.log(JSON.stringify({ status: "BROWSER_READY" }));');
  const stealth = `
export async function applyStealthPatches(cdp) { cdp.calls.push('stealth.apply'); }
export async function verifyStealth(cdp) {
  cdp.calls.push('stealth.verify');
  return { passed: ${failedChecks ? 0 : 1}, failed: ${failedChecks}, total: 1 };
}
`;
  writeFileSync(join(chrome, 'undercover.mjs'), stealth);
  writeFileSync(join(chrome, 'dom-actionability.mjs'), 'export async function waitForPageReady() { return true; }\n');
  mkdirSync(join(root, '.octocode'));
  copyFileSync(join(chrome, 'dom-actionability.mjs'), join(root, '.octocode/dom-actionability.mjs'));
  const tracePath = join(root, 'trace.json');
  writeFileSync(join(chrome, 'cdp-sandbox.mjs'), `
import { writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
const { run } = await import(pathToFileURL(process.argv[2]));
const listeners = new Map();
const calls = [];
const cdp = {
  calls,
  on(name, callback) { listeners.set(name, callback); },
  async send(method, params) {
    calls.push(method);
    if (method === 'Page.navigate') {

      listeners.get('Network.responseReceived')?.({ type: 'Document', frameId: 'main', response: { status: 200 } });
    }
    if (method === 'Page.getFrameTree') return { frameTree: { frame: { id: 'main' } } };
    if (method === 'Runtime.evaluate' && params.expression.includes('document.readyState')) return {result:{value:{ready:'complete',blank:false,content:true}}};
    return method === 'Runtime.evaluate' ? { result: { value: '<html>verified CDP fixture</html>' } } : {};
  },
};
try { await run(cdp); }
catch (error) { console.error(error.message); process.exitCode = 1; }
finally { writeFileSync(${JSON.stringify(tracePath)}, JSON.stringify(calls)); }
`);
  return { root, lib, tracePath };
}

test('explicit CDP provider executes the generated runner through navigation and body capture', async () => {
  const f = fixture(0);
  try {
    const { resolveProvider } = await import(pathToFileURL(join(f.lib, 'providers.mjs')));
    const provider = resolveProvider('cdp');
    assert.equal(provider.name, 'cdp');
    const result = await provider.fetch({ url: 'https://fixture.invalid/', pageId: 'success', config: { cdpWaitMs: 100 } });
    assert.equal(result.fetchError, null);
    assert.equal(result.status, 200);
    assert.equal(result.body, '<html>verified CDP fixture</html>');
    assert.equal(existsSync(join(f.root, '.octocode/undercover.mjs')), false);
    assert.deepEqual(JSON.parse(readFileSync(f.tracePath, 'utf8')), [
      'Page.enable', 'Network.enable', 'Page.getFrameTree', 'Page.navigate', 'Runtime.evaluate',
    ]);
    assert.equal(existsSync(join(f.root, '.octocode/tmp/cdp-provider/success-runner.mjs')), false);
    assert.equal(existsSync(join(f.root, '.octocode/tmp/cdp-provider/success-body.html')), false);
  } finally { rmSync(f.root, { recursive: true, force: true }); }
});

test('explicit stealth failure prevents navigation and never returns a successful body', async () => {
  const f = fixture(1);
  try {
    const { fetchCdp } = await import(pathToFileURL(join(f.lib, 'client.mjs')));
    const result = await fetchCdp({ url: 'https://fixture.invalid/', pageId: 'blocked', config: { cdpWaitMs: 100, cdpStealth: true } });
    assert.match(result.fetchError, /\[STEALTH_GATE\].*1 stealth checks failed/);
    assert.equal(result.status, 0);
    assert.equal(result.body, '');
    assert.deepEqual(JSON.parse(readFileSync(f.tracePath, 'utf8')), [
      'Page.enable', 'Network.enable', 'Page.getFrameTree', 'stealth.apply', 'stealth.verify',
    ]);
    assert.equal(existsSync(join(f.root, '.octocode/tmp/cdp-provider/blocked-runner.mjs')), false);
    assert.equal(existsSync(join(f.root, '.octocode/tmp/cdp-provider/blocked-body.html')), false);
  } finally { rmSync(f.root, { recursive: true, force: true }); }
});
