// Process probes for the soak: RSS of a server (and its descendants) and a stack sample of a hung process.
import { execFile, execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

export function rssKb(pid) {
  try { return Number(execFileSync('ps', ['-o', 'rss=', '-p', String(pid)], { encoding: 'utf8' }).trim()) || null; } catch { return null; }
}

/** RSS of the process plus its descendants (LSP servers etc.), informational. */
export function treeRssKb(pid) {
  try {
    const rows = execFileSync('ps', ['-A', '-o', 'pid=,ppid=,rss='], { encoding: 'utf8' }).trim().split('\n').map(l => l.trim().split(/\s+/).map(Number));
    const kids = new Map();
    for (const [p, pp] of rows) kids.set(pp, [...(kids.get(pp) ?? []), p]);
    const rss = new Map(rows.map(([p, , r]) => [p, r]));
    let total = 0; const stack = [pid];
    while (stack.length) { const p = stack.pop(); total += rss.get(p) ?? 0; stack.push(...(kids.get(p) ?? [])); }
    return total || null;
  } catch { return null; }
}

/** A `ps` snapshot plus the kernel stack where /proc has one (non-macOS). */
function psSnapshot(pid) {
  let text = '';
  try { text = execFileSync('ps', ['-o', 'pid,ppid,rss,stat,etime,command', '-p', String(pid)], { encoding: 'utf8' }); } catch (error) { text = error.message; }
  try { text += `\n${fs.readFileSync(`/proc/${pid}/stack`, 'utf8')}`; } catch { /* no /proc stack on this platform */ }
  return text;
}

/** `sample <pid> 5` (macOS) or a `ps` snapshot elsewhere, into `out.samples`; resolves the file relative to `out.dir`. */
export function captureStack(pid, name, out) {
  const file = path.join(out.samples, `${name}.txt`);
  return new Promise(resolve => {
    if (process.platform === 'darwin') {
      execFile('sample', [String(pid), '5', '-file', file], { timeout: 30_000 }, error => {
        if (error && !fs.existsSync(file)) fs.writeFileSync(file, `sample ${pid} failed: ${error.message}\n`);
        resolve(path.relative(out.dir, file));
      });
    } else {
      fs.writeFileSync(file, psSnapshot(pid));
      resolve(path.relative(out.dir, file));
    }
  });
}
