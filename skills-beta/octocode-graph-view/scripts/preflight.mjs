#!/usr/bin/env node
// What can run here, and how to enable the rest. Never installs anything.
import { execFile, execFileSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { HINTS } from './hints.mjs';
import { chrome, cliOk, octocode } from './lib.mjs';

const ok = (f) => { try { f(); return true; } catch { return false; } };
const run = (cmd, args) => ok(() => execFileSync(cmd, args, { stdio: 'ignore' }));

export async function preflight() {
  const host = process.env.OLLAMA_HOST?.replace(/^(?!http)/, 'http://') ?? 'http://localhost:11434';
  const r = {};
  r.graph = cliOk(['graph', '--help']) ? 'ok' : 'missing';
  r.chrome = ok(chrome) ? 'ok' : 'missing';
  r.ast = cliOk(['astSearch', '--help']) ? 'ok' : 'missing';
  try {
    const tags = await (await fetch(`${host}/api/tags`)).json();
    r.ocr = tags.models.some((m) => m.name.startsWith('glm-ocr')) ? 'ok' : 'model-missing';
  } catch {
    r.ocr = 'ollama-missing';
  }
  try {
    const c = await octocode(['clasify', JSON.stringify({ queries: [{ operation: 'models' }] }), '--json']);
    r.clasify = c.results?.[0]?.data?.models?.length ? 'ok' : 'missing';
  } catch {
    r.clasify = 'missing';
  }
  r.history = run('git', ['rev-parse', '--show-toplevel']) ? 'ok' : 'missing';
  r.rsync = run('rsync', ['--version']) ? 'ok' : 'missing';
  r.lsp = ['rust-analyzer', 'typescript-language-server', 'pyright-langserver', 'gopls'].filter((s) => run('which', [s])).join(',') || 'none found (opt-in)';
  return r;
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const r = await preflight();
  const todo = Object.entries(r).filter(([k, v]) => !['ok'].includes(v) && k !== 'lsp').map(([k, v]) => ({ layer: k, state: v, enable: HINTS[k] }));
  if (process.argv.includes('--json')) console.log(JSON.stringify({ layers: r, todo }));
  else {
    for (const [k, v] of Object.entries(r)) console.log(`${k === 'lsp' ? 'info   ' : v === 'ok' ? 'ok     ' : 'MISSING'} ${k.padEnd(8)} ${v === 'ok' ? '' : v}`);
    if (todo.length) console.log(`\nTo enable:\n${todo.map((t) => `- ${t.layer}: ${t.enable}`).join('\n')}`);
    console.log('\nRuns without the missing layers; the report lists each skipped layer.');
  }
  process.exit(r.graph === 'ok' ? 0 : 1);
}
