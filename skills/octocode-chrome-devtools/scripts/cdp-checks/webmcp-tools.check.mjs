#!/usr/bin/env node
// TDD grader for the webmcp intent (scripts/cdp-checks/webmcp-tools.mjs).
// Deterministic, no LLM judgment: launches an isolated headless Chrome with
// WebMCP enabled, points it at the fixture in fixtures/webmcp-fixture.html,
// and asserts exact prefixed stdout lines the intent script must produce.
//
// Usage: node webmcp-tools.check.mjs [--port 9245]
//
// Exit 0 = all assertions passed. Exit 1 = red (missing script or assertion
// failure) — expected before scripts/cdp-checks/webmcp-tools.mjs exists.

import { spawnSync } from 'child_process';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';

const __dir = dirname(fileURLToPath(import.meta.url));
const SKILL_DIR = resolve(__dir, '../..');
const OPEN_BROWSER = join(SKILL_DIR, 'scripts', 'open-browser.mjs');
const SANDBOX = join(SKILL_DIR, 'scripts', 'cdp-sandbox.mjs');
const TARGET_SCRIPT = join(__dir, 'webmcp-tools.mjs');
const FIXTURE_URL = `file://${join(__dir, 'fixtures', 'webmcp-fixture.html')}`;

const argv = process.argv.slice(2);
const getArg = (flag, def) => { const i = argv.indexOf(flag); return i !== -1 && argv[i + 1] ? argv[i + 1] : def; };
const PORT = getArg('--port', '9245');

const results = [];
function check(name, cond, detail) {
  results.push({ name, pass: Boolean(cond), detail });
  console.log(`${cond ? '[PASS]' : '[FAIL]'} ${name}${detail ? ` — ${detail}` : ''}`);
}

function run(cmd, args, opts = {}) {
  const res = spawnSync(cmd, args, { encoding: 'utf8', ...opts });
  return { stdout: res.stdout ?? '', stderr: res.stderr ?? '', status: res.status };
}

function cleanup() {
  run(process.execPath, [OPEN_BROWSER, '--port', PORT, '--cleanup']);
}

console.log(`[CHECK] Using port ${PORT}, fixture ${FIXTURE_URL}`);
cleanup();

const launch = run(process.execPath, [
  OPEN_BROWSER, '--headless', '--port', PORT, '--enableFeatures', 'WebMCP', '--url', FIXTURE_URL,
]);
let launchInfo = {};
try { launchInfo = JSON.parse(launch.stdout.trim().split('\n').pop()); } catch {}
// status === 'BROWSER_READY' alone isn't enough: open-browser.mjs returns that
// same status when it silently reuses an unrelated already-running Chrome on
// this port, in which case --enableFeatures never took effect. Reusing a port
// occupied by something else must fail loudly here, not pass and misattribute
// downstream [FAIL]s to a broken webmcp intent.
const launchedFreshWithFlag = launchInfo.status === 'BROWSER_READY'
  && launchInfo.reused === false
  && launchInfo.enableFeaturesConfigured === 'WebMCP';
check('browser launches fresh with WebMCP feature flag (not a reused session)', launchedFreshWithFlag, JSON.stringify(launchInfo));

if (launchedFreshWithFlag) {
  const list = run(process.execPath, [
    SANDBOX, TARGET_SCRIPT, '--port', PORT, '--target-url', 'webmcp-fixture.html', '--keep-tab',
  ], { env: { ...process.env, WEBMCP_ACTION: 'list' } });
  check('list mode exits 0', list.status === 0, `exit=${list.status} stderr=${list.stderr.slice(0, 300)}`);
  check('list mode discovers fixture tool', /\[WEBMCP_TOOL\][^\n]*name=echo_price/.test(list.stdout), list.stdout.slice(0, 500));

  const invoke = run(process.execPath, [
    SANDBOX, TARGET_SCRIPT, '--port', PORT, '--target-url', 'webmcp-fixture.html', '--keep-tab',
  ], { env: { ...process.env, WEBMCP_ACTION: 'invoke', WEBMCP_TOOL: 'echo_price', WEBMCP_INPUT: '{"name":"widget","price":21}' } });
  check('invoke mode exits 0', invoke.status === 0, `exit=${invoke.status} stderr=${invoke.stderr.slice(0, 300)}`);
  check('invoke mode returns structured output', /"echoed":"widget"/.test(invoke.stdout) && /"doubled":42/.test(invoke.stdout), invoke.stdout.slice(0, 500));
  check('invoke mode reports Completed status', /\[WEBMCP_RESULT\][^\n]*status=Completed/.test(invoke.stdout), invoke.stdout.slice(0, 500));

  const noTools = run(process.execPath, [
    SANDBOX, TARGET_SCRIPT, '--port', PORT, '--new-tab', 'about:blank',
  ], { env: { ...process.env, WEBMCP_ACTION: 'list', WEBMCP_WAIT_MS: '1500' } });
  check('no-tools page exits 0 (no hang, no crash)', noTools.status === 0, `exit=${noTools.status} stderr=${noTools.stderr.slice(0, 300)}`);
  check('no-tools page reports empty result, not an error', /\[FINDING\] WEBMCP_NO_TOOLS/.test(noTools.stdout), noTools.stdout.slice(0, 500));
}

cleanup();

const failed = results.filter(r => !r.pass);
console.log(`\n[CHECK] ${results.length - failed.length}/${results.length} passed`);
process.exit(failed.length ? 1 : 0);
