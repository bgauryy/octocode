#!/usr/bin/env node
// Merge scan + agent overlay, derive cycles/upward edges/metrics, validate, inline into the one-file template.
import { execFile } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HELP = `Usage: node render.mjs [--scan scan.json] [--model model.json] [--out view.html] [--open] [--check]

Merges the overlay (scheme/architecture-model.json) onto the scan by id, derives metrics,
dependency cycles and upward-layer edges, validates references, and writes one self-contained HTML file.
  --out    default: <dir of scan or model>/architecture.html
  --open   open the result in the default browser
  --check  validate only; print the report, write nothing
stdout: JSON report {ok, out, counts, warnings}. Exit 0 ok, 2 invalid input.
Example: node render.mjs --scan .octocode/architecture-view/scan.json --model .octocode/architecture-view/model.json --open`;

const args = process.argv.slice(2);
if (args.includes('--help') || args.includes('-h')) { console.log(HELP); process.exit(0); }
const flag = (n) => { const i = args.indexOf(n); return i >= 0 ? args[i + 1] : undefined; };
const die = (msg) => { console.error(`render: ${msg}`); process.exit(2); };
const load = (p) => { if (!p) return null; if (!existsSync(p)) die(`file not found: ${p}`); try { return JSON.parse(readFileSync(p, 'utf8')); } catch (e) { die(`invalid JSON in ${p}: ${e.message}`); } };
const scanPath = flag('--scan'), modelPath = flag('--model');
const scan = load(scanPath), over = load(modelPath);
if (!scan && !over) die('pass --scan and/or --model (see --help)');
const TEMPLATE = join(dirname(fileURLToPath(import.meta.url)), '../assets/template.html');
const OUT = resolve(flag('--out') || join(dirname(resolve(scanPath || modelPath)), 'architecture.html'));

const DEFAULT_LAYERS = [
  { id: 'clients', label: 'Clients & hosts', order: 0, description: 'People, IDEs, agents, browsers and other callers outside the system.' },
  { id: 'interface', label: 'Interfaces', order: 1, description: 'Entry points: UI, CLI, MCP servers, HTTP APIs, extensions. Translate requests; own no business rules.' },
  { id: 'application', label: 'Application', order: 2, description: 'Use-case orchestration: sequencing, policies, transactions.' },
  { id: 'domain', label: 'Domain / core', order: 3, description: 'Core logic, models, contracts. Depends on nothing above it.' },
  { id: 'infrastructure', label: 'Infrastructure', order: 4, description: 'Adapters: persistence, network clients, native bindings, processes.' },
  { id: 'data', label: 'Data & external', order: 5, description: 'Databases, caches, files, and third-party services.' },
  { id: 'tooling', label: 'Tooling & tests', order: 6, description: 'Tests, scripts, benchmarks. Excluded from layering rules.', exempt: true },
];
const warnings = [];
const warn = (w) => { if (warnings.length < 200) warnings.push(w); };

// ---- merge ----------------------------------------------------------------------
const meta = { ...(scan?.meta || {}), ...(over?.meta || {}) };
const layers = (over?.layers || scan?.layers || DEFAULT_LAYERS).map((l) => ({ ...l }));
const nodes = new Map((scan?.nodes || []).map((n) => [n.id, { ...n }]));
for (const n of over?.nodes || []) {
  if (!n.id) { warn('overlay node without id skipped'); continue; }
  if (n.remove) { nodes.delete(n.id); continue; }
  nodes.set(n.id, { ...(nodes.get(n.id) || { files: 0, loc: 0 }), guess: false, ...n });
}
const globRe = (g) => new RegExp(`^${g.replace(/[.+^${}()|[\]\\]/g, '\\$&').replace(/\*\*/g, '\u0000').replace(/\*/g, '[^/]*').replace(/\u0000/g, '.*')}$`);
for (const g of over?.exclude || []) { const re = globRe(g); let hit = 0; for (const id of [...nodes.keys()]) if (re.test(id)) { nodes.delete(id); hit++; } if (!hit) warn(`exclude "${g}" matched nothing`); }
// mergeInto chains → final target
const alias = new Map();
const finalId = (id) => { const seen = new Set(); while (alias.has(id) && !seen.has(id)) { seen.add(id); id = alias.get(id); } return id; };
for (const n of [...nodes.values()]) if (n.mergeInto) { if (!nodes.has(n.mergeInto)) { warn(`mergeInto target missing: ${n.id} → ${n.mergeInto}`); continue; } alias.set(n.id, n.mergeInto); }
for (const id of alias.keys()) { const n = nodes.get(id), t = nodes.get(finalId(id)); t.files = (t.files || 0) + (n.files || 0); t.loc = (t.loc || 0) + (n.loc || 0); nodes.delete(id); }
for (const n of nodes.values()) { if (n.parent) { n.parent = finalId(n.parent); if (!nodes.has(n.parent) || n.parent === n.id) delete n.parent; } delete n.mergeInto; }

const edges = new Map();
const ekey = (e) => `${e.source}|${e.target}|${e.kind}`;
const putEdge = (e, fromOverlay) => {
  const s = finalId(e.source), t = finalId(e.target);
  if (s === t) return;
  if (!nodes.has(s) || !nodes.has(t)) { if (fromOverlay) warn(`edge references missing node: ${e.source} → ${e.target}`); return; }
  const k = ekey({ ...e, source: s, target: t });
  if (e.remove) { edges.delete(k); return; }
  const prev = edges.get(k);
  if (prev && !fromOverlay) { prev.weight += e.weight || 1; if (prev.evidence.length < 3) prev.evidence.push(...(e.evidence || []).slice(0, 3 - prev.evidence.length)); return; }
  edges.set(k, { ...(prev || {}), ...e, source: s, target: t, weight: e.weight ?? prev?.weight ?? 1, evidence: e.evidence || prev?.evidence || [] });
};
for (const e of scan?.edges || []) putEdge(e, false);
for (const e of over?.edges || []) putEdge(e, true);

// ---- validate -----------------------------------------------------------------------
const layerIds = new Set(layers.map((l) => l.id));
for (const n of nodes.values()) if (!layerIds.has(n.layer)) { warn(`node ${n.id} has unknown layer "${n.layer}" → "unassigned"`); n.layer = 'unassigned'; }
if ([...nodes.values()].some((n) => n.layer === 'unassigned')) layers.push({ id: 'unassigned', label: 'Unassigned', order: 99, description: 'Nodes whose layer is missing from the layer list.' });
const flows = (over?.flows || scan?.flows || []).map((f) => ({ ...f, steps: (f.steps || []).map((s) => ({ ...s, from: finalId(s.from), to: finalId(s.to) })) }));
for (const f of flows) for (const [i, s] of f.steps.entries()) for (const end of [s.from, s.to]) if (!nodes.has(end)) warn(`flow ${f.id} step ${i + 1} references missing node ${end}`);
let missingEvidence = 0;
const checkEv = (ev) => { if (!meta.root) return; for (const x of ev || []) if (x.path && !existsSync(join(meta.root, x.path))) missingEvidence++; };
for (const n of nodes.values()) checkEv(n.evidence);
for (const e of edges.values()) checkEv(e.evidence);
for (const f of flows) for (const s of f.steps) checkEv(s.evidence);
if (missingEvidence) warn(`${missingEvidence} evidence paths do not exist under meta.root`);
if (!nodes.size) die('model has no nodes after merge/exclude');

// ---- derive -----------------------------------------------------------------------------
const order = new Map(layers.map((l) => [l.id, l.order]));
const exempt = new Set(layers.filter((l) => l.exempt).map((l) => l.id));
const STATIC = new Set(['import', 'call', 'ffi', 'rpc', 'mcp', 'spawn', 'http', 'db', 'stdio']);
const ancestorOf = (id) => { const seen = new Set(); let cur = id; while (nodes.get(cur)?.parent && !seen.has(cur)) { seen.add(cur); cur = nodes.get(cur).parent; } return cur; };
const isAncestor = (a, b) => { let cur = nodes.get(b)?.parent; const seen = new Set(); while (cur && !seen.has(cur)) { if (cur === a) return true; seen.add(cur); cur = nodes.get(cur)?.parent; } return false; };
for (const e of edges.values()) {
  const s = nodes.get(e.source), t = nodes.get(e.target);
  e.upward = !e.allowed && !exempt.has(s.layer) && !exempt.has(t.layer) && order.get(s.layer) > order.get(t.layer) && !isAncestor(e.source, e.target) && !isAncestor(e.target, e.source);
}
// Tarjan SCC on runtime edges, ignoring parent↔child (own files ↔ submodules are cohesive by construction)
const adj = new Map([...nodes.keys()].map((k) => [k, []]));
for (const e of edges.values()) if (STATIC.has(e.kind) && !isAncestor(e.source, e.target) && !isAncestor(e.target, e.source)) adj.get(e.source).push(e.target);
const sccs = []; { let i = 0; const index = new Map(), low = new Map(), stack = [], on = new Set();
  const strong = (v) => { const work = [[v, 0]]; index.set(v, i); low.set(v, i); i++; stack.push(v); on.add(v);
    while (work.length) { const top = work[work.length - 1]; const [u, k] = top; const nb = adj.get(u);
      if (k < nb.length) { top[1]++; const w = nb[k];
        if (!index.has(w)) { index.set(w, i); low.set(w, i); i++; stack.push(w); on.add(w); work.push([w, 0]); }
        else if (on.has(w)) low.set(u, Math.min(low.get(u), index.get(w))); }
      else { work.pop(); if (work.length) { const p = work[work.length - 1][0]; low.set(p, Math.min(low.get(p), low.get(u))); }
        if (low.get(u) === index.get(u)) { const comp = []; let w; do { w = stack.pop(); on.delete(w); comp.push(w); } while (w !== u); if (comp.length > 1) sccs.push(comp); } } } };
  for (const v of adj.keys()) if (!index.has(v)) strong(v); }
const sccOf = new Map(); sccs.forEach((c, i) => c.forEach((id) => sccOf.set(id, i)));
for (const e of edges.values()) e.cycle = STATIC.has(e.kind) && sccOf.has(e.source) && sccOf.get(e.source) === sccOf.get(e.target);
const fanIn = new Map(), fanOut = new Map();
for (const e of edges.values()) { if (e.kind === 'type-import') continue; (fanOut.get(e.source) || fanOut.set(e.source, new Set()).get(e.source)).add(e.target); (fanIn.get(e.target) || fanIn.set(e.target, new Set()).get(e.target)).add(e.source); }
for (const n of nodes.values()) { const ci = fanIn.get(n.id)?.size || 0, co = fanOut.get(n.id)?.size || 0;
  n.metrics = { fanIn: ci, fanOut: co, instability: ci + co ? +(co / (ci + co)).toFixed(2) : 0 }; if (sccOf.has(n.id)) n.metrics.cycle = sccOf.get(n.id) + 1; }

const findings = [...(scan?.findings || []), ...(over?.findings || [])];
if (meta.derivedFindings !== false) {
  sccs.forEach((c, i) => findings.push({ id: `derived-cycle-${i + 1}`, severity: 'warn', confidence: 'candidate', source: 'render: component SCC',
    title: `Dependency cycle across ${c.length} components`, detail: `Runtime dependencies form a strongly connected set. Confirm each closing edge with exact source before calling it a defect.`, nodes: c,
    evidence: [...edges.values()].filter((e) => e.cycle && sccOf.get(e.source) === i).slice(0, 6).map((e) => ({ ...(e.evidence?.[0] || { path: e.source }), note: `${e.source} → ${e.target}` })) }));
  const up = [...edges.values()].filter((e) => e.upward);
  const byPair = new Map(); for (const e of up) { const k = `${nodes.get(e.source).layer}→${nodes.get(e.target).layer}`; (byPair.get(k) || byPair.set(k, []).get(k)).push(e); }
  for (const [k, es] of byPair) findings.push({ id: `derived-upward-${k}`, severity: 'warn', confidence: 'candidate', source: 'render: layer order',
    title: `Upward dependency ${k} (${es.length} edge${es.length > 1 ? 's' : ''})`, detail: 'A lower layer depends on a higher one. Either the layer assignment is wrong, the edge is a legal callback (mark allowed), or the boundary leaks.',
    nodes: [...new Set(es.flatMap((e) => [e.source, e.target]))], evidence: es.slice(0, 6).map((e) => ({ ...(e.evidence?.[0] || { path: e.source }), note: `${e.source} → ${e.target}` })) });
}

// file drill-down: remap node indices to merged ids
const nodeList = [...nodes.values()];
const pos = new Map(nodeList.map((n, i) => [n.id, i]));
let files = null;
if (scan?.files) {
  const oldIds = scan.nodes.map((n) => n.id);
  const list = scan.files.list.map(([p, ni, lang, loc]) => { const id = oldIds[ni] !== undefined ? finalId(oldIds[ni]) : null; return [p, pos.has(id) ? pos.get(id) : -1, lang, loc]; });
  files = { list, edges: scan.files.edges.filter(([a, b]) => list[a][1] >= 0 && list[b][1] >= 0) };
}

const data = { version: 1, meta, layers: layers.sort((a, b) => a.order - b.order), nodes: nodeList, edges: [...edges.values()], flows, findings, files };
const counts = { nodes: nodeList.length, edges: data.edges.length, flows: flows.length, findings: findings.length, cycles: sccs.length, upward: data.edges.filter((e) => e.upward).length, guessed: nodeList.filter((n) => n.guess).length, files: files?.list.length || 0 };
if (args.includes('--check')) { console.log(JSON.stringify({ ok: true, counts, warnings }, null, 1)); process.exit(0); }

const tpl = readFileSync(TEMPLATE, 'utf8');
const MARK = '/*__ARCH_DATA__*/null';
if (!tpl.includes(MARK)) die(`template marker ${MARK} missing in ${TEMPLATE}`);
const json = JSON.stringify(data).replace(/</g, '\\u003c').replace(/\u2028/g, '\\u2028').replace(/\u2029/g, '\\u2029');
mkdirSync(dirname(OUT), { recursive: true });
writeFileSync(OUT, tpl.replace(MARK, () => json).replace('<title>Architecture</title>', () => `<title>${String(meta.name || 'Architecture').replace(/[<&]/g, '')} · Architecture</title>`));
console.log(JSON.stringify({ ok: true, out: OUT, bytes: Buffer.byteLength(json), counts, warnings }, null, 1));
if (args.includes('--open')) {
  const [cmd, a] = process.platform === 'darwin' ? ['open', [OUT]] : process.platform === 'win32' ? ['cmd', ['/c', 'start', '', OUT]] : ['xdg-open', [OUT]];
  execFile(cmd, a, (err) => err && console.error(`render: could not open browser (${err.message}); open ${OUT} manually`));
}
