import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { fileURLToPath } from 'node:url';
import { childrenReport, parseElapsed, parseMeminfo, parseProcStat, parsePs, parsePsi, parseSize } from '../machine-snapshot.mjs';

const script = fileURLToPath(new URL('../machine-snapshot.mjs', import.meta.url));
const supported = process.platform === 'darwin' || process.platform === 'linux';

function run(args) {
  const result = spawnSync(process.execPath, [script, ...args], { encoding: 'utf8', env: process.env });
  return { status: result.status, json: JSON.parse(result.stdout) };
}

test('host readings parse from their native formats', () => {
  assert.equal(parseSize('10240.00M'), 10240 * 1024 ** 2);
  assert.equal(parseSize('512K'), 512 * 1024);
  assert.equal(parseSize('x'), null);
  assert.deepEqual(['05', '01:05', '2:01:05', '3-02:01:05'].map(parseElapsed), [null, 65, 7265, 3 * 86_400 + 7265]);
  assert.deepEqual(parsePs('  7943  4990  7943 Ss   00:01 /bin/zsh\n  12 1 12 Z 1-00:00:00 /Applications/My App.app/Contents/MacOS/My App\n'), [
    { pid: 7943, ppid: 4990, pgid: 7943, state: 'Ss', elapsedSeconds: 1, name: 'zsh' },
    { pid: 12, ppid: 1, pgid: 12, state: 'Z', elapsedSeconds: 86_400, name: 'My App' },
  ]);
  const stat = parseProcStat('4242 (a (b) c) S 1 4242 4242 0 -1 4194560 0 0 0 0 0 0 0 0 20 0 1 0 9000 0 0');
  assert.deepEqual(stat, { pid: 4242, ppid: 1, pgid: 4242, state: 'S', startTicks: 9000, name: 'a (b) c' });
  assert.deepEqual(parseMeminfo('MemTotal:       16000 kB\nMemAvailable:    8000 kB\nHugePages_Total:       4\n'), { MemTotal: 16000 * 1024, MemAvailable: 8000 * 1024, HugePages_Total: 4 });
  assert.deepEqual(parsePsi('some avg10=1.50 avg60=0.75 avg300=0.10 total=123\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=4\n'), {
    some: { avg10: 1.5, avg60: 0.75, avg300: 0.1 },
    full: { avg10: 0, avg60: 0, avg300: 0 },
  });
});

test('children: a named PID, its descendants and its process group are running; zombies have exited; the reader is excluded', () => {
  const row = (pid, ppid, pgid, state = 'S') => ({ pid, ppid, pgid, state, elapsedSeconds: 1, name: `p${pid}` });
  const table = [row(10, 1, 10), row(11, 10, 10), row(12, 11, 10), row(13, 1, 10), row(14, 10, 10, 'Z'), row(20, 1, 20), row(99, 12, 99), row(100, 99, 99)];
  const report = childrenReport(table, [10], 99);
  assert.deepEqual(report.running.map(item => [item.pid, item.relation]), [[10, 'self'], [11, 'descendant'], [12, 'descendant'], [13, 'group']]);
  assert.deepEqual(report.zombies.map(item => item.pid), [14]);
  assert.equal(report.allExited, false);
  const gone = childrenReport(table.filter(item => item.pgid !== 10), [10, 11], 99);
  assert.deepEqual([gone.running, gone.allExited, gone.checked], [[], true, [10, 11]]);
});

test('a snapshot reads load, memory and disk, and invalid options are refused', { skip: !supported }, () => {
  const { status, json } = run(['--disk', '.']);
  assert.equal(status, 0);
  assert.ok(json.cpus >= 1);
  assert.equal(json.load.exceedsCpus, json.load.oneMinute > json.cpus);
  assert.ok(json.memory.totalBytes > 0);
  if (json.memory.availableBytes !== null) assert.ok(json.memory.availableBytes <= json.memory.totalBytes);
  assert.equal(json.disk.length, 1);
  assert.ok(json.disk[0].freeBytes <= json.disk[0].totalBytes);
  assert.ok(Array.isArray(json.notes));
  for (const args of [['--children', '0'], ['--children', '12,x'], ['--disk'], ['--bogus', '1'], ['--disk', '/no/such/dir/for/this/test']]) {
    const refused = run(args);
    assert.deepEqual([refused.status, refused.json.ok], [2, false], args.join(' '));
  }
});

test('--children reports a live child and grandchild, then confirms both exited after the stop', { skip: !supported }, async () => {
  const parent = spawn(process.execPath, ['-e', `
    const { spawn } = require('node:child_process');
    const child = spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], { stdio: 'ignore' });
    child.on('exit', () => process.exit(0));
    console.log(child.pid);
  `], { stdio: ['ignore', 'pipe', 'inherit'] });
  const [line] = await once(parent.stdout, 'data');
  const grandchild = Number(String(line).trim());
  const exited = once(parent, 'exit');

  const live = run(['--children', String(parent.pid)]);
  assert.equal(live.status, 1, 'a running process fails the check');
  assert.deepEqual(live.json.children.running.map(item => [item.pid, item.relation]), [[parent.pid, 'self'], [grandchild, 'descendant']].toSorted((a, b) => a[0] - b[0]));

  process.kill(grandchild, 'SIGKILL');
  await exited;
  const done = run(['--children', `${parent.pid},${grandchild}`]);
  assert.equal(done.status, 0);
  assert.deepEqual([done.json.children.running, done.json.children.allExited], [[], true]);
});
