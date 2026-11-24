#!/usr/bin/env node
// End-to-end checks of the built package against the real Octocode engine, through every interface: the CLI
// (build, query, refresh, diff, render, wiki, html), the live viewer in replay, both MCP servers, the library
// and the published tarball. Unit tests replace the engine with fakes; this is the one place it runs for real.
//
//   node scripts/e2e.mjs [--target <dir>] [--keep]
//
// Run from anywhere: commands run in the repository root (the engine's workspace). --target graphs another
// directory under it (default: this package). Needs `yarn build` and a built engine. Prints PASS/FAIL per check and
// exits 1 when any fails; --keep leaves the scratch directory for inspection.
import { spawn, spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
} from 'node:fs';
import { join, resolve } from 'node:path';
import { parseArgs } from 'node:util';

const PKG = resolve(new URL('..', import.meta.url).pathname);
const REPO = resolve(PKG, '../..');
const BIN = join(PKG, 'bin/octocode-graph.mjs');
const { values } = parseArgs({
  options: {
    target: { type: 'string', default: 'packages/octocode-graph' },
    keep: { type: 'boolean', default: false },
  },
});
// Scratch lives in the repository's scratch space, removed at the end unless --keep.
const SCRATCH = join(REPO, '.octocode', 'tmp', 'octocode-graph');
mkdirSync(SCRATCH, { recursive: true });
const scratch = mkdtempSync(join(SCRATCH, 'e2e-'));
const RUN = join(scratch, 'run');
const results = [];

async function check(name, fn) {
  const t0 = Date.now();
  try {
    const note = await fn();
    results.push({ name, ok: true, ms: Date.now() - t0 });
    process.stdout.write(
      `PASS ${name}${note ? `  ${String(note).slice(0, 140)}` : ''}\n`
    );
  } catch (error) {
    results.push({
      name,
      ok: false,
      ms: Date.now() - t0,
      error: String(error?.message ?? error),
    });
    process.stdout.write(
      `FAIL ${name}  ${String(error?.message ?? error).slice(0, 400)}\n`
    );
  }
}

function graph(args, { input, ok = true } = {}) {
  const r = spawnSync(process.execPath, [BIN, ...args], {
    cwd: REPO,
    encoding: 'utf8',
    input,
    maxBuffer: 1e9,
    env: { ...process.env, GRAPH_VIEW_NO_OPEN: '1' },
  });
  if (ok && r.status !== 0)
    throw new Error(
      `octocode-graph ${args.join(' ')} exited ${r.status}: ${r.stderr.slice(0, 300)}`
    );
  return r;
}
const json = text => JSON.parse(text);
const assert = (cond, message) => {
  if (!cond) throw new Error(message);
};

// A JSON-RPC session with an MCP server over stdio.
function mcp(command) {
  const child = spawn(process.execPath, [BIN, command], {
    cwd: REPO,
    stdio: ['pipe', 'pipe', 'ignore'],
    env: { ...process.env, GRAPH_VIEW_NO_OPEN: '1' },
  });
  let buf = '',
    id = 0;
  const waiting = new Map();
  child.stdout.on('data', d => {
    buf += d;
    for (let i; (i = buf.indexOf('\n')) >= 0;) {
      const line = buf.slice(0, i);
      buf = buf.slice(i + 1);
      if (!line.trim()) continue;
      const m = JSON.parse(line);
      waiting.get(m.id)?.(m);
    }
  });
  const call = (method, params) =>
    new Promise(ok => {
      const n = ++id;
      waiting.set(n, ok);
      child.stdin.write(
        `${JSON.stringify({ jsonrpc: '2.0', id: n, method, params })}\n`
      );
    });
  return {
    call,
    async start() {
      await call('initialize', {
        protocolVersion: '2025-06-18',
        capabilities: {},
        clientInfo: { name: 'e2e', version: '1' },
      });
      child.stdin.write(
        `${JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' })}\n`
      );
    },
    async tool(name, args) {
      const r = (await call('tools/call', { name, arguments: args })).result;
      if (r.isError)
        throw new Error(`${name}: ${r.content[0].text.slice(0, 300)}`);
      return r.content[0].text;
    },
    close: () => child.kill(),
  };
}

// 1. A real multi-chunk build and the run directory it leaves.
await check('build', () => {
  const r = graph(['build', values.target, '--out', RUN, '--lsp', 'smart']);
  const s = json(r.stdout);
  assert(s.files > 0 && s.out === RUN, 'no files built');
  return `${s.files} files, ${s.calls} calls, ${(s.ms / 1000).toFixed(1)} s, LSP ${s.lsp}`;
});
await check('run-directory', () => {
  for (const f of ['data.json', 'events.jsonl', 'brief.txt', 'wiki/index.md'])
    assert(existsSync(join(RUN, f)), `missing ${f}`);
  // One copy of the graph: no checkpoint left, one brief, gzipped chunk results, paths stored once.
  assert(!existsSync(join(RUN, 'stitched.json')), 'stitched.json kept');
  assert(!existsSync(join(RUN, 'brief-full.txt')), 'second brief');
  const chunks = readdirSync(join(RUN, 'chunks'));
  assert(
    chunks.every(c => existsSync(join(RUN, 'chunks', c, 'data.json.gz'))),
    'chunk result not gzipped'
  );
  const D = json(readFileSync(join(RUN, 'data.json'), 'utf8'));
  assert(
    typeof D.section === 'string' && !D.files.some(f => 'loc' in f),
    'derived locations stored'
  );
  return `${chunks.length} chunks`;
});
await check('one-command', () => {
  const pkg = json(readFileSync(join(PKG, 'package.json'), 'utf8'));
  const bin =
    typeof pkg.bin === 'string'
      ? { [pkg.name.split('/').pop()]: pkg.bin }
      : pkg.bin;
  assert(
    JSON.stringify(Object.keys(bin)) === '["octocode-graph"]',
    `bins: ${Object.keys(bin).join(', ')}`
  );
});

// 2. Saved-graph queries, natural phrasing, batches, ambiguity.
const queries = [
  'search runQuery',
  'search rnQry',
  'find wiki',
  'about src/runtime/query-api.ts',
  'about runQuery',
  'callers of runQuery',
  'impact src/runtime/query-index.ts depth 2',
  'deps src/runtime/pipeline.ts',
  'path src/cli.ts src/runtime/query-index.ts',
  'top pagerank 5',
  'critical functions 5',
  'critical points',
  'order src/runtime/query-api.ts --imports',
  'trace src/runtime/query-api.ts IMPORTS+',
  'files src/runtime/lib.ts:10',
  'community src/runtime/lib.ts',
];
for (const q of queries)
  await check(`query ${q}`, () => {
    const r = json(graph(['query', RUN, q, '--json']).stdout);
    assert(r.ok, r.error);
    return r.summary;
  });
await check('query next calls', () => {
  const about = json(graph(['query', RUN, 'about runQuery', '--json']).stdout);
  assert(about.next?.read?.tool === 'localFetch', 'about has no next.read');
  const callers = json(
    graph(['query', RUN, 'callers runQuery', '--json']).stdout
  );
  assert(
    callers.next?.confirm?.tool === 'lspSearch',
    'callers has no next.confirm'
  );
});
await check('query --stdin', () => {
  const out = graph(['query', RUN, '--stdin', '--json'], {
    input: 'search graphBuild\nabout src/index.ts\n',
  }).stdout;
  assert(out.trim().split('\n').length === 2, 'one result per line');
});
await check('query ambiguity lists candidates', () => {
  const r = json(
    graph(['query', RUN, 'about main', '--json'], { ok: false }).stdout
  );
  assert(!r.ok && r.candidates?.length, 'no candidates');
});

// 2b. Links beside the code graph: docs and manifests, then an agent's link verified against its quotes.
await check('links', () => {
  const r = json(graph(['query', RUN, 'links', '--json']).stdout);
  assert(r.ok && r.rows.length > 0, 'no links');
  const u = json(graph(['query', RUN, 'links --unresolved', '--json']).stdout);
  assert(u.ok, u.error);
  return `${r.summary}; ${u.summary}`;
});
await check('link add', () => {
  const root = resolve(REPO, values.target);
  const at = (file, quote) =>
    readFileSync(join(root, file), 'utf8')
      .split('\n')
      .findIndex(l => l.includes(quote)) + 1;
  const readme =
    "import { buildGraph, openGraph, refreshGraph } from '@octocodeai/octocode-graph';";
  const code = 'export async function buildGraph(';
  const link = {
    relation: 'documents',
    source: 'README.md',
    target: 'src/runtime/api.ts#buildGraph',
    claim: 'the README shows buildGraph from the library',
    anchors: [
      { path: 'README.md', line: at('README.md', readme), quote: readme },
      {
        path: 'src/runtime/api.ts',
        line: at('src/runtime/api.ts', code),
        quote: code,
      },
    ],
    proposedBy: 'e2e',
  };
  const stored = json(graph(['link', RUN, 'add', JSON.stringify(link)]).stdout);
  assert(
    stored.check === 'ok' && stored.confidence === 'likely',
    JSON.stringify(stored)
  );
  const wrong = graph(
    [
      'link',
      RUN,
      'add',
      JSON.stringify({
        ...link,
        anchors: [{ ...link.anchors[0], line: 1 }, link.anchors[1]],
      }),
    ],
    { ok: false }
  );
  assert(
    wrong.status === 1 && json(wrong.stdout).refused === 'quote-mismatch',
    'a wrong quote was stored'
  );
  const listed = json(
    graph(['query', RUN, 'links --rule agent', '--json']).stdout
  );
  assert(listed.rows.length === 1, 'the agent link is not in the graph');
  return stored.id;
});

// 3. Refresh with no edits, then the diff it leaves.
await check('refresh', () => {
  const r = json(graph(['refresh', RUN]).stdout);
  assert(Array.isArray(r.chunks), 'no chunk list');
  return `rebuilt ${r.chunks.length} chunks in ${r.ms} ms`;
});
await check('diff', () => graph(['diff', RUN]).stdout.split('\n')[0]);

// 4. Brief, wiki, standalone HTML.
await check('render', () =>
  graph(['render', '--data', join(RUN, 'data.json')]).stdout.trim()
);
await check(
  'wiki read',
  () => graph(['wiki', 'read', RUN, 'index']).stdout.split('\n')[0]
);
await check('wiki lint', () => {
  json(graph(['wiki', 'lint', RUN, '--json'], { ok: false }).stdout);
});
await check('html', () => {
  const file = join(scratch, 'graph.html');
  graph(['html', RUN, file]);
  const text = readFileSync(file, 'utf8');
  assert(
    text.length > 100_000 && text.includes('saved-worker'),
    'html without the saved worker'
  );
  return `${(text.length / 1e6).toFixed(1)} MB`;
});

// 5. The viewer in replay: page, script, token gate, a query.
await check('serve replay', async () => {
  const child = spawn(
    process.execPath,
    [BIN, 'serve', '--replay', RUN, '--no-open'],
    {
      cwd: REPO,
      stdio: ['ignore', 'pipe', 'ignore'],
    }
  );
  try {
    const line = await new Promise((ok, fail) => {
      let buf = '';
      child.stdout.on('data', d => {
        buf += d;
        if (buf.includes('\n')) ok(buf.split('\n')[0]);
      });
      child.once('exit', code => fail(new Error(`serve exited ${code}`)));
    });
    const url = new URL(json(line).url);
    const token = url.searchParams.get('t');
    const base = `${url.protocol}//${url.host}`;
    const page = await fetch(url);
    const script = await fetch(`${base}/live.js`);
    const noToken = await fetch(`${base}/`);
    const answer = await (
      await fetch(`${base}/query?t=${token}`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ query: 'search runQuery' }),
      })
    ).json();
    assert(
      page.status === 200 && script.status === 200,
      `page ${page.status}, script ${script.status}`
    );
    assert(noToken.status === 404, `no token answered ${noToken.status}`);
    assert(answer.ok, 'query failed');
  } finally {
    child.kill();
  }
});

// 6. MCP: the native graph tools, and the saved-graph loop.
await check('mcp: saved-graph tools beside the engine tools', async () => {
  const s = mcp('mcp');
  try {
    await s.start();
    const list = (await s.call('tools/list', {})).result.tools.map(t => t.name);
    for (const name of [
      'graphQuery',
      'graphBuild',
      'build_graph',
      'query_graph',
      'link_graph',
    ])
      assert(list.includes(name), `tools: ${list}`);
    assert(!list.includes('open_graph'), 'a viewer tool without a viewer');
    await s.tool('query_graph', { result: RUN, query: 'links' });
    await s.tool('graphQuery', {
      queries: [{ path: values.target, operation: 'stats' }],
    });
    return list.join(',');
  } finally {
    s.close();
  }
});
await check('mcp channel loop', async () => {
  const s = mcp('channel');
  try {
    await s.start();
    const built = await s.tool('build_graph', {
      path: join(values.target, 'src/runtime'),
      lsp: 'off',
    });
    const out = /built (\S+) in/.exec(built)[1];
    try {
      await s.tool('query_graph', {
        result: out,
        query: 'callers of runQuery',
      });
      await s.tool('refresh_graph', { result: out });
      return (await s.tool('diff_graph', { result: out })).split('\n')[0];
    } finally {
      rmSync(out, { recursive: true, force: true });
    }
  } finally {
    s.close();
  }
});

// 7. The library, as installed.
await check('library', async () => {
  const lib = await import(join(PKG, 'dist/index.js'));
  const { runGraphCli } = await import(join(PKG, 'dist/cli.js'));
  for (const name of [
    'buildGraph',
    'openGraph',
    'refreshGraph',
    'runQuery',
    'diffGraphs',
    'graphQuery',
    'startMcp',
  ])
    assert(typeof lib[name] === 'function', `missing export ${name}`);
  assert(typeof runGraphCli === 'function', 'missing runGraphCli');
  const g = lib.openGraph(RUN);
  const r = g.query('about src/runtime/query-api.ts');
  assert(r.ok && r.next?.read, 'about through the library');
  assert(g.diffFrom(g).review.length === 0, 'a graph differs from itself');
});

// 8. The tarball carries the runtime and nothing from src or tests.
await check('pack', () => {
  const r = spawnSync('npm', ['pack', '--dry-run', '--json'], {
    cwd: PKG,
    encoding: 'utf8',
    maxBuffer: 1e9,
  });
  const files = json(r.stdout)[0].files.map(f => f.path);
  const need = [
    'dist/index.js',
    'dist/cli.js',
    'dist/index.d.ts',
    'dist/runtime/build.js',
    'dist/runtime/query.js',
    'dist/runtime/serve.js',
    'dist/runtime/refresh.js',
    'dist/runtime/channel.js',
    'dist/assets/live.html',
    'dist/browser/offline.js',
    'dist/scheme/model.json',
    'skills/octocode-graph/SKILL.md',
    'bin/octocode-graph.mjs',
  ];
  const missing = need.filter(n => !files.includes(n));
  const leaked = files.filter(
    n => n.startsWith('src/') || n.startsWith('tests/')
  );
  assert(
    !missing.length && !leaked.length,
    `missing ${missing.join(', ')} leaked ${leaked.slice(0, 5).join(', ')}`
  );
  return `${files.length} files`;
});

const failed = results.filter(r => !r.ok);
process.stdout.write(
  `${results.length - failed.length}/${results.length} passed${values.keep ? `; scratch ${scratch}` : ''}\n`
);
if (!values.keep) rmSync(scratch, { recursive: true, force: true });
process.exitCode = failed.length ? 1 : 0;
