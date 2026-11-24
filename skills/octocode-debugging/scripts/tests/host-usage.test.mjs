import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { analyze, markerValue, parseClock, parsePsTable, parseSize, parseTopMemory, parseVmStat, readLinuxProcesses, serviceUnit } from '../host-usage.mjs';

const script = fileURLToPath(new URL('../host-usage.mjs', import.meta.url));
const options = (extra = {}) => ({ roots: [], rootNames: [], markers: [], markerPids: [], oldAfter: 3600, top: 10, expand: [], sort: 'memory', commands: false, ...extra });
const proc = (pid, ppid, extra = {}) => ({ pid, ppid, pgid: pid, uid: 501, ageSeconds: 10, cpuSeconds: 2, cpuBefore: 1, memoryBytes: pid * 100, memorySource: 'footprint', rssBytes: pid * 200, name: `p${pid}`, command: `p${pid} --run`, markers: {}, service: true, ...extra });

test('parsers read ps clocks, top sizes, vm_stat pages and env markers', () => {
  assert.equal(parseClock('05-21:14:42'), 5 * 86400 + 21 * 3600 + 14 * 60 + 42);
  assert.equal(parseClock('1176:55.48'), 1176 * 60 + 55.48);
  assert.equal(parseSize('2368K'), 2368 * 1024);
  assert.equal(parseSize('1.5G+'), 1.5 * 2 ** 30);
  assert.equal(parseSize('n/a'), null);
  const vm = parseVmStat('Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free:      3912.\n"Translation faults":   24.\nSwapouts:   7.\n');
  assert.equal(vm.pageSize, 16384);
  assert.deepEqual(vm.counts, { 'Pages free': 3912, 'Translation faults': 24, Swapouts: 7 });
  const top = parseTopMemory('Processes: 2 total\nPID    MEM   CMPRS\n99  150M  144M\n7  2368K 2032K\n');
  assert.deepEqual(top.get(99), { footprintBytes: 150 * 2 ** 20, compressedBytes: 144 * 2 ** 20 });
  assert.equal(top.size, 2);
  const [row] = parsePsTable('  644     1   644   501 05-21:10:09  6400   0:00.10 /Applications/Sublime Text.app/Contents/MacOS/sublime_text\n');
  assert.equal(row.comm, '/Applications/Sublime Text.app/Contents/MacOS/sublime_text');
  assert.equal(row.rssBytes, 6400 * 1024);
  assert.equal(markerValue('node app.mjs --flag LANE=fake LANE=real HOME=/x', 'LANE'), 'real');
  assert.equal(markerValue('node app.mjs', 'LANE'), undefined);
  assert.equal(serviceUnit('0::/system.slice/cron.service\n'), 'cron.service');
  assert.equal(serviceUnit('0::/user.slice/user-1000.slice/session-2.scope\n'), null);
});

test('groups under the nearest declared root, sums members and flags leak candidates with reasons', () => {
  // 1 init > 10 terminal > 20 shell > 30 session (declared) > 40 old child; 50 orphan (group leader 45 gone);
  // 60 reparented child of a session whose marker PID 99 has exited; 70 another user's process.
  const processes = [
    proc(1, 0),
    proc(10, 1),
    proc(20, 10),
    proc(30, 20, { name: 'Session', cpuBefore: 2 }),
    proc(40, 30, { ageSeconds: 7200 }),
    proc(50, 1, { pgid: 45, service: false }),
    proc(60, 1, { service: false, markers: { OWNER: '99' } }),
    proc(70, 1, { uid: 0, service: false }),
    proc(80, 1234),
  ];
  const result = analyze(processes, { elapsed: 2, ownUid: 501, selfPid: 1234, options: options({ rootNames: ['session'], markerPids: ['OWNER'], markers: ['OWNER'] }) });
  assert.ok(!result.roots.some((root) => root.tree.some((node) => node.pid === 80)), 'children of this script are excluded');
  const session = result.roots.find((root) => root.pid === 30);
  assert.equal(session.kind, 'declared');
  assert.equal(session.processes, 2);
  assert.equal(session.memoryBytes, 3000 + 4000);
  assert.equal(session.cpuPercent, 50, 'one CPU second over two elapsed seconds, root idle');
  assert.deepEqual(session.tree.map((node) => [node.pid, node.children?.map((child) => child.pid)]), [[30, [40]]]);
  const terminal = result.roots.find((root) => root.pid === 10);
  assert.equal(terminal.processes, 2, 'the shell joins its top-level ancestor; the session split off');
  const reasons = Object.fromEntries(result.leakCandidates.map((candidate) => [candidate.pid, candidate.reasons.map((reason) => reason.code)]));
  assert.deepEqual(reasons, { 40: ['oldChildOfIdleRoot'], 50: ['reparented'], 60: ['reparented', 'ownerExited'] });
  assert.match(result.leakCandidates.find((candidate) => candidate.pid === 50).reasons[0].detail, /leader 45 has exited/);
  assert.equal(result.totals.processes, processes.length - 1);
  assert.equal(result.totals.memoryBytes, processes.filter((p) => p.pid !== 80).reduce((sum, p) => sum + p.memoryBytes, 0));
});

test('only the first --top top-level groups carry trees; the rest are summed with the option that shows them', () => {
  const processes = [proc(1, 0), ...[10, 11, 12, 13].map((pid) => proc(pid, 1))];
  const result = analyze(processes, { elapsed: 1, ownUid: 501, options: options({ top: 2 }) });
  assert.equal(result.roots.length, 2);
  assert.deepEqual(result.roots.map((root) => root.pid), [13, 12], 'largest memory first');
  assert.equal(result.otherRoots.roots, 3);
  assert.equal(result.otherRoots.memoryBytes, 100 + 1000 + 1100);
  assert.equal(result.otherRoots.show, '--top 5');
});

test('reads a /proc tree: comm with spaces and parens, Pss when readable, RSS otherwise, services and markers', (t) => {
  const root = mkdtempSync(join(tmpdir(), 'host-usage-proc-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  writeFileSync(join(root, 'uptime'), '1000.00 1.00\n');
  const write = (pid, files) => {
    mkdirSync(join(root, String(pid)));
    for (const [name, text] of Object.entries(files)) writeFileSync(join(root, String(pid), name), text);
  };
  // Fields after "(comm)": state ppid pgrp session ... utime(14) stime(15) ... starttime(22) vsize rss(24).
  const stat = (pid, comm, ppid, utime, stime, start, rss) => `${pid} (${comm}) S ${ppid} ${pid} ${pid} 0 -1 0 0 0 0 0 ${utime} ${stime} 0 0 20 0 1 0 ${start} 0 ${rss} 0\n`;
  write(42, { stat: stat(42, 'my (odd) app', 1, 300, 100, 50000, 10), status: 'Name:\tx\nUid:\t1000\t1000\t1000\t1000\n', smaps_rollup: 'Rss: 40 kB\nPss: 12 kB\nPss_Anon: 8 kB\n', cmdline: 'app\0--x\0', cgroup: '0::/system.slice/app.service\n', environ: 'LANE=a\0HOME=/h\0' });
  write(43, { stat: stat(43, 'worker', 42, 0, 0, 90000, 5), status: 'Uid:\t0\t0\t0\t0\n', cgroup: '0::/user.slice/session-1.scope\n' });
  const [app, worker] = readLinuxProcesses({ proc: root, ticks: 100, pageSize: 4096, markers: ['LANE'] }).sort((a, b) => a.pid - b.pid);
  assert.equal(app.name, 'my (odd) app');
  assert.equal(app.ppid, 1);
  assert.equal(app.cpuSeconds, 4);
  assert.equal(app.ageSeconds, 1000 - 500);
  assert.equal(app.memoryBytes, 12 * 1024);
  assert.equal(app.memorySource, 'pss');
  assert.equal(app.command, 'app --x');
  assert.equal(app.service, true);
  assert.deepEqual(app.markers, { LANE: 'a' });
  assert.equal(worker.memorySource, 'rss', 'unreadable smaps_rollup falls back to RSS');
  assert.equal(worker.memoryBytes, 5 * 4096);
  assert.equal(worker.service, false);
  assert.equal(worker.uid, 0);
});

test('help and invalid options; a live snapshot of this host parses', () => {
  const help = spawnSync(process.execPath, [script, '--help'], { encoding: 'utf8' });
  assert.equal(help.status, 0);
  assert.match(help.stdout, /leakCandidates/);
  const bad = spawnSync(process.execPath, [script, '--interval', '0'], { encoding: 'utf8' });
  assert.equal(bad.status, 2);
  assert.equal(JSON.parse(bad.stdout).error.code, 'invalid-option');
  if (process.platform !== 'darwin' && process.platform !== 'linux') return;
  const live = spawnSync(process.execPath, [script, '--interval', '0.2', '--top', '1'], { encoding: 'utf8', maxBuffer: 2 ** 26 });
  assert.equal(live.status, 0, live.stdout + live.stderr);
  const snapshot = JSON.parse(live.stdout);
  assert.ok(snapshot.coverage.processes > 0);
  assert.equal(snapshot.totals.processes, snapshot.roots.reduce((sum, root) => sum + root.processes, 0) + snapshot.otherRoots.processes);
  assert.ok(snapshot.host.cpus > 0);
});
