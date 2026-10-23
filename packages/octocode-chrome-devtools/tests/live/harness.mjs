// Shared live-suite harness: engine and check-script runners against the suite's isolated
// Chrome, snapshot/artifact readers, assertions and the case runner. The entry assigns
// `live` (package root, work dir, CDP port, fixture base URL, --only filters) first.
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

export const live = { ROOT: '', WORK: '', CDP_PORT: '', BASE: '', ONLY: [] };
export const results = [];

export function node(script, scriptArgs, env = {}, seconds = 60) {
  const res = spawnSync(
    process.execPath,
    [join(live.ROOT, script), ...scriptArgs],
    {
      cwd: live.WORK,
      env: { ...process.env, ...env },
      encoding: 'utf8',
      timeout: seconds * 1000,
    }
  );
  const failure = res.error
    ? `\n[SUITE] spawn error: ${res.error.message}`
    : res.signal
      ? `\n[SUITE] killed by ${res.signal} after ${seconds}s`
      : '';
  return {
    code: res.status,
    out: `${res.stdout ?? ''}${res.stderr ?? ''}${failure}`,
  };
}
export const check = (
  name,
  env = {},
  target = ['--target-url', live.BASE, '--no-reload']
) =>
  node(
    'dist/engine/cdp-sandbox.mjs',
    [
      join(live.ROOT, 'dist/engine', 'cdp-checks', name),
      '--port',
      live.CDP_PORT,
      '--keep-tab',
      ...target,
    ],
    env
  );
export const success = result => {
  if (result.code !== 0) throw new Error(result.out);
  return result;
};
export const open = (name, path, env = {}) =>
  success(check(name, env, ['--new-tab', `${live.BASE}/${path}`]));
export const cli = args =>
  node('dist/engine/cli.mjs', [...args, '--port', live.CDP_PORT]);
export const execute = (steps, options = {}) =>
  check(
    'browser-execute.mjs',
    { BROWSER_PLAN: JSON.stringify({ waitMs: 4000, ...options, steps }) },
    ['--new-tab', 'about:blank']
  );

// ref of the first snapshot row whose role+label matches, e.g. ref(out, 'button "More"')
export function ref(out, label) {
  const line = out
    .split('\n')
    .find(l => /^\[SNAPSHOT\] \[e\d+\] /.test(l) && l.includes(`] ${label}`));
  if (!line) throw new Error(`no snapshot row for ${label}`);
  return line.match(/\[(e\d+)\]/)[1];
}
export const snapshot = () => success(check('page-snapshot.mjs')).out;
export const act = env => success(check('dom-operations-check.mjs', env)).out;
export const waitFor = text =>
  act({ DOM_ACTION: 'wait', DOM_VALUE: text, DOM_WAIT_MS: '4000' });

/** Path from the `[ARTIFACT] <name> <path>` line; throws when the output has none. */
function artifactPath(out, name) {
  const prefix = `[ARTIFACT] ${name} `;
  const line = out.split('\n').find(row => row.startsWith(prefix));
  if (!line) throw new Error(`Missing ${name}: ${out}`);
  return line.slice(prefix.length);
}
export const artifact = (out, name) =>
  JSON.parse(readFileSync(artifactPath(out, name)));
export const savedPageText = out =>
  readFileSync(artifactPath(out, 'PAGE_TEXT'), 'utf8');
/** A file the engine keeps under the suite port's session metadata. */
export const sessionMeta = file =>
  JSON.parse(
    readFileSync(
      join(
        live.WORK,
        '.octocode/octocode-chrome-devtools/tmp/session-meta',
        'port-' + live.CDP_PORT,
        file
      ),
      'utf8'
    )
  );

export async function runCase(name, fn) {
  if (live.ONLY.length && !live.ONLY.some(part => name.includes(part))) return;
  const started = Date.now();
  try {
    await fn();
    results.push({ name, ok: true, ms: Date.now() - started });
    console.log(`ok   ${name} (${Date.now() - started}ms)`);
  } catch (error) {
    results.push({ name, ok: false, error: error.message });
    console.log(`FAIL ${name}: ${error.message}`);
  }
}
export function expect(out, pattern, what) {
  if (!pattern.test(out))
    throw new Error(
      `${what ?? pattern} not found in output:\n${out
        .split('\n')
        .filter(l =>
          /^\[(ACTION|VERIFY|FINDING|WAIT|NEW|DIFF|SNAPSHOT|METRIC|SCREENSHOT)/.test(
            l
          )
        )
        .slice(0, 30)
        .join('\n')}`
    );
}
export function refute(out, pattern, what) {
  if (pattern.test(out)) throw new Error(`unexpected ${what ?? pattern}`);
}
