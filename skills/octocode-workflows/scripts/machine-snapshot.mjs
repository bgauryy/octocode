#!/usr/bin/env node
// One read of a shared machine's capacity before admitting heavy work (load, memory pressure, available memory, swap,
// free disk), and whether stopped processes and their children have exited. Read-only. Built-ins only.
import { execFileSync } from 'node:child_process';
import { readdirSync, readFileSync, statfsSync } from 'node:fs';
import os from 'node:os';
import { basename, resolve } from 'node:path';
import { isMainModule } from './lib/entrypoint.mjs';
import { errorMessage, fail } from './lib/fail.mjs';

const USAGE = `Usage: node machine-snapshot.mjs [--disk <dir>]... [--children <pid[,pid...]>]...

Reads the host once (macOS or Linux) and prints one JSON object:
  cpus      logical CPUs available to this process
  load      {"oneMinute","fiveMinutes","fifteenMinutes","perCpu","exceedsCpus"}: perCpu is the one-minute load over
            cpus; exceedsCpus is true when the one-minute load is above cpus
  memory    {"totalBytes","availableBytes","availablePercent","pressure":{"level","psi"}}
            macOS: availablePercent is the kernel memory-status level (kern.memorystatus_level, the free percentage
            memory_pressure reports) and availableBytes that share of totalBytes; level is normal | warn | critical
            (kern.memorystatus_vm_pressure_level); psi is null.
            Linux: availableBytes is MemAvailable (/proc/meminfo); psi is /proc/pressure/memory as
            {"some","full"}, each {"avg10","avg60","avg300"} percent of time tasks stalled on memory; level is null.
  swap      {"totalBytes","usedBytes"}
  disk      [{"path","freeBytes","totalBytes","freePercent"}] for each --disk (default: the current directory);
            freeBytes is the space available to this user
  children  with --children: {"checked","running","zombies","allExited"}
            running   [{"pid","ppid","pgid","state","elapsedSeconds","name","relation","of"}]: each named PID still
                      alive (relation self), its live descendants (descendant) and the members of a process group it
                      leads (group), which survive their parent's exit; of is the named PID; name is the executable
                      name only, never its arguments
            zombies   the same rows for processes that exited but were not reaped by their parent
            allExited true when running is empty
  notes     readings that were unavailable, which then print null
Options:
  --disk <dir>              a directory whose filesystem to read, such as the build output (repeatable)
  --children <pid[,pid...]> PIDs to check (repeatable, or comma-separated)
Effects: runs sysctl (macOS) and, with --children, ps (macOS); reads /proc (Linux) and statfs. Signals no process
and writes nothing.
Exit 0 with the snapshot, and with --children only when every listed process has exited; 1 when a --children
process is still running, the platform is neither macOS nor Linux, or the process table cannot be read; 2 for
invalid options or a --disk path that cannot be read. Failures print {"ok":false,"error":{"code","message"}}.`;

const tryRun = (command, args) => {
  try {
    return execFileSync(command, args, { encoding: 'utf8', timeout: 10_000, stdio: ['ignore', 'pipe', 'ignore'] });
  } catch {
    return null;
  }
};
const tryRead = file => {
  try {
    return readFileSync(file, 'utf8');
  } catch {
    return null;
  }
};
const round = (value, digits = 1) => (value == null || !Number.isFinite(value) ? null : Number(value.toFixed(digits)));

/** Bytes from a sysctl size such as 10240.00M. */
export function parseSize(text) {
  const match = /^([\d.]+)([KMGT]?)$/i.exec(text?.trim() ?? '');
  if (!match) return null;
  const power = ' KMGT'.indexOf(match[2].toUpperCase() || ' ');
  return Math.round(Number(match[1]) * 1024 ** power);
}

/** Seconds from a ps etime value: [[dd-]hh:]mm:ss. */
export function parseElapsed(text) {
  const match = /^(?:(\d+)-)?(?:(\d+):)?(\d+):(\d+)$/.exec(text.trim());
  if (!match) return null;
  const [, days = 0, hours = 0, minutes, seconds] = match;
  return Number(days) * 86_400 + Number(hours) * 3600 + Number(minutes) * 60 + Number(seconds);
}

/** Rows of `ps -axo pid=,ppid=,pgid=,stat=,etime=,comm=`. */
export function parsePs(text) {
  const rows = [];
  for (const line of text.split('\n')) {
    const match = /^\s*(\d+)\s+(\d+)\s+(\d+)\s+(\S+)\s+(\S+)\s+(.*)$/.exec(line);
    if (match) rows.push({ pid: Number(match[1]), ppid: Number(match[2]), pgid: Number(match[3]), state: match[4], elapsedSeconds: parseElapsed(match[5]), name: basename(match[6].trim()) });
  }
  return rows;
}

/** {pid, ppid, pgid, state, startTicks, name} from a /proc/<pid>/stat line; the name may hold spaces and parentheses. */
export function parseProcStat(text) {
  const open = text.indexOf('(');
  const close = text.lastIndexOf(')');
  if (open === -1 || close < open) return null;
  const fields = text.slice(close + 2).trim().split(/\s+/);
  return { pid: Number(text.slice(0, open).trim()), ppid: Number(fields[1]), pgid: Number(fields[2]), state: fields[0], startTicks: Number(fields[19]), name: text.slice(open + 1, close) };
}

/** /proc/meminfo values: kB lines in bytes, unitless lines (page counts) as given. */
export function parseMeminfo(text) {
  return Object.fromEntries([...text.matchAll(/^(\w+):\s+(\d+)(\s+kB)?$/gm)].map(([, key, value, kb]) => [key, Number(value) * (kb ? 1024 : 1)]));
}

/** {some, full} from a /proc/pressure file: each {avg10, avg60, avg300}. */
export function parsePsi(text) {
  const out = {};
  for (const [, kind, rest] of text.matchAll(/^(some|full)\s+(.*)$/gm)) {
    const values = Object.fromEntries(rest.trim().split(/\s+/).map(pair => pair.split('=')));
    out[kind] = { avg10: Number(values.avg10), avg60: Number(values.avg60), avg300: Number(values.avg300) };
  }
  return out;
}

function macMemory(notes) {
  const total = os.totalmem();
  const level = tryRun('sysctl', ['-n', 'kern.memorystatus_level']);
  const pressure = tryRun('sysctl', ['-n', 'kern.memorystatus_vm_pressure_level']);
  const swap = tryRun('sysctl', ['-n', 'vm.swapusage']);
  if (level == null) notes.push('kern.memorystatus_level unreadable: availableBytes and availablePercent are null');
  if (pressure == null) notes.push('kern.memorystatus_vm_pressure_level unreadable: pressure.level is null');
  if (swap == null) notes.push('vm.swapusage unreadable: swap is null');
  const percent = level == null ? null : Number(level);
  return {
    memory: {
      totalBytes: total,
      availableBytes: percent == null ? null : Math.round((total * percent) / 100),
      availablePercent: percent,
      pressure: { level: { 1: 'normal', 2: 'warn', 4: 'critical' }[Number(pressure)] ?? null, psi: null },
    },
    swap: swap == null ? null : { totalBytes: parseSize(/total = (\S+)/.exec(swap)?.[1]), usedBytes: parseSize(/used = (\S+)/.exec(swap)?.[1]) },
  };
}

function linuxMemory(notes) {
  const text = tryRead('/proc/meminfo');
  if (text == null) notes.push('/proc/meminfo unreadable: availableBytes and swap are null');
  const info = text == null ? {} : parseMeminfo(text);
  const psiText = tryRead('/proc/pressure/memory');
  if (psiText == null) notes.push('/proc/pressure/memory unreadable (kernel without PSI): pressure.psi is null');
  const total = info.MemTotal ?? os.totalmem();
  return {
    memory: {
      totalBytes: total,
      availableBytes: info.MemAvailable ?? null,
      availablePercent: info.MemAvailable == null ? null : round((info.MemAvailable / total) * 100),
      pressure: { level: null, psi: psiText == null ? null : parsePsi(psiText) },
    },
    swap: info.SwapTotal == null ? null : { totalBytes: info.SwapTotal, usedBytes: info.SwapTotal - (info.SwapFree ?? 0) },
  };
}

function processTable(platform) {
  if (platform === 'darwin') {
    const text = tryRun('ps', ['-axo', 'pid=,ppid=,pgid=,stat=,etime=,comm=']);
    if (text == null) throw new Error('ps could not list processes');
    return parsePs(text);
  }
  const ticks = Number(tryRun('getconf', ['CLK_TCK']));
  const uptime = Number(tryRead('/proc/uptime')?.split(/\s+/)[0]);
  const rows = [];
  for (const entry of readdirSync('/proc')) {
    if (!/^\d+$/.test(entry)) continue;
    const stat = tryRead(`/proc/${entry}/stat`);
    const row = stat == null ? null : parseProcStat(stat);
    if (!row) continue;
    const { startTicks, ...rest } = row;
    rows.push({ ...rest, elapsedSeconds: ticks > 0 && Number.isFinite(uptime) ? Math.max(0, Math.round(uptime - startTicks / ticks)) : null });
  }
  return rows;
}

/** The named PIDs still alive, their descendants and their process-group members; zombies apart. */
export function childrenReport(table, pids, self = process.pid) {
  const rows = table.filter(row => row.pid !== self && row.ppid !== self);
  const byParent = new Map();
  for (const row of rows) byParent.set(row.ppid, [...(byParent.get(row.ppid) ?? []), row]);
  const found = new Map();
  const add = (row, relation, of) => { if (!found.has(row.pid)) found.set(row.pid, { ...row, relation, of }); };
  for (const pid of pids) {
    const own = rows.find(row => row.pid === pid);
    if (own) add(own, 'self', pid);
    const visited = new Set([pid]);
    const stack = [...(byParent.get(pid) ?? [])];
    while (stack.length) {
      const row = stack.pop();
      if (visited.has(row.pid)) continue;
      visited.add(row.pid);
      add(row, 'descendant', pid);
      stack.push(...(byParent.get(row.pid) ?? []));
    }
    for (const row of rows) if (row.pgid === pid && row.pid !== pid) add(row, 'group', pid);
  }
  const all = [...found.values()].toSorted((a, b) => a.pid - b.pid);
  const running = all.filter(row => !row.state.startsWith('Z'));
  return { checked: pids, running, zombies: all.filter(row => row.state.startsWith('Z')), allExited: running.length === 0 };
}

function disk(path) {
  const absolute = resolve(path);
  let stats;
  try {
    stats = statfsSync(absolute);
  } catch (error) {
    fail('disk-unreadable', `--disk ${path}: ${error.code ?? errorMessage(error)}`, 2);
  }
  const block = stats.frsize || stats.bsize;
  const freeBytes = stats.bavail * block;
  const totalBytes = stats.blocks * block;
  return { path: absolute, freeBytes, totalBytes, freePercent: totalBytes ? round((freeBytes / totalBytes) * 100) : null };
}

function parseArgs(argv) {
  const options = { disks: [], children: [] };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--help' || arg === '-h') { console.log(USAGE); process.exit(0); }
    const value = argv[++i];
    if (value === undefined || value.startsWith('--')) fail('invalid-args', `${arg} needs a value; see --help`, 2);
    if (arg === '--disk') options.disks.push(value);
    else if (arg === '--children') {
      for (const item of value.split(',')) {
        if (!/^\d+$/.test(item.trim()) || Number(item) < 1) fail('invalid-args', `--children ${value}: expected positive integer PIDs`, 2);
        options.children.push(Number(item));
      }
    } else fail('invalid-args', `unknown argument ${arg}; see --help`, 2);
  }
  return options;
}

export function main(argv = process.argv.slice(2)) {
  const options = parseArgs(argv);
  const platform = process.platform;
  if (platform !== 'darwin' && platform !== 'linux') fail('unsupported-platform', `${platform}: this script reads macOS or Linux`, 1);
  const notes = [];
  const cpus = os.availableParallelism();
  const [oneMinute, fiveMinutes, fifteenMinutes] = os.loadavg().map(value => round(value, 2));
  const { memory, swap } = platform === 'darwin' ? macMemory(notes) : linuxMemory(notes);
  const disks = (options.disks.length ? options.disks : ['.']).map(disk);
  let children;
  if (options.children.length) {
    try {
      children = childrenReport(processTable(platform), [...new Set(options.children)]);
    } catch (error) {
      fail('process-table-unreadable', errorMessage(error), 1);
    }
  }
  const snapshot = {
    ok: children ? children.allExited : true,
    platform,
    cpus,
    load: { oneMinute, fiveMinutes, fifteenMinutes, perCpu: round(oneMinute / cpus, 2), exceedsCpus: oneMinute > cpus },
    memory,
    swap,
    disk: disks,
    ...(children ? { children } : {}),
    notes,
  };
  console.log(JSON.stringify(snapshot, null, 2));
  process.exit(snapshot.ok ? 0 : 1);
}

if (isMainModule(import.meta.url)) main();
