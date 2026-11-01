#!/usr/bin/env node
// Per-crate syntax-graph call edges compared with an LSP cross-check (an octocode-graph-view
// data.json whose `calls` rows carry `via: lsp | both | graph`). Builds each crate with refresh,
// follows every `witnesses` page (calls, confidence low) and reports how many of the peer's
// LSP-only and shared edges the graph has, plus graph edges from probed functions LSP never saw.
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isMainModule } from './lib/entrypoint.mjs';

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
const HELP = `Usage: lsp-crosscheck.mjs --peer DATA.json --crates DIR,... --out FILE [--bin PATH]

  --peer    graph-view data.json with the LSP cross-check (\`calls[].via\`)
  --crates  crate directories, comma-separated; each directory's name prefixes its node IDs
            (the peer's convention, e.g. runtime/src/x.rs#f)
  --out     result JSON: summary, builds, recovered, lostBoth, probedGraphEdgesNotInLsp, edges
  --bin     native octocode binary or launcher .js (default: OCTO_BIN, else the repo launcher)`;

function parseArgs(argv) {
  const opts = {};
  for (let i = 0; i < argv.length; i++) {
    const flag = argv[i];
    if (flag === '--help' || flag === '-h') opts.help = true;
    else if (['--peer', '--crates', '--out', '--bin'].includes(flag)) {
      const value = argv[++i];
      if (value === undefined) throw new Error(`${flag} needs a value`);
      opts[flag.slice(2)] = value;
    } else throw new Error(`unknown argument ${flag}\n\n${HELP}`);
  }
  if (!opts.help && !(opts.peer && opts.crates && opts.out)) throw new Error(HELP);
  return opts;
}

// Pure comparison of graph edges ({from, to}, crate-prefixed) with the peer's call rows.
export function crosscheck(edges, peerCalls) {
  const key = (c) => `${c.source}->${c.target}`;
  const graphKeys = new Set(edges.map((e) => `${e.from}->${e.to}`));
  const lspOnly = peerCalls.filter((c) => c.via === 'lsp');
  const both = peerCalls.filter((c) => c.via === 'both');
  const sameCrate = (c) => c.source.split('/')[0] === c.target.split('/')[0];
  const probed = new Set([...lspOnly, ...both].map((c) => c.source));
  const lspKeys = new Set([...lspOnly, ...both].map(key));
  const recovered = lspOnly.filter((c) => graphKeys.has(key(c)));
  const probedGraphEdgesNotInLsp = edges.filter((e) => probed.has(e.from) && !lspKeys.has(`${e.from}->${e.to}`));
  return {
    summary: {
      edges: edges.length,
      lspOnly: { total: lspOnly.length, sameCrate: lspOnly.filter(sameCrate).length, recovered: recovered.length },
      both: { total: both.length, sameCrate: both.filter(sameCrate).length, stillLinked: both.filter((c) => graphKeys.has(key(c))).length },
      probedGraphEdgesNotInLsp: probedGraphEdgesNotInLsp.length,
    },
    recovered,
    lostBoth: both.filter((c) => sameCrate(c) && !graphKeys.has(key(c))),
    probedGraphEdgesNotInLsp,
  };
}

function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.help) return console.log(HELP);
  const bin = resolve(opts.bin ?? process.env.OCTO_BIN ?? join(REPO, 'packages/octocode/out/octocode.js'));
  const octo = (tool, input) => {
    const args = [tool, '--json', JSON.stringify(input)];
    const r = bin.endsWith('.js') ? spawnSync('node', [bin, ...args], { maxBuffer: 1 << 30 }) : spawnSync(bin, args, { maxBuffer: 1 << 30 });
    const j = JSON.parse(r.stdout.toString());
    return { ...(j.results?.[0]?.data ?? {}), responsePagination: j.responsePagination };
  };
  const edges = [];
  const builds = {};
  for (const dir of opts.crates.split(',').filter(Boolean).map((d) => resolve(d))) {
    const crate = basename(dir);
    const b = octo('graphBuild', { queries: [{ path: dir, refresh: true }] });
    builds[crate] = { id: b.id, calls: b.calls, error: b.error };
    if (!b.id) continue;
    let page = octo('graphQuery', { queries: [{ operation: 'witnesses', path: dir, graph: b.id, edges: ['calls'], confidence: 'low', limit: 1000 }], responseChars: 50000 });
    for (;;) {
      for (const w of page.rows ?? []) edges.push({ from: `${crate}/${w.from}`, to: `${crate}/${w.to}`, line: w.line, detail: w.detail, confidence: w.confidence, test: w.test });
      const next = page.responsePagination?.next?.input ?? page.next?.nextPage?.input;
      if (!next || page.error) break;
      page = octo('graphQuery', next);
    }
  }
  const peer = JSON.parse(readFileSync(resolve(opts.peer), 'utf8'));
  const result = crosscheck(edges, peer.calls ?? []);
  writeFileSync(resolve(opts.out), JSON.stringify({ bin, ...result, builds, edges }, null, 1));
  console.log(JSON.stringify(result.summary, null, 1));
}

if (isMainModule(import.meta.url)) {
  try {
    main();
  } catch (e) {
    console.error(e.message);
    process.exitCode = 2;
  }
}
