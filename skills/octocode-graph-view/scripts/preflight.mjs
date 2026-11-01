#!/usr/bin/env node
// What can run here, and how to enable the rest. Never installs or starts anything.
import { execFileSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { cliOk, octocode } from './lib.mjs';

const has = (cmd, args) => { try { execFileSync(cmd, args, { stdio: 'ignore' }); return true; } catch { return false; } };
const HINTS = {
  cli: 'build the checkout (`node skills-dev/octocode-dev/scripts/dev.mjs build:dev`), set OCTOCODE_CLI, or install `octocode`',
  graph: 'the octocode CLI lacks `graphBuild`/`graphQuery`: rebuild or update it',
  structureSearch: 'listing falls back to `git ls-files`; check `octocode structureSearch` for this workspace (WORKSPACE_ROOT, allowed roots)',
  astSearch: 'AST outlines in the viewer stay empty until `octocode astSearch` works here',
  lspSearch: 'calls stay graph-only (labelled `graph`); the run probes a language server per language and reports why it skipped',
  rsync: 'install rsync: a tree that changes during ingest is graphed from a frozen copy',
  opener: 'open the printed URL by hand (no `open`/`xdg-open`)',
};

async function tool(name, input) {
  try {
    const r = await octocode([name, '--json', JSON.stringify(input)], { partial: true });
    const row = r.results?.[0]?.data;
    return row?.errorCode ? `error: ${row.errorCode}` : 'ok';
  } catch (error) {
    return `error: ${error.message.split('\n')[0].slice(0, 160)}`;
  }
}

export async function preflight() {
  const here = process.cwd();
  const r = { cli: cliOk(['--version']) ? 'ok' : 'missing' };
  r.graph = cliOk(['schema', 'graphBuild']) && cliOk(['schema', 'graphQuery']) ? 'ok' : 'missing';
  r.structureSearch = await tool('structureSearch', { queries: [{ operation: 'files', path: here, entryType: 'file', view: 'paths', limit: 1 }] });
  r.astSearch = cliOk(['schema', 'astSearch']) ? 'ok' : 'missing';
  r.lspSearch = cliOk(['schema', 'lspSearch']) ? 'ok (servers are probed per language during the run)' : 'missing';
  r.rsync = has('rsync', ['--version']) ? 'ok' : 'missing';
  r.opener = process.platform === 'darwin' || process.platform === 'win32' || has('which', ['xdg-open']) ? 'ok' : 'missing';
  return r;
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const r = await preflight();
  const todo = Object.entries(r).filter(([, v]) => !v.startsWith('ok')).map(([k, v]) => ({ layer: k, state: v, enable: HINTS[k] }));
  if (process.argv.includes('--json')) console.log(JSON.stringify({ layers: r, todo }));
  else {
    for (const [k, v] of Object.entries(r)) console.log(`${v.startsWith('ok') ? 'ok     ' : 'MISSING'} ${k.padEnd(16)} ${v}`);
    if (todo.length) console.log(`\nTo enable:\n${todo.map((t) => `- ${t.layer}: ${t.enable}`).join('\n')}`);
    console.log('\nThe viewer loads d3 7.9.0 from cdnjs: it needs network access for that file.');
  }
  process.exit(r.cli === 'ok' && r.graph === 'ok' ? 0 : 1);
}
