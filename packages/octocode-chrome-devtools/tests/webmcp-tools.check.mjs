#!/usr/bin/env node
// Regression checks against an isolated Chrome session and local WebMCP fixture.
import { spawnSync } from 'child_process';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

const __dir = dirname(fileURLToPath(import.meta.url));
const SKILL_DIR = resolve(__dir, '..');
const OPEN_BROWSER = join(SKILL_DIR, 'dist/engine', 'open-browser.mjs');
const SANDBOX = join(SKILL_DIR, 'dist/engine', 'cdp-sandbox.mjs');
const TARGET_SCRIPT = join(
  SKILL_DIR,
  'dist/engine/cdp-checks/webmcp-tools.mjs'
);
const FIXTURE_URL = `file://${join(__dir, 'fixtures', 'webmcp-fixture.html')}`;

const argv = process.argv.slice(2);
const getArg = (flag, def) => {
  const i = argv.indexOf(flag);
  return i !== -1 && argv[i + 1] ? argv[i + 1] : def;
};
if (argv.includes('--help') || argv.includes('-h')) {
  console.log('Usage: webmcp-tools.check.mjs [--port 9245]');
  process.exit(0);
}
const PORT = getArg('--port', '9245');
const WORK = mkdtempSync(join(tmpdir(), 'octo-webmcp-'));

const results = [];
function check(name, cond, detail) {
  results.push({ name, pass: Boolean(cond), detail });
  console.log(
    `${cond ? '[PASS]' : '[FAIL]'} ${name}${detail ? ` — ${detail}` : ''}`
  );
}

function run(cmd, args, opts = {}) {
  const res = spawnSync(cmd, args, {
    cwd: WORK,
    encoding: 'utf8',
    timeout: 60000,
    ...opts,
  });
  return {
    stdout: res.stdout ?? '',
    stderr: res.stderr ?? '',
    status: res.status,
  };
}

function cleanup() {
  run(process.execPath, [OPEN_BROWSER, '--port', PORT, '--cleanup']);
}

try {
  console.log(`[CHECK] Using port ${PORT}, fixture ${FIXTURE_URL}`);
  cleanup();

  const launch = run(process.execPath, [
    OPEN_BROWSER,
    '--headless',
    '--port',
    PORT,
    '--enableFeatures',
    'WebMCP',
    '--url',
    FIXTURE_URL,
  ]);
  let launchInfo = {};
  try {
    launchInfo = JSON.parse(launch.stdout.trim().split('\n').pop());
  } catch {}
  // status === 'BROWSER_READY' alone isn't enough: open-browser.mjs returns that
  // same status when it silently reuses an unrelated already-running Chrome on
  // this port, in which case --enableFeatures never took effect. Reusing a port
  // occupied by something else must fail loudly here, not pass and misattribute
  // downstream [FAIL]s to a broken webmcp intent.
  const launchedFreshWithFlag =
    launchInfo.status === 'BROWSER_READY' &&
    launchInfo.reused === false &&
    launchInfo.enableFeaturesConfigured === 'WebMCP';
  check(
    'browser launches fresh with WebMCP feature flag (not a reused session)',
    launchedFreshWithFlag,
    JSON.stringify(launchInfo)
  );

  if (launchedFreshWithFlag) {
    const list = run(
      process.execPath,
      [
        SANDBOX,
        TARGET_SCRIPT,
        '--port',
        PORT,
        '--target-url',
        'webmcp-fixture.html',
        '--keep-tab',
      ],
      { env: { ...process.env, WEBMCP_ACTION: 'list' } }
    );
    check(
      'list mode exits 0',
      list.status === 0,
      `exit=${list.status} stderr=${list.stderr.slice(0, 300)}`
    );
    check(
      'list mode discovers fixture tool',
      /\[WEBMCP_TOOL\][^\n]*name=echo_price/.test(list.stdout),
      list.stdout.slice(0, 500)
    );

    const invoke = run(
      process.execPath,
      [
        SANDBOX,
        TARGET_SCRIPT,
        '--port',
        PORT,
        '--target-url',
        'webmcp-fixture.html',
        '--keep-tab',
      ],
      {
        env: {
          ...process.env,
          WEBMCP_ACTION: 'invoke',
          WEBMCP_TOOL: 'echo_price',
          WEBMCP_INPUT: '{"name":"widget","price":21}',
        },
      }
    );
    check(
      'invoke mode exits 0',
      invoke.status === 0,
      `exit=${invoke.status} stderr=${invoke.stderr.slice(0, 300)}`
    );
    check(
      'invoke mode returns structured output',
      /"echoed":"widget"/.test(invoke.stdout) &&
        /"doubled":42/.test(invoke.stdout),
      invoke.stdout.slice(0, 500)
    );
    check(
      'invoke mode reports Completed status',
      /\[WEBMCP_RESULT\][^\n]*status=Completed/.test(invoke.stdout),
      invoke.stdout.slice(0, 500)
    );

    const noTools = run(
      process.execPath,
      [SANDBOX, TARGET_SCRIPT, '--port', PORT, '--new-tab', 'about:blank'],
      { env: { ...process.env, WEBMCP_ACTION: 'list', WEBMCP_WAIT_MS: '1500' } }
    );
    check(
      'no-tools page exits 0 (no hang, no crash)',
      noTools.status === 0,
      `exit=${noTools.status} stderr=${noTools.stderr.slice(0, 300)}`
    );
    check(
      'no-tools page reports empty result, not an error',
      /\[FINDING\] WEBMCP_NO_TOOLS/.test(noTools.stdout),
      noTools.stdout.slice(0, 500)
    );
  }
} finally {
  cleanup();
  rmSync(WORK, { recursive: true, force: true });
}

const failed = results.filter(r => !r.pass);
console.log(
  `\n[CHECK] ${results.length - failed.length}/${results.length} passed`
);
process.exit(failed.length ? 1 : 0);
