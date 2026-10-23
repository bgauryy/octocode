// The suite's isolated Chrome: engine CLI calls, launch, liveness and the fixture worker.
// The entry assigns `suite` (work dir, port, engine dir) before any call.
import { spawn } from 'node:child_process';
import { join } from 'node:path';

export const suite = { work: '', port: '', engineDir: '' };
export const counters = { engineCalls: 0, relaunches: 0, engineMs: 0 };
// The initial page owns a dedicated worker that a replaced browser or target must regain.
export const fixtureUrl =
  'data:text/html,' +
  encodeURIComponent(
    '<title>cdp-all</title><script>new Worker("data:text/javascript,setInterval(()=>{},1000)")</script>'
  );

export function engine(argv, timeout = 120000) {
  const started = Date.now();
  counters.engineCalls++;
  return new Promise((yes, no) => {
    const child = spawn(
      process.execPath,
      [join(suite.engineDir, 'cli.mjs'), ...argv],
      {
        cwd: suite.work,
        stdio: ['ignore', 'pipe', 'pipe'],
      }
    );
    let out = '';
    child.stdout.on('data', chunk => (out += chunk));
    child.stderr.on('data', chunk => (out += chunk));
    const timer = setTimeout(() => child.kill('SIGKILL'), timeout);
    child.once('error', no);
    child.once('close', code => {
      clearTimeout(timer);
      counters.engineMs += Date.now() - started;
      yes({ code, out });
    });
  });
}
export const artifact = (out, name) => {
  const prefix = '[ARTIFACT] ' + name + ' ',
    line = out.split('\n').find(row => row.startsWith(prefix));
  return line ? line.slice(prefix.length) : null;
};
async function alive() {
  try {
    const res = await fetch(`http://127.0.0.1:${suite.port}/json/version`, {
      signal: AbortSignal.timeout(1500),
    });
    return res.ok;
  } catch {
    return false;
  }
}
async function listTargets() {
  try {
    const res = await fetch(`http://127.0.0.1:${suite.port}/json/list`, {
      signal: AbortSignal.timeout(1500),
    });
    return res.ok ? await res.json() : [];
  } catch {
    return [];
  }
}
const hasWorker = async () =>
  (await listTargets()).some(target => target.type === 'worker');
// Waits for the fixture page's worker; a sweep that lost it opens the fixture again.
async function ensureWorker(reopen = false) {
  if (reopen && !(await hasWorker()))
    await fetch(
      `http://127.0.0.1:${suite.port}/json/new?${encodeURIComponent(fixtureUrl)}`,
      { method: 'PUT', signal: AbortSignal.timeout(5000) }
    ).catch(() => {});
  for (let waited = 0; waited < 5000; waited += 100) {
    if (await hasWorker()) return;
    await new Promise(yes => setTimeout(yes, 100));
  }
  throw Error('Fixture worker target did not appear');
}
export async function launch(first = false) {
  if (!first) counters.relaunches++;
  const r = await engine([
    'open',
    '--headless',
    '--port',
    suite.port,
    '--url',
    fixtureUrl,
  ]);
  const info = JSON.parse(
    r.out.split('\n').find(line => line.startsWith('{')) ?? '{}'
  );
  if (r.code !== 0 || info.status !== 'BROWSER_READY')
    throw Error('Launch failed: ' + r.out);
  if (first && info.reused !== false)
    throw Error('Suite needs a fresh isolated browser');
  await ensureWorker();
  return info;
}
// Returns true when the browser had died and was relaunched.
export async function ensureBrowser() {
  if (await alive()) {
    await ensureWorker(true);
    return false;
  }
  // A destroyed browser leaves a stale tracked session; cleanup is idempotent.
  await engine(['cleanup', '--port', suite.port], 30000);
  await launch();
  return true;
}
