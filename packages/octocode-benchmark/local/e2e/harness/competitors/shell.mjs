// The shell side of a competitor task: the expert (or unfiltered) recipe run
// step by step in bash (rg/sed/git/gh/ast-grep/npm), its stdout parsed into
// line/file evidence and judged by the shared truth check.
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { REPOS, REPOS_REL, ROOT } from '../paths.mjs';
import { emptyEvidence, evaluate } from './truth.mjs';
import { relativize } from './workspace.mjs';

/** `rg` for the recipes: RG_BIN, else rg on PATH, else Claude Code's embedded rg. */
function rgPreamble() {
  if (process.env.RG_BIN) return `rg() { "${process.env.RG_BIN}" "$@"; }`;
  if (spawnSync('bash', ['-c', 'type -P rg'], { encoding: 'utf8' }).stdout.trim()) return '';
  const claude = process.env.CLAUDE_CODE_EXECPATH;
  if (claude && fs.existsSync(claude)) return `rg() { (exec -a rg "${claude}" "$@"); }`;
  throw new Error('ripgrep not found: set RG_BIN');
}

// Set by prepareShell(), so importing this module has no side effects (`--self-test` needs no rg).
let preamble;
let scratch;
/** Resolve `rg` (throws when there is none) and make the recipes' scratch directory (`$CT`). */
export function prepareShell() {
  preamble = rgPreamble();
  scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-competitors-'));
}
/** Remove the recipes' scratch directory. */
export const removeShellScratch = () => fs.rmSync(scratch, { recursive: true, force: true });

function sh(step, ws = '') {
  const started = performance.now();
  const cmd = relativize(step.sh, ws, ['$ROOT/$T', '$T']);
  const run = spawnSync('bash', ['-e', '-o', 'pipefail', '-c', `${preamble}\n${cmd}`], {
    cwd: path.join(ROOT, ws), encoding: 'utf8', maxBuffer: 256 << 20, timeout: 120_000,
    env: { ...process.env, T: ws ? REPOS : REPOS_REL, CT: scratch, ROOT, GH_PAGER: '', NO_COLOR: '1' },
  });
  const ms = Math.round(performance.now() - started);
  const stdout = run.stdout ?? '';
  const stderr = run.stderr ?? '';
  return { cmd, ms, bytes: Buffer.byteLength(stdout + stderr, 'utf8'), exit: run.status, expectedExitCodes: step.expectedExitCodes ?? [0], stdout, stderr, error: run.error?.message };
}

/** One stdout line into evidence, per parse mode. */
const PARSERS = {
  rg: (l, ev) => {
    const m = /^(.+?)[:-](\d+)[:-]/.exec(l);
    if (m) { ev.pairs.push({ file: m[1], line: +m[2] }); ev.pairFiles.add(m[1]); } else if (!/\s/.test(l.trim())) ev.files.add(l.trim());
  },
  numbered: (l, ev, step) => { const m = /^\s*(\d+)[:\t│\- ]/.exec(l); if (m) ev.pairs.push({ file: step.file, line: +m[1] }); },
  files: (l, ev) => { ev.files.add(l.trim()); ev.fileRows.push(l.trim()); },
  filecolon: (l, ev) => { const m = /^(\S+?):\s/.exec(l); if (m) ev.files.add(m[1]); },
  diff: (l, ev) => { const m = /^diff --git a\/(\S+) b\//.exec(l); if (m) ev.files.add(m[1]); },
  ghsearch: (l, ev) => { const m = /^[^\s:]+\/[^\s:]+:([^:]+):/.exec(l); if (m) ev.files.add(m[1]); },
};

/** Line/file evidence from one shell step's stdout, by the step's parse mode. */
function shellEvidence(step, out, ev) {
  const parse = Object.hasOwn(PARSERS, step.parse) ? PARSERS[step.parse] : null;
  if (parse) for (const l of out.stdout.split('\n').filter(x => x.length)) parse(l, ev, step);
  if (step.parse === 'sed') out.stdout.split('\n').slice(0, -1).forEach((_, i) => ev.pairs.push({ file: step.file, line: step.start + i }));
  ev.text += `${out.stdout}\n${out.stderr}\n`;
}

/** Nonzero result statuses (such as rg no-match) require an explicit recipe policy. */
export function shellSide(recipe, truth, ws) {
  const ev = emptyEvidence();
  const steps = [];
  for (const step of recipe) {
    const out = sh(step, ws);
    steps.push(out);
    shellEvidence(step, out, ev);
  }
  const verdict = evaluate(truth, ev);
  const errors = steps.filter(step => step.error || !step.expectedExitCodes.includes(step.exit));
  const fails = [...verdict.fails, ...errors.map(step => step.error ?? `shell exit ${step.exit}: ${step.cmd}`)];
  return { ok: fails.length === 0, fails, calls: steps.length, bytes: steps.reduce((a, s) => a + s.bytes, 0), ms: steps.reduce((a, s) => a + s.ms, 0), steps };
}
