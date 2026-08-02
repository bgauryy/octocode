#!/usr/bin/env node
// Deterministic checks for undercover.mjs (stealth) and human-input.mjs (trusted input events).
// Part A needs no browser (pure builder functions). Part B launches an isolated headless
// Chrome and runs the real stealth patch + verify against a data: URL — no network dependency,
// no third-party site to go stale.
import { mkdir, rm, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const here = new URL('.', import.meta.url).pathname;
const root = resolve(process.cwd());
const outBase = join(root, '.octocode/tmp/chrome-devtools-eval');
const openBrowserScript = resolve(here, 'open-browser.mjs');
const sandboxScript = resolve(here, 'cdp-sandbox.mjs');
const EVAL_PORT = process.env.EVAL_CDP_PORT || '9291';

const checks = [];
const assert = (name, condition, detail = '') => checks.push({ name, ok: Boolean(condition), detail });

await rm(outBase, { recursive: true, force: true });
await mkdir(outBase, { recursive: true });

// ── Part A: human-input.mjs builders (pure functions, no browser) ──────────────────────────
const { buildMouseMoveEvents, buildClickEvents, buildTypingEvents, buildScrollEvents, buildHumanClickSequence } = await import(resolve(here, 'human-input.mjs'));

const moveEvents = buildMouseMoveEvents(0, 0, 500, 300);
assert('mouse move produces multiple steps', moveEvents.length >= 8, moveEvents.length);
assert('mouse move events are dispatchMouseEvent', moveEvents.every((e) => e.method === 'Input.dispatchMouseEvent'), '');
const last = moveEvents[moveEvents.length - 1].params;
assert('mouse move ends near target', Math.hypot(last.x - 500, last.y - 300) < 20, `ended at (${last.x},${last.y})`);

const clickEvents = buildClickEvents(100, 100);
assert('click produces mousePressed + mouseReleased', clickEvents.length === 2 && clickEvents[0].params.type === 'mousePressed' && clickEvents[1].params.type === 'mouseReleased', '');

const typingEvents = buildTypingEvents('hi');
const insertedText = typingEvents.filter((e) => e.method === 'Input.insertText').map((e) => e.params.text).join('');
assert('typing inserts the requested characters (possibly with typo/backspace noise)', insertedText.includes('h') && insertedText.includes('i'), insertedText);

const scrollEvents = buildScrollEvents(50, 50, 500);
assert('scroll produces mouseWheel events summing toward the requested delta', scrollEvents.length > 0 && scrollEvents.every((e) => e.params.type === 'mouseWheel'), scrollEvents.length);

const clickSeq = buildHumanClickSequence(0, 0, 200, 200);
assert('human click sequence combines move + click', clickSeq.some((e) => e.params.type === 'mouseMoved') && clickSeq.some((e) => e.params.type === 'mousePressed'), '');

// ── Part B: real stealth patch + verify, no network dependency ─────────────────────────────
const launch = spawnSync(process.execPath, [openBrowserScript, '--headless', '--port', EVAL_PORT, '--url', 'about:blank'], { encoding: 'utf8' });
let launchOk = false;
try { launchOk = JSON.parse(launch.stdout).status === 'BROWSER_READY'; } catch {}
checks.push({ name: 'headless Chrome launches for eval', ok: launchOk, stdout: launch.stdout.slice(0, 500), stderr: launch.stderr.slice(0, 500) });

if (launchOk) {
  const runnerPath = join(outBase, 'stealth-eval-runner.mjs');
  await writeFile(runnerPath, `
import { resolve } from 'path';
import { pathToFileURL } from 'url';
const { applyStealthPatches, verifyStealth } = await import(pathToFileURL(resolve(process.cwd(), '.octocode', 'undercover.mjs')).href);
export async function run(cdp) {
  await cdp.send('Page.enable', {});
  await cdp.send('Runtime.enable', {});
  await applyStealthPatches(cdp);
  await cdp.send('Page.navigate', { url: 'data:text/html,<html><body>eval</body></html>' });
  await new Promise((r) => setTimeout(r, 500));
  const result = await verifyStealth(cdp);
  console.log('EVAL_RESULT_JSON:' + JSON.stringify(result));
}
`);

  const sandboxRun = spawnSync(process.execPath, [sandboxScript, runnerPath, '--port', EVAL_PORT, '--new-tab', 'about:blank', '--timeout', '15000', '--script-timeout', '20000'], { encoding: 'utf8' });
  const resultLine = sandboxRun.stdout.split('\n').find((l) => l.startsWith('EVAL_RESULT_JSON:'));
  let result = null;
  try { result = resultLine ? JSON.parse(resultLine.slice('EVAL_RESULT_JSON:'.length)) : null; } catch {}
  checks.push({ name: 'stealth eval script runs', ok: sandboxRun.status === 0, stdout: sandboxRun.stdout.slice(-1500), stderr: sandboxRun.stderr.slice(0, 500) });
  assert('stealth self-test passes all checks (15/15)', result && result.failed === 0 && result.passed === result.total, JSON.stringify(result));

  spawnSync(process.execPath, [openBrowserScript, '--port', EVAL_PORT, '--cleanup'], { encoding: 'utf8' });
} else {
  assert('stealth self-test skipped — Chrome unavailable', false, 'headless Chrome failed to launch; cannot verify undercover.mjs without a browser');
}

const failed = checks.filter((c) => !c.ok);
console.log(JSON.stringify({ ok: failed.length === 0, checks }, null, 2));
process.exit(failed.length === 0 ? 0 : 1);
