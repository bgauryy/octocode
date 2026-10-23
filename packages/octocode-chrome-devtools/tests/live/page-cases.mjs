// Screenshots, key presses, list snapshots, performance metrics and cross-cwd cleanup.
import { spawnSync } from 'node:child_process';
import { readFileSync, existsSync, mkdirSync, realpathSync } from 'node:fs';
import { join } from 'node:path';
import {
  live,
  check,
  success,
  open,
  ref,
  snapshot,
  act,
  expect,
  refute,
} from './harness.mjs';

export const cases = [];
const test = (name, fn) => cases.push([name, fn]);

test('screenshot: annotated refs', () => {
  open('page-snapshot.mjs', 'actions.html');
  const out = success(check('page-screenshot.mjs', { SHOT_ANNOTATE: '1' })).out;
  const labels = Number(out.match(/labels=(\d+)/)?.[1] ?? 0);
  if (labels < 5) throw new Error(`expected >=5 labels, got ${labels}`);
  const file = out.match(/\[SCREENSHOT\] (\S+)/)?.[1];
  if (!file || !existsSync(file)) throw new Error('screenshot file missing');
});

test('screenshot: long page retains every tile', () => {
  const url =
    'data:text/html,<body style="margin:0;height:17000px;background:linear-gradient(red,blue)">Long page</body>';
  const result = success(
    check('page-screenshot.mjs', { SHOT_FULL: '1', SHOT_SCALE: '0.25' }, [
      '--new-tab',
      url,
    ])
  ).out;
  const path = result.match(/\[ARTIFACT\] screenshot-manifest.json (.+)/)?.[1];
  const manifest = JSON.parse(readFileSync(path));
  if (!manifest.complete || manifest.tiles.length !== 3)
    throw new Error('Long screenshot is incomplete');
  let y = 0;
  for (const tile of manifest.tiles) {
    if (tile.clip.y !== y || !readFileSync(tile.file).length)
      throw new Error('Screenshot coverage gap');
    y += tile.clip.height;
  }
  if (y !== 17000) throw new Error('Screenshot height was truncated');
  refute(result, /SHOT_TRUNCATED/);
});

test('press: Enter submits and navigates', () => {
  open('page-snapshot.mjs', 'actions.html');
  act({ DOM_SELECTOR: '#name', DOM_ACTION: 'fill', DOM_VALUE: 'Ada' });
  const snap = snapshot();
  const out = act({
    DOM_REF: ref(snap, 'textbox "Name"'),
    DOM_ACTION: 'press',
    DOM_KEY: 'Enter',
  });
  expect(out, /navigated=.*\/done\?q=Ada/);
  expect(out, /\[NEXT\] page navigated/);
});

test('snapshot: list context and unnamed links', () => {
  const out = open('page-snapshot.mjs', 'list.html').out;
  expect(out, /— .*128 points by ada/);
  expect(out, /\] link "upvote"/);
  expect(out, /\] link ~"votearrow/);
});

test('perf: LCP and CLS from a late hero and banner', () => {
  const result = check(
    'performance-measure-check.mjs',
    { MEASURE_URL: `${live.BASE}/perf.html`, PERF_WAIT_MS: '3500' },
    ['--new-tab', 'about:blank']
  );
  if (result.code !== 0) throw new Error(result.out);
  const out = result.out;
  const lcp = Number(out.match(/lcp=(\d+)/)?.[1] ?? 0);
  const cls = Number(out.match(/cls=([\d.]+)/)?.[1] ?? 0);
  if (lcp < 2500) throw new Error(`expected lcp >= 2500, got ${lcp}`);
  if (cls < 0.05) throw new Error(`expected cls >= 0.05, got ${cls}`);
  expect(out, /\[FINDING\] PERF_SLOW_LCP|SLOW_LCP/);
});

test('cleanup: another cwd stops the isolated Chrome it did not open', () => {
  const port = String(Number(live.CDP_PORT) + 400);
  const [owner, other] = ['owner', 'other'].map(name => {
    const dir = join(live.WORK, `cleanup-${name}`);
    mkdirSync(dir, { recursive: true });
    return realpathSync(dir);
  });
  const launcher = (cwd, launchArgs) => {
    const res = spawnSync(
      process.execPath,
      [join(live.ROOT, 'dist/engine/cli.mjs'), ...launchArgs, '--port', port],
      { cwd, encoding: 'utf8', timeout: 60000 }
    );
    if (res.status !== 0) throw new Error(`${res.stdout}${res.stderr}`);
    return JSON.parse(res.stdout.trim().split('\n').at(-1));
  };
  const opened = launcher(owner, [
    'open',
    '--headless',
    '--url',
    'about:blank',
  ]);
  const session = JSON.parse(readFileSync(opened.sessionFile, 'utf8'));
  try {
    const cleaned = launcher(other, ['cleanup']);
    if (cleaned.status !== 'CLEANED_UP' || cleaned.workspace !== owner)
      throw new Error(JSON.stringify(cleaned));
    let gone = false;
    try {
      process.kill(session.pid, 0);
    } catch {
      gone = true;
    }
    if (!gone) throw new Error(`Chrome pid ${session.pid} still running`);
    if (existsSync(session.profileDir) || existsSync(opened.sessionFile))
      throw new Error('profile or session file left behind');
  } finally {
    launcher(owner, ['cleanup']);
  }
});
