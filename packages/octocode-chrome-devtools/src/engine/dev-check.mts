#!/usr/bin/env node
// dev-check: launch or reuse an isolated Chrome, load one URL in a new tab under
// cdp-checks/dev-server-check, and print its report as one JSON document.
import { spawn } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { fail, optionValue, parse } from './cli-options.mjs';
import { errorMessage } from './error-message.mjs';

const scripts = dirname(fileURLToPath(import.meta.url));
const USAGE =
  'Usage: dev-check --url <http(s) URL> [--port 9222] [--visible] [--wait-until load|networkIdle] [--idle-ms 500] [--timeout-ms 30000] [--full-page] [--keep-tab]\nLaunches or reuses an isolated Chrome (never a real profile) and reports console errors and warnings with source-mapped stacks, failed and 4xx/5xx requests, and a screenshot.';

function child(file: string, args: string[], env: Record<string, string>) {
  return new Promise<{ code: number; stdout: string }>(resolve => {
    const engine = spawn(process.execPath, [join(scripts, file), ...args], {
      stdio: ['ignore', 'pipe', 'inherit'],
      env: { ...process.env, ...env },
    });
    const chunks: Buffer[] = [];
    engine.stdout.on('data', chunk => chunks.push(chunk));
    engine.once('error', error => {
      process.stderr.write(`[DEV_CHECK] ${error.message}\n`);
      resolve({ code: 1, stdout: '' });
    });
    engine.once('close', code =>
      resolve({ code: code ?? 1, stdout: Buffer.concat(chunks).toString() })
    );
  });
}

const whole = (text: string | undefined, name: string, min: number) => {
  if (text === undefined) return undefined;
  const value = Number(text);
  if (!Number.isSafeInteger(value) || value < min) fail(`Invalid ${name}`);
  return value;
};

/** Validated dev-check flags. */
function options(argv: string[]) {
  const { opts, positional } = parse(
    argv,
    ['--url', '--port', '--wait-until', '--idle-ms', '--timeout-ms'],
    ['--visible', '--full-page', '--keep-tab']
  );
  if (positional.length) fail('Unexpected positional argument');
  const url = optionValue(opts, '--url') ?? '';
  if (!/^https?:\/\/\S+$/.test(url)) fail('dev-check needs --url http(s)://…');
  const port = String(whole(optionValue(opts, '--port'), '--port', 1) ?? 9222);
  const waitUntil = optionValue(opts, '--wait-until') ?? 'load';
  if (!['load', 'networkIdle'].includes(waitUntil))
    fail('--wait-until needs load or networkIdle');
  const timeoutMs = whole(optionValue(opts, '--timeout-ms'), '--timeout-ms', 1);
  const idleMs = whole(optionValue(opts, '--idle-ms'), '--idle-ms', 0);
  return { opts, url, port, waitUntil, timeoutMs, idleMs };
}

/** Launches or reuses the browser on `port`; null after printing why it is unusable. */
async function isolatedBrowser(port: string, visible: boolean) {
  const opened = await child(
    'open-browser.mjs',
    ['--port', port, ...(visible ? [] : ['--headless'])],
    {}
  );
  const line = opened.stdout.split('\n').findLast(row => row.startsWith('{'));
  const browser = line ? JSON.parse(line) : null;
  if (opened.code !== 0 || browser?.status !== 'BROWSER_READY') {
    process.stdout.write(opened.stdout);
    return null;
  }
  if (browser.isolated !== true) {
    process.stderr.write(
      `[DEV_CHECK] Port ${port} serves a browser without an isolated session; use another port.\n`
    );
    return null;
  }
  return browser;
}

async function main(argv: string[]) {
  if (argv.includes('--help') || argv.includes('-h')) {
    process.stdout.write(USAGE + '\n');
    return 0;
  }
  const { opts, url, port, waitUntil, timeoutMs, idleMs } = options(argv);
  const browser = await isolatedBrowser(port, Boolean(opts['--visible']));
  if (!browser) return 1;
  const ceiling = timeoutMs ?? 30000;
  const checked = await child(
    'cdp-sandbox.mjs',
    [
      join(scripts, 'cdp-checks/dev-server-check.mjs'),
      '--port',
      port,
      '--new-tab',
      'about:blank',
      opts['--keep-tab'] ? '--keep-tab' : '--close-tab',
      '--script-timeout',
      String(Math.max(300000, ceiling + 60000)),
    ],
    {
      DEV_URL: url,
      DEV_WAIT_UNTIL: waitUntil,
      DEV_TIMEOUT_MS: String(ceiling),
      ...(idleMs === undefined ? {} : { DEV_IDLE_MS: String(idleMs) }),
      ...(opts['--full-page'] ? { SHOT_FULL: '1' } : {}),
    }
  );
  const report = checked.stdout
    .split('\n')
    .find(row => row.startsWith('[ARTIFACT] DEV_CHECK '))
    ?.slice('[ARTIFACT] DEV_CHECK '.length);
  if (checked.code !== 0 || !report) {
    process.stdout.write(checked.stdout);
    return checked.code || 1;
  }
  const { status: _status, wsUrl: _wsUrl, ...launch } = browser;
  process.stdout.write(
    JSON.stringify({
      ...JSON.parse(readFileSync(report, 'utf8')),
      report,
      browser: launch,
    }) + '\n'
  );
  return 0;
}

try {
  process.exitCode = await main(process.argv.slice(2));
} catch (error) {
  process.stderr.write(`[DEV_CHECK] ${errorMessage(error)}\n${USAGE}\n`);
  process.exitCode = 2;
}
