import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const render = join(dirname(fileURLToPath(import.meta.url)), 'render.mjs');
const file = (id, fanIn, fanOut) => ({ id, label: id.split('/').pop(), group: id.split('/')[0], fanIn, fanOut });
const data = {
  section: '/repo',
  frozen: false,
  snapshot: { id: 's', root: '/repo', counts: { nodes: 4, edges: 3, nodesByKind: { file: 3 } }, linkage: 0.9 },
  files: [file('ui/app.ts', 0, 1), file('core/svc.ts', 1, 1), file('data/db.ts', 1, 1)],
  imports: [
    { source: 'ui/app.ts', target: 'core/svc.ts', line: 3 },
    { source: 'core/svc.ts', target: 'data/db.ts', line: 5 },
    { source: 'data/db.ts', target: 'ui/app.ts', line: 9 },
  ],
  symbols: [],
  calls: [],
  fnFiles: [],
  legends: { files: ['Files: 3', 'Imports: 3'], functions: ['Files: 0', 'Functions: 0', 'Calls: 0'] },
  insights: { hubs: [], health: { files: 2, filesInCycles: 2, averageReach: 1.5 }, findings: {}, issues: [], cycles: { total: 1, top: [] }, groups: [], mutualGroups: [] },
  hubFiles: [],
  tokens: { sourceBytes: 4000, sourceFiles: 3 },
};
const layers = [
  { id: 'interface', label: 'Interface', order: 1, match: ['ui/**'] },
  { id: 'application', label: 'Application', order: 2, match: ['core/**'] },
  { id: 'data', label: 'Data', order: 3, match: ['data/**'] },
];

function run(model, verify) {
  const dir = mkdtempSync(join(tmpdir(), 'gv-'));
  writeFileSync(join(dir, 'data.json'), JSON.stringify(data));
  if (model) writeFileSync(join(dir, 'model.json'), JSON.stringify(model));
  if (verify) writeFileSync(join(dir, 'verify.json'), JSON.stringify(verify));
  const r = spawnSync(process.execPath, [render, '--data', join(dir, 'data.json')], { encoding: 'utf8' });
  assert.equal(r.status, 0, r.stderr);
  const html = readFileSync(join(dir, 'report.html'), 'utf8');
  const embedded = JSON.parse(html.match(/<script id="data" type="application\/json">([\s\S]*?)<\/script>/)[1]);
  return { r, html, embedded, brief: readFileSync(join(dir, 'brief-full.txt'), 'utf8'), short: readFileSync(join(dir, 'brief.txt'), 'utf8') };
}

test('renders without an overlay and keeps the template untouched', () => {
  const { embedded, html } = run(null);
  assert.equal(embedded.model, undefined);
  assert.match(html, /d3\.min\.js/);
  assert.equal(embedded.tokens.rows.length, 5);
});

test('derives upward imports from layer order', () => {
  const { embedded, brief } = run({ layers });
  assert.deepEqual(embedded.model.upward.map((e) => `${e.source}>${e.target}`), ['data/db.ts>ui/app.ts']);
  assert.equal(embedded.model.layerOf['core/svc.ts'], 'application');
  assert.match(brief, /upward_imports=1/);
});

test('allowed and exempt layers suppress upward findings', () => {
  const allowed = run({ layers, allowed: [{ source: 'data/db.ts', target: 'ui/app.ts', reason: 'callback' }] });
  assert.equal(allowed.embedded.model.upward.length, 0);
  const exempt = run({ layers: layers.map((l) => (l.id === 'data' ? { ...l, exempt: true } : l)) });
  assert.equal(exempt.embedded.model.upward.length, 0);
});

test('warns on unknown ids, missing evidence and unplaced files', () => {
  const { r, embedded } = run({
    layers: layers.slice(0, 2),
    edges: [{ source: 'ui/app.ts', target: 'ext:nowhere', kind: 'http', evidence: {} }],
    flows: [{ id: 'f', name: 'F', steps: [{ from: 'ui/app.ts', to: 'core/svc.ts', action: 'call', evidence: { path: 'ui/app.ts', line: 1 } }, { from: 'core/svc.ts', to: 'missing.ts', action: 'x', evidence: { path: 'a', line: 1 } }] }],
    findings: [{ title: 'No proof', confidence: 'confirmed' }],
  });
  const w = embedded.model.warnings.join('\n');
  assert.match(w, /unknown target ext:nowhere/);
  assert.match(w, /edge .* missing evidence/);
  assert.match(w, /flow f step 2: unknown to missing\.ts/);
  assert.match(w, /confirmed needs evidence/);
  assert.match(w, /1 files match no layer/);
  assert.match(r.stderr, /model: /);
});

test('escapes markup in embedded data', () => {
  const { html } = run({ meta: { summary: '</script><b>x</b>' }, layers });
  assert.equal(html.includes('</script><b>x</b>'), false);
});

test('puts evidence layers, leads and layout facts into the brief', () => {
  const verify = {
    layers: { witness: 'ok', clasify: 'ok', ocr: 'skipped: Ollama unreachable', lsp: 'off' },
    leads: [{ id: 'l1', kind: 'edge', detector: 'cycle-edge', file: 'core/svc.ts', target: 'data/db.ts', line: 5, signals: { clasify: { typeOnly: 0.1, runtimeCall: 0.9 } }, verdict: 'corroborated', why: 'agree' }],
    render: { layout: { files: { labels: 3, overlaps: 1, labelOnNode: 0, clipped: 0, worst: [] } } },
    usage: { input_tokens: 1234, output_tokens: 5 },
  };
  const { brief, short, embedded } = run(null, verify);
  assert.match(short, /LEAD \[corroborated\] cycle-edge core\/svc\.ts > data\/db\.ts:5/);
  assert.match(short, /brief-full\.txt/);
  assert.ok(short.length < brief.length);
  assert.match(brief, /LAYERS witness=ok clasify=ok ocr=skipped: Ollama unreachable lsp=off \(opt-in\)/);
  assert.match(brief, /SECTION \/repo: 3 files scanned/);
  assert.match(brief, /SECTION-WIDE health over 2 production source files \(the count above is every scanned file/);
  assert.match(brief, /all 0 ranked findings are in data\.json insights\.issues/);
  assert.match(brief, /TERMS importers = files that import it/);
  assert.match(brief, /LEAD \[corroborated\] cycle-edge core\/svc\.ts > data\/db\.ts:5 :: agree \(typeOnly 0\.1, runtimeCall 0\.9\)/);
  assert.equal(embedded.tokens.clasifyInput, 1234);
});

test('synthesizes islands, hubs and unused-import insights', async () => {
  const { synthesize } = await import('./insights.mjs');
  const D = {
    ...data,
    files: [file('a/x.ts', 5, 0), file('a/y.ts', 1, 1), file('b/z.ts', 0, 0)],
    imports: [{ source: 'a/y.ts', target: 'a/x.ts', line: 2 }],
    verify: {
      leads: [{ kind: 'edge', verdict: 'unused', file: 'a/y.ts', target: 'a/x.ts', why: 'w', signals: { source: { names: ['X'] } } }],
      connections: { checked: 1, confirmed: 1, viaImport: 1, viaPath: 0, na: 0, unconfirmed: [], mutual: [] },
      render: { layout: { files: { overlaps: 0 }, functions: { overlaps: 3 } } },
    },
  };
  const text = synthesize(D).map((x) => x.text).join('\n');
  assert.match(text, /2 disconnected islands/);
  assert.doesNotMatch(text, /fan-in/);
  assert.doesNotMatch(text, /\?|\(\)/);
  assert.match(text, /imports .* but never uses X/);
  assert.match(text, /AST confirms 1 of 1/);
  assert.match(text, /Functions view: 3 overlapping labels, 0 labels hidden/);
});

test('plans chunks under the snapshot budget and links declared dependencies', () => {
  const root = mkdtempSync(join(tmpdir(), 'gv-root-'));
  const put = (path, text = 'x') => { execFileSync('mkdir', ['-p', join(root, path, '..')]); writeFileSync(join(root, path), text); };
  for (let i = 0; i < 12; i++) { put(`pkg-a/src/f${i}.ts`); put(`pkg-b/src/f${i}.ts`); }
  put('pkg-a/package.json', JSON.stringify({ name: '@x/a', dependencies: { '@x/b': '1' } }));
  put('pkg-b/package.json', JSON.stringify({ name: '@x/b' }));
  const out = join(root, '.out');
  const chunk = join(dirname(fileURLToPath(import.meta.url)), 'chunk.mjs');
  const r = spawnSync(process.execPath, [chunk, root, '--out', out, '--chunk-files', '20', '--build', '0'], { encoding: 'utf8' });
  assert.equal(r.status, 0, r.stderr);
  const overview = JSON.parse(readFileSync(join(out, 'overview', 'data.json'), 'utf8'));
  assert.deepEqual(overview.chunks.map((c) => c.path).sort(), ['pkg-a', 'pkg-b']);
  assert.equal(overview.chunks.every((c) => c.files <= 20), true);
  assert.deepEqual(overview.imports.map((e) => `${e.source}>${e.target}:${e.via}`), ['pkg-a>pkg-b:declared']);
  const area = spawnSync(process.execPath, [chunk, root, '--out', join(root, '.area'), '--area', 'pkg-b', '--build', '0'], { encoding: 'utf8' });
  assert.equal(area.status, 0, area.stderr);
  const scoped = JSON.parse(readFileSync(join(root, '.area', 'overview', 'data.json'), 'utf8'));
  assert.deepEqual(scoped.chunks.map((c) => c.path), ['pkg-b']);
  assert.equal(scoped.area, 'pkg-b');
});

test('overview insights name built and unbuilt chunks without file wording', async () => {
  const { synthesize } = await import('./insights.mjs');
  const text = synthesize({
    mode: 'overview',
    area: null,
    files: [{ id: 'a', fanIn: 0, fanOut: 1 }, { id: 'b', fanIn: 1, fanOut: 0 }, { id: 'c', fanIn: 0, fanOut: 0 }],
    imports: [{ source: 'a', target: 'b', line: 1, via: 'declared' }],
    chunks: [{ id: 'a', path: 'pkg/a', files: 100, built: true, filesInCycles: 7 }, { id: 'b', path: 'pkg/b', files: 50, built: false }, { id: 'c', path: 'pkg/c', files: 10, built: false }],
  }).map((x) => x.text).join('\n');
  assert.match(text, /3 chunks cover 160 files; 1 built \(a\), 2 not built\. Build one with chunk\.mjs --only b/);
  assert.match(text, /1 links between chunks \(1 declared/);
  assert.match(text, /1 of 3 chunks have no declared or observed link/);
  assert.doesNotMatch(text, /disconnected islands/);
});

test('observed links use the prefixed package ids that the graph emits', () => {
  const root = mkdtempSync(join(tmpdir(), 'gv-obs-'));
  const put = (path, text = 'x') => { execFileSync('mkdir', ['-p', join(root, path, '..')]); writeFileSync(join(root, path), text); };
  for (let i = 0; i < 12; i++) { put(`pkg-a/f${i}.ts`); put(`pkg-b/f${i}.ts`); }
  put('pkg-a/package.json', JSON.stringify({ name: '@x/a' }));
  put('pkg-b/package.json', JSON.stringify({ name: '@x/b' }));
  const out = join(root, '.out');
  put('.out/chunks/pkg-a/data.json', JSON.stringify({ insights: { findings: {}, health: {} }, externals: [{ id: 'pkg:@x/b', count: 4 }, { id: 'pkg:json', count: 9 }] }));
  const chunk = join(dirname(fileURLToPath(import.meta.url)), 'chunk.mjs');
  const r = spawnSync(process.execPath, [chunk, root, '--out', out, '--chunk-files', '20', '--build', '0'], { encoding: 'utf8' });
  assert.equal(r.status, 0, r.stderr);
  const overview = JSON.parse(readFileSync(join(out, 'overview', 'data.json'), 'utf8'));
  assert.deepEqual(overview.imports.map((e) => `${e.source}>${e.target}:${e.via}:${e.line}`), ['pkg-a>pkg-b:observed:4']);
});

test('png crop flattens alpha and scales without external tools', async () => {
  const { cropScale, decodePng } = await import('./png.mjs');
  const { deflateSync } = await import('node:zlib');
  // 4x2 RGBA: left half opaque red, right half fully transparent.
  const w = 4, h = 2, raw = Buffer.alloc(h * (w * 4 + 1));
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) { const o = y * (w * 4 + 1) + 1 + x * 4; raw.set(x < 2 ? [255, 0, 0, 255] : [0, 0, 0, 0], o); }
  const src = Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])]);
  const crc = (b) => { let c = ~0; for (const x of b) { c ^= x; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; } return ~c >>> 0; };
  const ch = (n, body) => { const o = Buffer.alloc(12 + body.length); o.writeUInt32BE(body.length, 0); o.write(n, 4); body.copy(o, 8); o.writeUInt32BE(crc(o.subarray(4, 8 + body.length)), 8 + body.length); return o; };
  const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(h, 4); ihdr[8] = 8; ihdr[9] = 6;
  const png = Buffer.concat([src, ch('IHDR', ihdr), ch('IDAT', deflateSync(raw)), ch('IEND', Buffer.alloc(0))]);
  const out = decodePng(cropScale(png, { x: 1, y: 0, w: 3, h: 2 }, 2));
  assert.deepEqual([out.w, out.h, out.channels], [6, 4, 3]);
  assert.deepEqual([...out.data.subarray(0, 3)], [255, 0, 0]);   // opaque red kept
  assert.deepEqual([...out.data.subarray(6, 9)], [255, 255, 255]); // transparent became white
});

test('files directly in a split directory get their own chunk', () => {
  const root = mkdtempSync(join(tmpdir(), 'gv-loose-'));
  const put = (path) => { execFileSync('mkdir', ['-p', join(root, path, '..')]); writeFileSync(join(root, path), 'x'); };
  for (let i = 0; i < 12; i++) { put(`top${i}.ts`); put(`a/f${i}.ts`); put(`b/f${i}.ts`); }
  const out = join(root, '.out');
  const chunk = join(dirname(fileURLToPath(import.meta.url)), 'chunk.mjs');
  const r = spawnSync(process.execPath, [chunk, root, '--out', out, '--chunk-files', '20', '--build', '0'], { encoding: 'utf8' });
  assert.equal(r.status, 0, r.stderr);
  const plan = JSON.parse(readFileSync(join(out, 'plan.json'), 'utf8'));
  assert.deepEqual(plan.map((c) => [c.path, c.files, !!c.loose]).sort(), [['.', 12, true], ['a', 12, false], ['b', 12, false]]);
  assert.equal(plan.reduce((a, c) => a + c.files, 0), 36);
});

test('rust child modules that inherit imports count as usage', async () => {
  const { childUsage, countUsage } = await import('./usage.mjs');
  const dir = mkdtempSync(join(tmpdir(), 'gv-rs-'));
  const put = (path, text) => { execFileSync('mkdir', ['-p', join(dir, path, '..')]); writeFileSync(join(dir, path), text); };
  put('scope.rs', 'use super::failure::LspFailure;\nfn only_here() {}\n');
  put('scope/scan.rs', 'use super::*;\nfn f() -> Result<(), LspFailure> { LspFailure::cancelled() }\n');
  put('scope/other.rs', 'fn g() { LspFailure::ignored(); }\n'); // no inherit: must not count
  const own = countUsage(['use super::failure::LspFailure;', 'fn only_here() {}'], ['LspFailure'], 1, 1);
  assert.equal(own.usage, 0);
  const kids = childUsage(join(dir, 'scope.rs'), ['LspFailure']);
  assert.deepEqual([kids.usage, kids.calls, kids.files], [1, 1, ['scan.rs']]);
  assert.equal(childUsage(join(dir, 'scope/scan.rs'), ['LspFailure']).usage, 0); // leaf with no folder
});

test('edge verdicts follow the agreement of usage and clasify', async () => {
  const { edgeVerdict } = await import('./verdict.mjs');
  const src = (o) => ({ ast: 'confirmed', textOk: true, usageLines: 3, callLike: 1, names: ['X'], reexport: false, ...o });
  const runtime = { runtimeCall: 0.9, typeOnly: 0.1 }, type = { runtimeCall: 0.1, typeOnly: 0.8 }, unsure = { runtimeCall: 0.5, typeOnly: 0.5 };
  assert.equal(edgeVerdict(src({ usageLines: 0, callLike: 0 }), type).verdict, 'unused');
  assert.equal(edgeVerdict(src({ reexport: true }), runtime).verdict, 'reexport');
  assert.equal(edgeVerdict(src({ ast: 'absent' }), runtime).verdict, 'unconfirmed');
  const halfSure = { typeOnly: 0.84, runtimeCall: 0.6 };
  assert.equal(edgeVerdict(src({ callLike: 0, typePosition: true }), halfSure).verdict, 'dismissed', 'AST type position plus usage settle an edge clasify is unsure about');
  assert.equal(edgeVerdict(src({ callLike: 0 }), halfSure).verdict, 'unresolved', 'without the AST type position it stays unresolved');
  const { cohesionVerdict } = await import('./verdict.mjs');
  assert.equal(cohesionVerdict(0.82, 'hub').verdict, 'not-supported');
  assert.equal(cohesionVerdict(0.2, 'hub').verdict, 'corroborated');
  assert.equal(cohesionVerdict(0.5, 'hub'), null);
  assert.equal(cohesionVerdict(undefined, 'hub'), null);
  assert.equal(edgeVerdict(src({ ast: 'n/a', textOk: false }), runtime).verdict, 'unconfirmed');
  assert.equal(edgeVerdict(src({ callLike: 2 }), type).verdict, 'conflict');
  assert.equal(edgeVerdict(src({ callLike: 2 }), runtime).verdict, 'corroborated');
  assert.equal(edgeVerdict(src({ callLike: 0 }), type).verdict, 'dismissed');
  assert.equal(edgeVerdict(src({ callLike: 2 }), unsure).verdict, 'unresolved');
  assert.equal(edgeVerdict(src({ callLike: 2 }), null).verdict, 'unresolved');
  assert.deepEqual(edgeVerdict(src({ callLike: 2 }), runtime).votes, { usage: 'runtime', clasify: 'runtime' });
  // without an AST, a text witness still lets usage vote
  assert.equal(edgeVerdict(src({ ast: 'n/a', textOk: true }), runtime).verdict, 'corroborated');
});

test('file verdicts use strict thresholds for the generic question', async () => {
  const { fileVerdict } = await import('./verdict.mjs');
  const big = { lines: 900, owners: 3 }, small = { lines: 200, owners: 1 };
  assert.equal(fileVerdict({ unrelated: 0.9 }, big, 'god-file').verdict, 'corroborated');
  assert.equal(fileVerdict({ unrelated: 0.1 }, big, 'god-file').verdict, 'not-supported');
  assert.equal(fileVerdict({ unrelated: 0.65 }, big, 'god-file').verdict, 'unresolved');
  assert.equal(fileVerdict({ unrelated: 0.65 }, small, 'god-file').verdict, 'not-supported');
  assert.equal(fileVerdict({ owners: 0.76 }, big, 'hub').verdict, 'corroborated'); // named question: 0.7 is enough
  assert.equal(fileVerdict({ owners: 0.3 }, big, 'hub').verdict, 'not-supported');
  assert.equal(fileVerdict(undefined, big, 'hub').verdict, 'unresolved');
});

test('lead selection orders, caps and deduplicates', async () => {
  const { selectLeads } = await import('./leads.mjs');
  const f = (id, fanIn, group) => ({ id, label: id, group, fanIn, fanOut: 1 });
  const D = {
    files: [f('a/x.ts', 9, 'a'), f('a/y.ts', 2, 'a'), f('b/z.ts', 5, 'b'), f('b/w.ts', 1, 'b')],
    imports: [
      { source: 'a/y.ts', target: 'a/x.ts', line: 1 }, { source: 'b/z.ts', target: 'a/x.ts', line: 2 },
      { source: 'b/w.ts', target: 'b/z.ts', line: 3 }, { source: 'a/x.ts', target: 'a/y.ts', line: 4 },
    ],
    insights: { cycles: { top: [{ nodes: ['a/x.ts', 'a/y.ts'] }] }, issues: [{ detector: 'god-file', subject: 'b/z.ts', title: 't', score: 1 }, { detector: 'cycle', subject: 'a/x.ts', title: 'c', score: 1 }] },
  };
  const leads = selectLeads(D, { upward: [{ source: 'b/w.ts', target: 'a/x.ts', line: 7 }], layerOf: { 'b/w.ts': 'l2', 'a/x.ts': 'l1' } }, 6);
  assert.deepEqual(leads.map((l) => l.detector).slice(0, 3), ['upward-import', 'cycle-edge', 'cycle-edge']);
  assert.ok(leads.length <= 6 && new Set(leads.map((l) => l.id)).size === leads.length);
  assert.ok(leads.some((l) => l.kind === 'file' && l.detector === 'god-file' && l.file === 'b/z.ts'));
  assert.ok(leads.every((l) => l.signals.graph));
  assert.equal(selectLeads(D, null, 2).length, 2);
});

test('history analysis counts churn, small-commit co-change and section-wide top files', async () => {
  const { analyzeHistory } = await import('./history.mjs');
  const D = { files: [{ id: 'a.ts', fanIn: 5 }, { id: 'b.ts', fanIn: 1 }], imports: [] };
  const day = new Date(Date.now() - 864e5).toISOString().slice(0, 10);
  const log = [
    ...[1, 2, 3].map((n) => `@c${n}|ann|${day}\na.ts\nb.ts\nother.ts`),
    `@big|bob|2020-01-01\n${Array.from({ length: 50 }, (_, i) => `f${i}.ts`).join('\n')}\na.ts`,
  ].join('\n');
  const h = analyzeHistory(log, D, { maxCommitFiles: 40, limit: 0 });
  assert.deepEqual([h.quality.commits, h.quality.small, h.quality.maxPair], [4, 3, 3]);
  assert.deepEqual(h.coChange.map((c) => [c.a, c.b, c.count, c.imported]), [['a.ts', 'b.ts', 3, false]]);
  assert.deepEqual(h.top[0], { id: 'a.ts', commits: 4 });
  assert.equal(h.files['a.ts'].recent90, 3);
  assert.equal(h.hotspots[0].id, 'a.ts');
});

test('a rust crate root is named by its crate name as well as its stem', async () => {
  const { stemsOf, stemOf } = await import('./names.mjs');
  const dir = mkdtempSync(join(tmpdir(), 'gv-crate-'));
  const crate = join(dir, 'crates', 'contract-types');
  execFileSync('mkdir', ['-p', join(crate, 'src')]);
  writeFileSync(join(crate, 'Cargo.toml'), '[package]\nname = "octocode-contract-types"\nversion = "1.0.0"\n\n[dependencies]\nserde = "1"\n');
  writeFileSync(join(crate, 'src', 'lib.rs'), 'pub struct ErrorCode;\n');
  assert.equal(stemOf('crates/contract-types/src/lib.rs'), 'contract-types');
  assert.deepEqual(stemsOf('crates/contract-types/src/lib.rs', join(crate, 'src', 'lib.rs')), ['contract-types', 'contract_types', 'octocode_contract_types']);
  assert.deepEqual(stemsOf('crates/runtime/src/id.rs', join(dir, 'crates/runtime/src/id.rs')), ['id']);
  assert.deepEqual(stemsOf('src/app.ts', join(dir, 'src/app.ts')), ['app']);
});

test('source size skips build output and excluded directories', async () => {
  const { sourceSize } = await import('./lib.mjs');
  const dir = mkdtempSync(join(tmpdir(), 'gv-size-'));
  for (const d of ['src', 'target/debug', 'node_modules/x', 'fuzz', '.hidden']) execFileSync('mkdir', ['-p', join(dir, d)]);
  writeFileSync(join(dir, 'src', 'a.rs'), 'fn a() {}\n');
  writeFileSync(join(dir, 'target', 'debug', 'big.bin'), 'x'.repeat(10_000));
  writeFileSync(join(dir, 'node_modules', 'x', 'i.js'), 'x'.repeat(500));
  writeFileSync(join(dir, 'fuzz', 'corpus'), 'x'.repeat(700));
  writeFileSync(join(dir, '.hidden', 'h'), 'x'.repeat(300));
  writeFileSync(join(dir, 'src', 'staged.bin'), Buffer.alloc((1 << 20) + 1));
  assert.deepEqual(sourceSize(dir), { bytes: 10 + 700, files: 2 });
  assert.deepEqual(sourceSize(dir, ['fuzz']), { bytes: 10, files: 1 });
});

test('freellama vision requests and receipts keep the Ollama decoding shape', async () => {
  const { visionRequest, textOf, evalCount, errorOf, modelNames, installedTag } = await import('./freellama.mjs');
  assert.equal(installedTag(['gpt-oss:20b', 'glm-ocr:latest'], 'glm-ocr'), 'glm-ocr:latest');
  assert.equal(installedTag(['glm-ocr:q8_0', 'glm-ocr:latest'], 'glm-ocr:q8_0'), 'glm-ocr:q8_0');
  assert.equal(installedTag(['gpt-oss:20b'], 'glm-ocr'), null);
  const req = visionRequest({ model: 'glm-ocr', prompt: 'Text Recognition:', image: 'AAAA', maxTokens: 600, sessionId: 's1', options: { repeat_penalty: 1.15 } });
  assert.equal(req.task, 'vision');
  assert.deepEqual(req.required_capabilities, ['vision']);
  assert.deepEqual(req.images, ['AAAA']);
  assert.equal(req.session_id, 's1');
  assert.deepEqual(req.request_options.options, { temperature: 0, num_predict: 600, repeat_penalty: 1.15 });
  assert.equal('session_id' in visionRequest({ model: 'm', prompt: 'p', image: 'i', maxTokens: 1 }), false);
  assert.equal(req.context_tokens, 4096, 'OCR runs in a 4K context unless the caller asks otherwise');
  assert.equal('context_tokens' in visionRequest({ model: 'm', prompt: 'p', image: 'i', maxTokens: 1, contextTokens: 0 }), false);
  const { labelTokens, STRIP_TOKENS } = await import('./freellama.mjs');
  assert.equal(labelTokens(31), 198, 'measured: 31 labels read inside 200 tokens');
  assert.equal(labelTokens(1), 48);
  assert.equal(labelTokens(500), 600);
  assert.equal(STRIP_TOKENS, 400);
  assert.equal(textOf({ message: { content: 'raw' } }), 'raw');
  assert.equal(textOf({ response: { message: { content: 'managed' }, eval_count: 7 } }), 'managed');
  assert.equal(evalCount({ response: { eval_count: 7 } }), 7);
  assert.equal(errorOf({ error: 'repeat limit' }), 'repeat limit');
  assert.equal(errorOf({ error: { code: 'admission_timeout' } }), 'admission_timeout');
  assert.deepEqual(modelNames({ models: [{ name: 'glm-ocr:latest' }, { model: 'x' }] }), ['glm-ocr:latest', 'x']);
  assert.deepEqual(modelNames(['a']), ['a']);
});
