#!/usr/bin/env node
// Attribute host CPU and memory to process trees and flag processes that may have outlived their session. Read-only.
import { execFile } from 'node:child_process';
import { readdirSync, readFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';
import { isMainModule } from './lib/entrypoint.mjs';
import { errorMessage, fail } from './lib/fail.mjs';

const USAGE = `Usage: node host-usage.mjs [options]
Samples the process table and the host's paging counters twice, --interval seconds apart, and prints one JSON object:
  host            cpus, loadAverage and loadExceedsCpus; memory pressure (macOS level and available percent, Linux
                  PSI); swap; paging between the samples (activePaging: swap-outs rose); macOS compressorBytes
                  (pages occupied by the compressor times the page size vm_stat reports)
  roots           process groups, largest first. Each process joins the group of its --marker value, else of its
                  nearest declared root (--root, --root-name), else of its top-level ancestor below PID 1. A group sums
                  memoryBytes (macOS physical footprint, top MEM; Linux Pss from smaps_rollup; RSS where those cannot be
                  read), rssBytes, compressedBytes (macOS) and cpuPercent (CPU time used between the samples divided
                  by the elapsed time; 100 = one core). Declared, marker, --expand and the first --top groups carry
                  their process tree; otherRoots sums the rest.
  leakCandidates  processes of your user, each with the reasons it was flagged:
                    reparented          parent is PID 1 and it is not a service (macOS: not a launchd job and its
                                        executable is outside /System and .app/.appex/.xpc/.framework bundles;
                                        Linux: not in a systemd .service unit)
                    ownerExited         its --marker-pid variable names a PID that no longer exists
                    oldChildOfIdleRoot  at least --old-after seconds old under a declared root (or live --marker-pid
                                        owner) that used no CPU between the samples
  coverage        processes read; cpuUnknown (started or exited between samples); memoryAsRss (footprint or Pss
                  unreadable); the user the leak checks cover
Options:
  --interval <s>      seconds between the two samples (default 1)
  --root <pid>        a session root (repeatable)
  --root-name <name>  processes whose executable name equals <name>, ignoring case, are session roots (repeatable)
  --marker <VAR>      group processes by the value of environment variable VAR before ancestry (repeatable). Only
                      your own processes expose it; only VAR is read and its value appears in the group key, so pick
                      a non-secret marker such as a session id or lane name
  --marker-pid <VAR>  as --marker, where VAR holds the owning session's PID; flags ownerExited when it is gone
  --old-after <s>     age for oldChildOfIdleRoot (default 3600)
  --top <n>           top-level groups printed with their tree (default 10)
  --expand <pid>      also print the tree of the group rooted at <pid> (repeatable)
  --sort memory|cpu   group order (default memory)
  --commands          add each tree node's command line (roots and candidates always carry theirs)
Effects: runs ps, top, sysctl, vm_stat and launchctl list on macOS; reads /proc and runs getconf on Linux. Signals no
process and writes nothing.
Exit 0 with the snapshot; 1 when the process table cannot be read or the platform is neither macOS nor Linux; 2 for
invalid options. Failures print {"ok":false,"error":{code,message}}.`;

const execFileP = promisify(execFile);
const exec = async (command, args) => (await execFileP(command, args, { encoding: 'utf8', maxBuffer: 2 ** 28 })).stdout;
const tryExec = (command, args) => exec(command, args).catch(() => null);
const sleep = (seconds) => new Promise((resolve) => setTimeout(resolve, seconds * 1000));
const now = () => Number(process.hrtime.bigint()) / 1e9;
const tryRead = (file) => {
  try {
    return readFileSync(file, 'utf8');
  } catch {
    return null;
  }
};

/** Runs `read` and returns its value with the midpoint of the call, the best estimate of when it sampled. */
async function timed(read) {
  const start = now();
  const value = await read();
  return { value, at: (start + now()) / 2 };
}

function parseArgs(argv) {
  const options = { interval: 1, roots: [], rootNames: [], markers: [], markerPids: [], oldAfter: 3600, top: 10, expand: [], sort: 'memory', commands: false };
  const positive = (flag, value) => {
    const number = Number(value);
    if (!Number.isFinite(number) || number <= 0) fail('invalid-option', `${flag}: a positive number`, 2);
    return number;
  };
  const pid = (flag, value) => {
    if (!/^[1-9]\d*$/.test(value ?? '')) fail('invalid-option', `${flag}: a process id`, 2);
    return Number(value);
  };
  for (let i = 0; i < argv.length; i++) {
    const flag = argv[i];
    if (flag === '--help') {
      console.log(USAGE);
      process.exit(0);
    }
    if (flag === '--commands') {
      options.commands = true;
      continue;
    }
    const value = argv[++i];
    if (value === undefined) fail('invalid-option', `${flag} needs a value; see --help`, 2);
    if (flag === '--interval') options.interval = positive(flag, value);
    else if (flag === '--old-after') options.oldAfter = positive(flag, value);
    else if (flag === '--top') {
      options.top = positive(flag, value);
      if (!Number.isInteger(options.top)) fail('invalid-option', '--top: a positive integer', 2);
    } else if (flag === '--root') options.roots.push(pid(flag, value));
    else if (flag === '--expand') options.expand.push(pid(flag, value));
    else if (flag === '--root-name') options.rootNames.push(value.toLowerCase());
    else if (flag === '--marker' || flag === '--marker-pid') {
      if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(value)) fail('invalid-option', `${flag}: an environment variable name`, 2);
      options.markers.push(value);
      if (flag === '--marker-pid') options.markerPids.push(value);
    } else if (flag === '--sort' && (value === 'memory' || value === 'cpu')) options.sort = value;
    else fail('invalid-option', `unknown option ${flag} ${value}; see --help`, 2);
  }
  return options;
}

/** "[[dd-]hh:]mm:ss[.cc]" (ps etime and time) as seconds. */
export function parseClock(text) {
  const [days, clock] = text.includes('-') ? text.split('-') : ['0', text];
  const seconds = Number(days) * 86400 + clock.split(':').reduce((sum, part) => sum * 60 + Number(part), 0);
  return Number.isFinite(seconds) ? seconds : null;
}

const UNITS = { B: 1, K: 2 ** 10, M: 2 ** 20, G: 2 ** 30, T: 2 ** 40 };
/** A top or sysctl size such as "2368K", "1.5G" or "150M+" as bytes. */
export function parseSize(text) {
  const match = /^([\d.]+)([BKMGT]?)[+-]?$/.exec(String(text).trim());
  return match ? Math.round(Number(match[1]) * UNITS[match[2] || 'B']) : null;
}

/** The value of environment variable `name` in `ps -E` text: the last NAME=value word, since the environment follows the arguments. */
export function markerValue(text, name) {
  let value;
  for (const match of text.matchAll(new RegExp(`(?:^|\\s)${name}=(\\S+)`, 'g'))) value = match[1];
  return value;
}

/** Rows of `ps -axo pid=,ppid=,pgid=,uid=,etime=,rss=,time=,comm=` (macOS; rss in KiB). */
export function parsePsTable(text) {
  const rows = [];
  for (const line of text.split('\n')) {
    const match = /^\s*(\d+)\s+(\d+)\s+(\d+)\s+(\d+)\s+(\S+)\s+(\d+)\s+(\S+)\s+(.*)$/.exec(line);
    if (!match) continue;
    const [, pid, ppid, pgid, uid, etime, rss, time, comm] = match;
    rows.push({ pid: +pid, ppid: +ppid, pgid: +pgid, uid: +uid, ageSeconds: parseClock(etime), rssBytes: +rss * 1024, cpuSeconds: parseClock(time), comm: comm.trim() });
  }
  return rows;
}

/** pid → text after the pid, for `ps -o pid=,<one free-text column>=` output. */
export function parsePidText(text) {
  const out = new Map();
  for (const line of text.split('\n')) {
    const match = /^\s*(\d+)\s(.*)$/.exec(line);
    if (match) out.set(+match[1], match[2]);
  }
  return out;
}

/** pid → { footprintBytes, compressedBytes } from `top -l 1 -stats pid,mem,cmprs`. */
export function parseTopMemory(text) {
  const out = new Map();
  let table = false;
  for (const line of text.split('\n')) {
    if (/^PID\s+MEM\s+CMPRS/.test(line)) table = true;
    else if (table) {
      const [pid, mem, cmprs] = line.trim().split(/\s+/);
      if (/^\d+$/.test(pid ?? '')) out.set(+pid, { footprintBytes: parseSize(mem), compressedBytes: parseSize(cmprs) });
    }
  }
  return out;
}

/** { pageSize, counts } from vm_stat: counts are pages, keyed by the label. */
export function parseVmStat(text) {
  const pageSize = Number(/page size of (\d+) bytes/.exec(text)?.[1]);
  const counts = {};
  for (const match of text.matchAll(/^"?([^":\n]+)"?:\s+(\d+)\.?$/gm)) counts[match[1].trim()] = Number(match[2]);
  return { pageSize, counts };
}

/** { some, full } from a /proc/pressure file: each { avg10, avg60, avg300 } percent of time stalled, total µs. */
export function parsePsi(text) {
  const out = {};
  for (const match of text.matchAll(/^(some|full)\s+(.*)$/gm)) out[match[1]] = Object.fromEntries(match[2].trim().split(/\s+/).map((pair) => pair.split('=')).map(([key, value]) => [key, Number(value)]));
  return out;
}

/** The systemd unit a /proc/<pid>/cgroup file places the process in, when that unit is a service. */
export function serviceUnit(cgroup) {
  for (const line of cgroup.split('\n')) {
    const unit = line.split(':').at(-1)?.split('/').filter(Boolean).at(-1);
    if (unit?.endsWith('.service')) return unit;
  }
  return null;
}

const MAC_SERVICE_PATH = /(^\/System\/|\.(app|appex|xpc|framework)\/)/;

async function collectMac(options) {
  const top = tryExec('top', ['-l', '1', '-stats', 'pid,mem,cmprs']);
  const vm0 = await timed(() => tryExec('vm_stat', []));
  const cpu0 = await timed(() => exec('ps', ['-axo', 'pid=,time=']));
  await sleep(options.interval);
  const vm1 = await timed(() => tryExec('vm_stat', []));
  const table = await timed(() => exec('ps', ['-axo', 'pid=,ppid=,pgid=,uid=,etime=,rss=,time=,comm=']));
  const [commands, environments, launchd, pressure, available, swap, memory] = await Promise.all([
    exec('ps', ['-axww', '-o', 'pid=,command=']),
    options.markers.length ? exec('ps', ['-axwwE', '-o', 'pid=,command=']) : null,
    tryExec('launchctl', ['list']),
    tryExec('sysctl', ['-n', 'kern.memorystatus_vm_pressure_level']),
    tryExec('sysctl', ['-n', 'kern.memorystatus_level']),
    tryExec('sysctl', ['-n', 'vm.swapusage']),
    top,
  ]);
  const before = new Map([...parsePidText(cpu0.value)].map(([pid, time]) => [pid, parseClock(time.trim())]));
  const commandOf = parsePidText(commands);
  const environmentOf = environments ? parsePidText(environments) : new Map();
  const footprints = memory ? parseTopMemory(memory) : new Map();
  const jobs = launchd ? new Set(launchd.split('\n').map((line) => line.split('\t')[0]).filter((pid) => /^\d+$/.test(pid)).map(Number)) : null;
  const processes = parsePsTable(table.value).map((row) => {
    const used = footprints.get(row.pid);
    const environment = environmentOf.get(row.pid) ?? '';
    return {
      ...row,
      name: path.basename(row.comm),
      command: commandOf.get(row.pid)?.trim() || row.comm,
      cpuBefore: before.get(row.pid) ?? null,
      memoryBytes: used?.footprintBytes ?? row.rssBytes,
      memorySource: used?.footprintBytes == null ? 'rss' : 'footprint',
      compressedBytes: used?.compressedBytes ?? null,
      markers: Object.fromEntries(options.markers.map((name) => [name, markerValue(environment, name)]).filter(([, value]) => value)),
      service: jobs?.has(row.pid) || MAC_SERVICE_PATH.test(row.comm),
    };
  });
  const levels = { 1: 'normal', 2: 'warn', 4: 'critical' };
  const swapText = swap ?? '';
  const [vmBefore, vmAfter] = [vm0.value, vm1.value].map((text) => (text ? parseVmStat(text) : null));
  const delta = (label) => (vmBefore && vmAfter ? vmAfter.counts[label] - vmBefore.counts[label] : null);
  const host = {
    memory: {
      pressure: levels[Number(pressure)] ?? null,
      availablePercent: available == null ? null : Number(available),
    },
    swap: { totalBytes: parseSize(/total = (\S+)/.exec(swapText)?.[1] ?? ''), usedBytes: parseSize(/used = (\S+)/.exec(swapText)?.[1] ?? '') },
    paging: {
      seconds: round(vm1.at - vm0.at, 2),
      swapins: delta('Swapins'),
      swapouts: delta('Swapouts'),
      activePaging: delta('Swapouts') == null ? null : delta('Swapouts') > 0,
    },
    pageSize: vmAfter?.pageSize ?? null,
    compressorBytes: vmAfter ? vmAfter.counts['Pages occupied by compressor'] * vmAfter.pageSize : null,
    storedInCompressorBytes: vmAfter ? vmAfter.counts['Pages stored in compressor'] * vmAfter.pageSize : null,
  };
  const notes = [];
  if (!memory) notes.push('top failed: memoryBytes is RSS for every process');
  if (!jobs) notes.push('launchctl list failed: reparented checks only the executable path');
  return { processes, host, elapsed: table.at - cpu0.at, metric: 'footprint', notes };
}

/** CPU seconds per pid from /proc/<pid>/stat (utime + stime). */
export function readLinuxCpu(proc, ticks) {
  const out = new Map();
  for (const entry of readdirSync(proc)) {
    if (!/^\d+$/.test(entry)) continue;
    const fields = statFields(tryRead(path.join(proc, entry, 'stat')));
    if (fields) out.set(Number(entry), (Number(fields[11]) + Number(fields[12])) / ticks);
  }
  return out;
}

/** The space-separated fields after "(comm)" in a /proc/<pid>/stat line, and the comm itself as `.comm`. */
function statFields(stat) {
  if (!stat) return null;
  const close = stat.lastIndexOf(')');
  const fields = stat.slice(close + 2).trim().split(' ');
  fields.comm = stat.slice(stat.indexOf('(') + 1, close);
  return fields;
}

/** Processes from a /proc tree: ids, age, CPU seconds, RSS, Pss (when smaps_rollup is readable), cgroup and markers. */
export function readLinuxProcesses({ proc = '/proc', ticks, pageSize, markers = [] }) {
  const uptime = Number(readFileSync(path.join(proc, 'uptime'), 'utf8').split(' ')[0]);
  const processes = [];
  for (const entry of readdirSync(proc)) {
    if (!/^\d+$/.test(entry)) continue;
    const dir = path.join(proc, entry);
    const fields = statFields(tryRead(path.join(dir, 'stat')));
    if (!fields) continue;
    const rssBytes = Number(fields[21]) * pageSize;
    const rollup = tryRead(path.join(dir, 'smaps_rollup'));
    const pss = rollup == null ? null : Number(/^Pss:\s+(\d+) kB/m.exec(rollup)?.[1] ?? 0) * 1024;
    const environment = markers.length ? (tryRead(path.join(dir, 'environ')) ?? '').split('\0') : [];
    const command = (tryRead(path.join(dir, 'cmdline')) ?? '').replace(/\0+$/, '').replaceAll('\0', ' ');
    processes.push({
      pid: Number(entry),
      ppid: Number(fields[1]),
      pgid: Number(fields[2]),
      sid: Number(fields[3]),
      uid: Number(/^Uid:\s+(\d+)/m.exec(tryRead(path.join(dir, 'status')) ?? '')?.[1] ?? NaN),
      ageSeconds: uptime - Number(fields[19]) / ticks,
      cpuSeconds: (Number(fields[11]) + Number(fields[12])) / ticks,
      rssBytes,
      memoryBytes: pss ?? rssBytes,
      memorySource: pss == null ? 'rss' : 'pss',
      name: fields.comm,
      comm: fields.comm,
      command: command || `[${fields.comm}]`,
      markers: Object.fromEntries(markers.map((name) => [name, environment.findLast((pair) => pair.startsWith(`${name}=`))?.slice(name.length + 1)]).filter(([, value]) => value)),
      service: serviceUnit(tryRead(path.join(dir, 'cgroup')) ?? '') !== null,
    });
  }
  return processes;
}

async function collectLinux(options) {
  const [ticks, pageSize] = (await Promise.all([exec('getconf', ['CLK_TCK']), exec('getconf', ['PAGESIZE'])])).map(Number);
  const vmstat = () => Object.fromEntries((tryRead('/proc/vmstat') ?? '').split('\n').map((line) => line.split(' ')).map(([key, value]) => [key, Number(value)]));
  const vm0 = await timed(vmstat);
  const cpu0 = await timed(() => readLinuxCpu('/proc', ticks));
  await sleep(options.interval);
  const vm1 = await timed(vmstat);
  const table = await timed(() => readLinuxProcesses({ ticks, pageSize, markers: options.markers }));
  const processes = table.value.map((row) => ({ ...row, cpuBefore: cpu0.value.get(row.pid) ?? null }));
  const meminfo = Object.fromEntries((tryRead('/proc/meminfo') ?? '').split('\n').map((line) => /^(\w+):\s+(\d+)/.exec(line)).filter(Boolean).map(([, key, kib]) => [key, Number(kib) * 1024]));
  const psi = Object.fromEntries(['memory', 'cpu', 'io'].map((name) => [name, tryRead(`/proc/pressure/${name}`)]).map(([name, text]) => [name, text == null ? null : parsePsi(text)]));
  const delta = (key) => (Number.isFinite(vm1.value[key]) && Number.isFinite(vm0.value[key]) ? vm1.value[key] - vm0.value[key] : null);
  const host = {
    memory: { totalBytes: meminfo.MemTotal ?? null, availableBytes: meminfo.MemAvailable ?? null, pressure: psi },
    swap: { totalBytes: meminfo.SwapTotal ?? null, usedBytes: meminfo.SwapTotal == null ? null : meminfo.SwapTotal - meminfo.SwapFree },
    paging: { seconds: round(vm1.at - vm0.at, 2), swapins: delta('pswpin'), swapouts: delta('pswpout'), activePaging: delta('pswpout') == null ? null : delta('pswpout') > 0 },
    pageSize,
  };
  const notes = psi.memory ? [] : ['/proc/pressure is unavailable (kernel without PSI)'];
  return { processes, host, elapsed: table.at - cpu0.at, metric: 'pss', notes };
}

function round(value, digits = 1) {
  return value == null || !Number.isFinite(value) ? null : Number(value.toFixed(digits));
}

/**
 * Groups processes and flags leak candidates. `processes` carry pid, ppid, pgid, uid, ageSeconds, cpuSeconds,
 * cpuBefore (CPU seconds at the first sample, or null), memoryBytes, memorySource, rssBytes, name, command, markers
 * and service; `elapsed` is the seconds between the two CPU samples.
 */
export function analyze(processes, { elapsed, ownUid, selfPid = -1, options }) {
  const seen = processes.filter((p) => p.pid !== selfPid && p.ppid !== selfPid);
  const byPid = new Map(seen.map((p) => [p.pid, p]));
  for (const p of seen) {
    const used = p.cpuBefore == null || p.cpuSeconds == null ? null : p.cpuSeconds - p.cpuBefore;
    p.cpuPercent = used == null || used < 0 || !(elapsed > 0) ? null : (100 * used) / elapsed;
  }
  const declared = new Set([...options.roots.filter((pid) => byPid.has(pid)), ...seen.filter((p) => options.rootNames.includes(p.name.toLowerCase())).map((p) => p.pid)]);
  const ownerPid = (p) => {
    for (const name of options.markerPids) if (/^\d+$/.test(p.markers?.[name] ?? '')) return Number(p.markers[name]);
    return null;
  };
  const keyOf = (p) => {
    for (const name of options.markers) if (p.markers?.[name]) return `marker:${name}=${p.markers[name]}`;
    const visited = new Set();
    let top = p;
    for (let at = p; at && !visited.has(at.pid); at = byPid.get(at.ppid)) {
      visited.add(at.pid);
      if (declared.has(at.pid)) return `root:${at.pid}`;
      top = at;
      if (at.ppid <= 1) break;
    }
    return `root:${top.pid}`;
  };
  const groups = new Map();
  for (const p of seen) {
    const key = keyOf(p);
    p.group = key;
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(p);
  }
  const sum = (members, field) => members.reduce((total, p) => total + (p[field] ?? 0), 0);
  const describe = (key, members) => {
    const marker = key.startsWith('marker:') ? key.slice('marker:'.length) : null;
    const owner = marker ? ownerPid(members[0]) : Number(key.slice('root:'.length));
    const root = byPid.get(owner);
    const group = {
      kind: marker ? 'marker' : declared.has(owner) ? 'declared' : 'top-level',
      ...(marker ? { marker } : {}),
      pid: root?.pid ?? null,
      name: root?.name ?? null,
      command: root?.command ?? null,
      ageSeconds: round(root?.ageSeconds, 0),
      processes: members.length,
      memoryBytes: sum(members, 'memoryBytes'),
      rssBytes: sum(members, 'rssBytes'),
      ...(members.some((p) => p.compressedBytes != null) ? { compressedBytes: sum(members, 'compressedBytes') } : {}),
      cpuPercent: round(sum(members, 'cpuPercent')),
      cpuUnknown: members.filter((p) => p.cpuPercent == null).length,
    };
    return { key, group, members, root };
  };
  const described = [...groups].map(([key, members]) => describe(key, members));
  const rank = options.sort === 'cpu' ? (d) => d.group.cpuPercent ?? 0 : (d) => d.group.memoryBytes;
  described.sort((a, b) => rank(b) - rank(a));
  const tree = (members) => {
    const inGroup = new Set(members.map((p) => p.pid));
    const children = new Map();
    for (const p of members) {
      if (!inGroup.has(p.ppid) || p.ppid === p.pid) continue;
      if (!children.has(p.ppid)) children.set(p.ppid, []);
      children.get(p.ppid).push(p);
    }
    const node = (p) => {
      const kids = (children.get(p.pid) ?? []).sort((a, b) => b.memoryBytes - a.memoryBytes).map(node);
      return { pid: p.pid, name: p.name, memoryBytes: p.memoryBytes, cpuPercent: round(p.cpuPercent), ageSeconds: round(p.ageSeconds, 0), ...(options.commands ? { command: p.command } : {}), ...(kids.length ? { children: kids } : {}) };
    };
    return members.filter((p) => !inGroup.has(p.ppid) || p.ppid === p.pid).sort((a, b) => b.memoryBytes - a.memoryBytes).map(node);
  };
  let topLevel = 0;
  const shown = [];
  const rest = [];
  for (const d of described) {
    const wanted = d.group.kind !== 'top-level' || options.expand.includes(d.group.pid) || topLevel++ < options.top;
    (wanted ? shown : rest).push(d);
  }
  const roots = shown.map((d) => ({ ...d.group, tree: tree(d.members) }));
  const otherRoots = {
    roots: rest.length,
    processes: rest.reduce((total, d) => total + d.group.processes, 0),
    memoryBytes: rest.reduce((total, d) => total + d.group.memoryBytes, 0),
    cpuPercent: round(rest.reduce((total, d) => total + (d.group.cpuPercent ?? 0), 0)),
    ...(rest.length ? { show: `--top ${options.top + rest.length}` } : {}),
  };
  const groupOf = new Map(described.map((d) => [d.key, d]));
  const leakCandidates = [];
  for (const p of seen) {
    if (p.uid !== ownUid) continue;
    const reasons = [];
    if (p.ppid === 1 && p.service === false) {
      const leader = p.pgid !== p.pid ? byPid.get(p.pgid) : undefined;
      const group = p.pgid === p.pid ? '' : leader ? `; its process group leader ${p.pgid} (${leader.name}) is running` : `; its process group leader ${p.pgid} has exited`;
      reasons.push({ code: 'reparented', detail: `parent is PID 1 and it is not a service${group}` });
    }
    for (const name of options.markerPids) {
      const owner = p.markers?.[name];
      if (/^\d+$/.test(owner ?? '') && !byPid.has(Number(owner))) reasons.push({ code: 'ownerExited', detail: `${name}=${owner} names a process that no longer exists` });
    }
    const root = groupOf.get(p.group)?.root;
    const session = groupOf.get(p.group)?.group.kind !== 'top-level';
    if (session && root && root.pid !== p.pid && root.cpuPercent === 0 && p.ageSeconds >= options.oldAfter)
      reasons.push({ code: 'oldChildOfIdleRoot', detail: `${round(p.ageSeconds, 0)} s old; root ${root.pid} (${root.name}) used no CPU in ${round(elapsed, 2)} s` });
    if (reasons.length) leakCandidates.push({ pid: p.pid, ppid: p.ppid, pgid: p.pgid, name: p.name, command: p.command, ageSeconds: round(p.ageSeconds, 0), memoryBytes: p.memoryBytes, cpuPercent: round(p.cpuPercent), group: p.group, reasons });
  }
  leakCandidates.sort((a, b) => b.memoryBytes - a.memoryBytes);
  return {
    roots,
    otherRoots,
    leakCandidates,
    totals: { processes: seen.length, memoryBytes: sum(seen, 'memoryBytes'), cpuPercent: round(sum(seen, 'cpuPercent')) },
    coverage: {
      processes: seen.length,
      cpuUnknown: seen.filter((p) => p.cpuPercent == null).length,
      memoryAsRss: seen.filter((p) => p.memorySource === 'rss' && p.rssBytes > 0).length,
      leakChecksUid: ownUid,
      declaredRoots: [...declared],
    },
  };
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const collect = { darwin: collectMac, linux: collectLinux }[process.platform];
  if (!collect) fail('unsupported-platform', `${process.platform}: this script reads macOS or Linux process tables`, 1);
  let snapshot;
  try {
    snapshot = await collect(options);
  } catch (error) {
    fail('process-table-unreadable', errorMessage(error), 1);
  }
  const cpus = os.availableParallelism();
  const loadAverage = os.loadavg().map((value) => round(value, 2));
  const result = analyze(snapshot.processes, { elapsed: snapshot.elapsed, ownUid: process.getuid(), selfPid: process.pid, options });
  console.log(
    JSON.stringify({
      ok: true,
      platform: process.platform,
      intervalSeconds: round(snapshot.elapsed, 2),
      memoryMetric: snapshot.metric,
      host: { cpus, loadAverage, loadExceedsCpus: loadAverage[0] > cpus, ...snapshot.host },
      ...result,
      notes: snapshot.notes,
    }),
  );
}

if (isMainModule(import.meta.url)) await main();
