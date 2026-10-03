#!/usr/bin/env node
// Deterministic repo scan → draft architecture model (components, import edges, signals, stores).
// Zero dependencies. Output conforms to scheme/architecture-model.json.
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync, statSync, writeFileSync, mkdirSync } from 'node:fs';
import { basename, dirname, extname, join, posix, relative, resolve } from 'node:path';

const HELP = `Usage: node scan.mjs <root> [--out <file>] [--max-files N] [--split N] [--exclude <prefix,...>] [--octocode]

Scans a repository and writes a draft architecture model (JSON) for render.mjs.
  --out        output file (default <root>/.octocode/architecture-view/scan.json)
  --max-files  code-file ceiling, truncation is reported (default 20000)
  --split      split a component into sub-components above N code files (default 80)
  --exclude    comma-separated path prefixes to skip (e.g. fixtures,packages/x/context)
  --octocode   also run \`octocode astTopology cycles\` (needs OCTOCODE_BETA) and attach runtime cycles
stdout: compact summary for the modeling step. Exit 0 ok, 2 bad input.
Example: node scan.mjs . --exclude packages/bench/context --octocode`;

const args = process.argv.slice(2);
if (!args.length || args.includes('--help') || args.includes('-h')) { console.log(HELP); process.exit(args.length ? 0 : 2); }
const flag = (name, dflt) => { const i = args.indexOf(name); return i >= 0 ? args[i + 1] : dflt; };
const ROOT = resolve(args.find((a, i) => !a.startsWith('--') && !['--out', '--max-files', '--split', '--exclude'].includes(args[i - 1])) || '.');
if (!existsSync(ROOT) || !statSync(ROOT).isDirectory()) { console.error(`scan: root is not a directory: ${ROOT}`); process.exit(2); }
const OUT = resolve(flag('--out', join(ROOT, '.octocode/architecture-view/scan.json')));
const MAX_FILES = Number(flag('--max-files', 20000));
const SPLIT = Number(flag('--split', 80));
const EXCLUDE = (flag('--exclude', '') || '').split(',').map((s) => s.trim().replace(/\/$/, '')).filter(Boolean);

const CODE_EXT = { '.ts': 'ts', '.tsx': 'ts', '.mts': 'ts', '.cts': 'ts', '.js': 'js', '.jsx': 'js', '.mjs': 'js', '.cjs': 'js',
  '.vue': 'vue', '.svelte': 'svelte', '.rs': 'rust', '.py': 'python', '.go': 'go', '.java': 'java', '.kt': 'kotlin', '.rb': 'ruby',
  '.php': 'php', '.cs': 'csharp', '.swift': 'swift', '.c': 'c', '.h': 'c', '.cc': 'cpp', '.cpp': 'cpp', '.hpp': 'cpp', '.sql': 'sql' };
const IGNORE_DIRS = new Set(['node_modules', '.git', 'target', 'dist', 'build', 'out', 'coverage', '.next', '.nuxt', 'vendor', '__pycache__', '.venv', 'venv', '.octocode', '.turbo', '.cache']);
const MANIFESTS = ['package.json', 'Cargo.toml', 'pyproject.toml', 'setup.py', 'go.mod', 'pom.xml', 'build.gradle', 'build.gradle.kts'];

// ---- file inventory -------------------------------------------------------
function listFiles() {
  try {
    const out = execFileSync('git', ['-C', ROOT, 'ls-files', '-co', '--exclude-standard'], { maxBuffer: 256 << 20, stdio: ['ignore', 'pipe', 'ignore'] });
    return out.toString().split('\n').filter(Boolean);
  } catch {
    const acc = [];
    const walk = (d) => { for (const e of readdirSync(join(ROOT, d), { withFileTypes: true })) {
      if (e.isDirectory()) { if (!IGNORE_DIRS.has(e.name) && !e.name.startsWith('.')) walk(d ? `${d}/${e.name}` : e.name); }
      else acc.push(d ? `${d}/${e.name}` : e.name);
    } };
    walk('');
    return acc;
  }
}
const excluded = (p) => p.split('/').some((s, i, a) => IGNORE_DIRS.has(s) || (i < a.length - 1 && s.startsWith('.'))) || EXCLUDE.some((x) => p === x || p.startsWith(`${x}/`));
const all = listFiles().filter((p) => !excluded(p));
const manifestPaths = all.filter((p) => MANIFESTS.includes(basename(p)));
let code = all.filter((p) => CODE_EXT[extname(p)] && !/\.min\.js$|\.d\.ts$/.test(p));
const truncated = code.length > MAX_FILES;
if (truncated) code = code.slice(0, MAX_FILES);
const codeSet = new Set(code);

// ---- manifests → components -------------------------------------------------
const readText = (p) => { try { return readFileSync(join(ROOT, p), 'utf8'); } catch { return ''; } };
const tomlDeps = (txt) => { const deps = new Set(); let inDeps = false;
  for (const line of txt.split('\n')) {
    const sec = line.match(/^\s*\[([^\]]+)\]/);
    if (sec) { inDeps = /(^|\.)(dependencies|build-dependencies)$/.test(sec[1]) && !/dev-dependencies/.test(sec[1]);
      const inline = sec[1].match(/dependencies\.([\w-]+)$/); if (inline) deps.add(inline[1]); continue; }
    if (inDeps) { const m = line.match(/^\s*([\w-]+)\s*(=|\.)/); if (m) deps.add(m[1]); }
  }
  return [...deps]; };
const comps = new Map(); // dir -> component
const pkgByName = new Map(); // js package / crate / go module name -> dir
for (const m of manifestPaths.sort()) {
  const dir = dirname(m) === '.' ? '.' : dirname(m);
  const txt = readText(m); const file = basename(m);
  const c = comps.get(dir) || { dir, name: dir === '.' ? basename(ROOT) : basename(dir), manifests: [], deps: new Set(), bins: [], entries: [], workspace: false };
  c.manifests.push(file);
  if (file === 'package.json') { try {
    const j = JSON.parse(txt); if (j.name) { c.name = j.name; pkgByName.set(j.name, dir); }
    for (const k of ['dependencies', 'peerDependencies', 'optionalDependencies']) Object.keys(j[k] || {}).forEach((d) => c.deps.add(d));
    if (j.bin) c.bins.push(...(typeof j.bin === 'string' ? [j.bin] : Object.values(j.bin)));
    const exp = typeof j.exports === 'string' ? j.exports : j.exports?.['.']?.import || j.exports?.['.']?.default || j.exports?.['.'];
    for (const e of [j.source, j.module, typeof exp === 'string' ? exp : null, j.main]) if (e) c.entries.push(posix.normalize(posix.join(dir, e)));
    if (j.workspaces) c.workspace = true;
    if (j.engines?.vscode || j.contributes) c.deps.add('vscode');
  } catch { /* malformed package.json: keep dir as component */ } }
  if (file === 'Cargo.toml') {
    const name = txt.match(/^\s*\[package\][^[]*?\bname\s*=\s*"([^"]+)"/ms)?.[1];
    if (name) { c.name = name; pkgByName.set(name.replace(/-/g, '_'), dir); c.crate = name.replace(/-/g, '_'); }
    if (/^\s*\[workspace\]/m.test(txt)) c.workspace = true;
    tomlDeps(txt).forEach((d) => c.deps.add(d));
    const libPath = txt.match(/^\s*\[lib\][^[]*?\bpath\s*=\s*"([^"]+)"/ms)?.[1];
    c.entries.push(posix.join(dir === '.' ? '' : dir, libPath || 'src/lib.rs'), posix.join(dir === '.' ? '' : dir, 'src/main.rs'));
    if (/^\s*\[\[bin\]\]/m.test(txt) || codeSet.has(posix.join(dir === '.' ? '' : dir, 'src/main.rs'))) c.bins.push('main.rs');
  }
  if (file === 'go.mod') { const mod = txt.match(/^module\s+(\S+)/m)?.[1]; if (mod) { c.goModule = mod; pkgByName.set(mod, dir); } }
  if (file === 'pyproject.toml') { const name = txt.match(/^\s*name\s*=\s*"([^"]+)"/m)?.[1]; if (name) c.name = name;
    for (const d of txt.matchAll(/^\s*"([A-Za-z0-9_.-]+)[^"]*",?\s*$/gm)) c.deps.add(d[1].toLowerCase()); }
  comps.set(dir, c);
}
// drop manifest dirs without code and pure workspace roots with nothing of their own
const ownerOf = (p) => { let d = dirname(p); for (;;) { if (comps.has(d)) return d; if (d === '.' || d === '') return comps.has('.') ? '.' : null; d = dirname(d); } };

// ---- component assignment + splitting ---------------------------------------
const topDir = (p) => (p.includes('/') ? p.split('/')[0] : '.');
const fileComp = new Map();
for (const f of code) fileComp.set(f, ownerOf(f) && ownerOf(f) !== '.' ? ownerOf(f) : (comps.has('.') && !comps.get('.').workspace && topDir(f) === '.') ? '.' : topDir(f));
const members = new Map();
for (const [f, c] of fileComp) { if (!members.has(c)) members.set(c, []); members.get(c).push(f); }
const parentOf = new Map();
function split(cid, depthLeft) {
  const files = members.get(cid); if (!files || files.length <= SPLIT || depthLeft === 0) return;
  const base = cid === '.' ? '' : `${cid}/`;
  const srcRoot = ['src', 'lib', 'app', 'crates', 'packages', 'internal', 'pkg'].find((s) => files.some((f) => f.startsWith(`${base}${s}/`)));
  const rootPrefix = srcRoot && files.filter((f) => f.startsWith(`${base}${srcRoot}/`)).length > files.length * 0.5 ? `${base}${srcRoot}/` : base;
  const groups = new Map();
  for (const f of files) { if (!f.startsWith(rootPrefix)) continue; const rest = f.slice(rootPrefix.length); if (!rest.includes('/')) continue;
    const g = rootPrefix + rest.split('/')[0]; if (!groups.has(g)) groups.set(g, []); groups.get(g).push(f); }
  const kids = [...groups].filter(([, fs]) => fs.length >= 4);
  if (kids.length < 2) return;
  const own = new Set(files);
  for (const [g, fs] of kids) { members.set(g, fs); fs.forEach((f) => { fileComp.set(f, g); own.delete(f); }); parentOf.set(g, cid); split(g, depthLeft - 1); }
  members.set(cid, [...own]);
}
for (const cid of [...members.keys()]) if (!parentOf.has(cid)) split(cid, 2);
// ensure parents exist as nodes even when they own no files
for (const p of parentOf.values()) if (!members.has(p)) members.set(p, []);

// ---- per-file parsing ---------------------------------------------------------
const SIGNALS = {
  ui: /^(react|react-dom|vue|svelte|@angular\/core|solid-js|preact|next|nuxt|lit|@sveltejs\/kit|vscode|@vscode\/webview-ui-toolkit|leptos|yew|dioxus)$/,
  api: /^(express|fastify|koa|hono|@nestjs\/core|axum|actix-web|rocket|warp|flask|fastapi|django|github\.com\/gin-gonic\/gin|github\.com\/labstack\/echo)/,
  mcp: /^(@modelcontextprotocol\/sdk|rmcp|mcp|fastmcp)/,
  cli: /^(commander|yargs|clap|cac|meow|citty|click|typer|github\.com\/spf13\/cobra|inquirer|@clack\/prompts)$/,
  db: /^(@prisma\/client|prisma|typeorm|sequelize|mongoose|mongodb|knex|drizzle-orm|better-sqlite3|sqlite3|node:sqlite|bun:sqlite|pg|postgres|mysql2|redis|ioredis|rusqlite|sqlx|diesel|sea-orm|sqlalchemy|psycopg2?|lancedb|@lancedb\/lancedb|duckdb|sled|redb|rocksdb)$/,
  http: /^(axios|got|node-fetch|undici|ky|reqwest|ureq|hyper|requests|httpx|aiohttp|@octokit\/[\w-]+|octokit|octocrab)$/,
  llm: /^(openai|@anthropic-ai\/sdk|anthropic|ollama|ollama-rs|@google\/generative-ai|@google\/genai|langchain|@langchain\/[\w-]+|ai|async-openai)$/,
  ffi: /^(napi|napi-derive|@napi-rs\/[\w-]+|node-addon-api|wasm-bindgen|pyo3|neon)$/,
  queue: /^(kafkajs|amqplib|bullmq|rdkafka|lapin|nats|async-nats|@aws-sdk\/client-sqs|celery)$/,
};
const CONTENT_SIGNALS = [
  ['db', /\b(CREATE TABLE|INSERT INTO|SELECT\s+[\w*.,\s]+?\s+FROM\s+\w|UPDATE\s+\w+\s+SET|DELETE FROM)\b/],
  ['mcp', /\bregisterTool\s*\(|\bnew McpServer\b|#\[tool\(|server\.tool\s*\(/],
  ['ffi', /#\[napi\b|\bnapi::bindgen_prelude|require\([^)]*\.node['"]\)|#\[wasm_bindgen|#\[pyfunction/],
  ['process', /\bchild_process\b|\bspawn\s*\(|\bexecFile(Sync)?\s*\(|Command::new\s*\(|subprocess\.(run|Popen)/],
  ['api', /\.listen\s*\(\s*\d|\bcreateServer\s*\(|app\.(get|post|put|delete)\s*\(\s*['"]\//],
  ['http', /\bfetch\s*\(\s*[`'"]?https?:|\bfetch\s*\(\s*\w+Url|reqwest::Client|https?:\/\/api\./],
  ['config', /process\.env\.[A-Z_]{3,}|std::env::var\s*\(|os\.environ|os\.getenv\s*\(/],
  ['event', /\bnew EventEmitter\b|\.emit\s*\(\s*['"]|broadcast::channel|mpsc::channel|postMessage\s*\(/],
  ['ui', /<\/?[A-Z][A-Za-z]+[\s/>]|document\.querySelector|createWebviewPanel|\bReactDOM\b/],
];
const STORE_OF = [[/sqlite|rusqlite|redb|sled/, 'SQLite / embedded DB'], [/^(pg|postgres)$|psycopg|sqlx|diesel|sea-orm|typeorm|sequelize|knex|drizzle|prisma/, 'SQL database'],
  [/mysql/, 'MySQL'], [/redis/, 'Redis'], [/mongo/, 'MongoDB'], [/lancedb/, 'LanceDB'], [/duckdb/, 'DuckDB'], [/rocksdb/, 'RocksDB']];
const EXT_OF = [[/octokit|octocrab/, 'GitHub API'], [/^openai$|async-openai/, 'OpenAI API'], [/anthropic/, 'Anthropic API'], [/ollama/, 'Ollama'], [/google\/gen/, 'Google GenAI'],
  [/kafka|rdkafka/, 'Kafka'], [/amqp|lapin/, 'RabbitMQ'], [/nats/, 'NATS'], [/bullmq/, 'Redis queue'], [/sqs/, 'AWS SQS']];

const lineAt = (starts, idx) => { let lo = 0, hi = starts.length - 1; while (lo < hi) { const mid = (lo + hi + 1) >> 1; if (starts[mid] <= idx) lo = mid; else hi = mid - 1; } return lo + 1; };
const JS_RE = /(?:^|[;\s])(import|export)\s+(type\s+)?(?:[\w*{}\s,$]+?\s+from\s+)?['"]([^'"\n]+)['"]|\bimport\s*\(\s*['"]([^'"\n]+)['"]\s*\)|\brequire\s*\(\s*['"]([^'"\n]+)['"]\s*\)/g;
const JS_EXTS = ['', '.ts', '.tsx', '.mts', '.js', '.jsx', '.mjs', '.cjs', '/index.ts', '/index.tsx', '/index.js', '/index.mjs'];
function resolveJs(from, spec) {
  if (spec.startsWith('.')) {
    const base = posix.normalize(posix.join(posix.dirname(from), spec));
    const stripped = base.replace(/\.(m|c)?js$/, '');
    for (const b of [base, stripped]) for (const e of JS_EXTS) if (codeSet.has(b + e)) return { file: b + e };
    return { unresolved: true };
  }
  if (spec.startsWith('node:') || spec.startsWith('bun:')) return { external: spec };
  const pkg = spec.startsWith('@') ? spec.split('/').slice(0, 2).join('/') : spec.split('/')[0];
  if (pkgByName.has(pkg)) { const dir = pkgByName.get(pkg); const c = comps.get(dir);
    const sub = spec.slice(pkg.length).replace(/^\//, '');
    if (sub) for (const pre of ['src/', '', 'lib/']) { const b = posix.join(dir === '.' ? '' : dir, pre + sub); for (const e of JS_EXTS) if (codeSet.has(b + e)) return { file: b + e }; }
    for (const e of c?.entries || []) { const alt = [e, e.replace(/^(.*?)(dist|out|build|lib)\//, '$1src/').replace(/\.(m|c)?js$/, '.ts')]; for (const a of alt) if (codeSet.has(a)) return { file: a }; }
    for (const e of ['src/index.ts', 'src/index.tsx', 'index.ts', 'index.js', 'src/index.js', 'src/lib.rs']) { const a = posix.join(dir === '.' ? '' : dir, e); if (codeSet.has(a)) return { file: a }; }
    return { comp: dir };
  }
  return { external: pkg };
}
function rustModFile(dir, segs) {
  for (let n = segs.length; n > 0; n--) { const b = posix.join(dir, ...segs.slice(0, n)); if (codeSet.has(`${b}.rs`)) return `${b}.rs`; if (codeSet.has(`${b}/mod.rs`)) return `${b}/mod.rs`; }
  return null;
}
function crateSrc(f) { let d = dirname(f); while (d && d !== '.') { if (comps.get(d)?.crate) return { src: posix.join(d, 'src'), dir: d }; d = dirname(d); } return comps.get('.')?.crate ? { src: 'src', dir: '.' } : null; }
function modDir(f) { const b = basename(f); return ['mod.rs', 'lib.rs', 'main.rs'].includes(b) ? dirname(f) : posix.join(dirname(f), b.replace(/\.rs$/, '')); }
const pyIndex = new Map();
for (const f of code) if (f.endsWith('.py')) { const mod = f.replace(/\.py$/, '').replace(/\/__init__$/, '').split('/'); for (let i = 0; i < mod.length; i++) pyIndex.set(mod.slice(i).join('.'), pyIndex.get(mod.slice(i).join('.')) ?? f); }

const fileInfo = new Map(); // path -> {lang, loc, imports:[{file|comp, line, type}], ext:Set, sig:{cat:[line]}}
let unresolved = 0; const unresolvedSamples = [];
for (const f of code) {
  const lang = CODE_EXT[extname(f)]; const info = { lang, loc: 0, imports: [], ext: new Set(), sig: {} };
  fileInfo.set(f, info);
  let txt = ''; try { const st = statSync(join(ROOT, f)); if (st.size > 1_500_000) continue; txt = readFileSync(join(ROOT, f), 'utf8'); } catch { continue; }
  const starts = [0]; for (let i = 0; i < txt.length; i++) if (txt.charCodeAt(i) === 10) { starts.push(i + 1); }
  for (let i = 0; i < starts.length; i++) { const s = starts[i], e = (starts[i + 1] ?? txt.length + 1) - 1; if (/\S/.test(txt.slice(s, e))) info.loc++; }
  const add = (r, idx, type) => { const line = lineAt(starts, idx);
    if (r.file && r.file !== f) info.imports.push({ file: r.file, line, type });
    else if (r.comp) info.imports.push({ comp: r.comp, line, type });
    else if (r.external) info.ext.add(r.external);
    else if (r.unresolved) { unresolved++; if (unresolvedSamples.length < 8) unresolvedSamples.push(`${f}:${line}`); } };
  if (lang === 'ts' || lang === 'js' || lang === 'vue' || lang === 'svelte') {
    for (const m of txt.matchAll(JS_RE)) { const spec = m[3] || m[4] || m[5]; if (spec) add(resolveJs(f, spec), m.index, m[2] ? 'type' : 'runtime'); }
  } else if (lang === 'rust') {
    const cs = crateSrc(f);
    for (const m of txt.matchAll(/^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;/gm)) { const t = rustModFile(modDir(f), [m[1]]); if (t) add({ file: t }, m.index, 'runtime'); }
    if (cs) for (const m of txt.matchAll(/\bcrate::(\{[^}]*\}|\w+(?:::\w+)*)/g)) {
      const paths = m[1].startsWith('{') ? m[1].slice(1, -1).split(',').map((s) => s.trim().split(/\s|::\{/)[0]).filter(Boolean) : [m[1]];
      for (const p of paths) { const t = rustModFile(cs.src, p.split('::')); if (t) add({ file: t }, m.index, 'runtime'); }
    }
    for (const m of txt.matchAll(/\bsuper::(\w+(?:::\w+)*)/g)) { const t = rustModFile(dirname(modDir(f)), m[1].split('::')); if (t) add({ file: t }, m.index, 'runtime'); }
    const seen = new Set();
    for (const m of txt.matchAll(/\b([a-z_][a-z0-9_]*)::/g)) { const n = m[1]; if (seen.has(n) || ['crate', 'self', 'super', 'std', 'core', 'alloc'].includes(n)) continue; seen.add(n);
      if (pkgByName.has(n) && pkgByName.get(n) !== cs?.dir) { const d = pkgByName.get(n); const entry = comps.get(d)?.entries.find((e) => codeSet.has(e)); add(entry ? { file: entry } : { comp: d }, m.index, 'runtime'); }
      else { const owner = comps.get(ownerOf(f)); const dash = n.replace(/_/g, '-'); if (owner?.deps.has(n) || owner?.deps.has(dash)) info.ext.add(owner.deps.has(dash) ? dash : n); } }
  } else if (lang === 'python') {
    for (const m of txt.matchAll(/^\s*(?:from\s+(\.*[\w.]*)\s+import|import\s+([\w.]+))/gm)) { const spec = m[1] ?? m[2];
      if (spec.startsWith('.')) { const up = spec.match(/^\.+/)[0].length; let d = dirname(f); for (let i = 1; i < up; i++) d = dirname(d);
        const rest = spec.slice(up); const t = rest ? rustLike(d, rest.split('.')) : null; if (t) add({ file: t }, m.index, 'runtime'); continue; }
      const hit = pyIndex.get(spec) || pyIndex.get(spec.split('.').slice(0, -1).join('.'));
      if (hit) add({ file: hit }, m.index, 'runtime'); else info.ext.add(spec.split('.')[0].toLowerCase()); }
  } else if (lang === 'go') {
    for (const m of txt.matchAll(/"([\w.\-/]+)"/g)) { const spec = m[1]; const mod = [...pkgByName.keys()].find((k) => k.includes('/') && spec.startsWith(k));
      if (mod) { const dir = posix.join(pkgByName.get(mod) === '.' ? '' : pkgByName.get(mod), spec.slice(mod.length).replace(/^\//, '')); const t = code.find((x) => dirname(x) === dir && x.endsWith('.go') && !x.endsWith('_test.go')); if (t) add({ file: t }, m.index, 'runtime'); }
      else if (/^[\w.-]+\.[a-z]+\//.test(spec)) info.ext.add(spec.split('/').slice(0, 3).join('/')); }
  }
  for (const [cat, re] of CONTENT_SIGNALS) { if (cat === 'ui' && !/\.(tsx|jsx|vue|svelte)$/.test(f)) continue; const m = re.exec(txt); if (m) info.sig[cat] = lineAt(starts, m.index); }
  for (const m of txt.matchAll(/https?:\/\/((?:api|registry)\.[a-z0-9.-]+\.[a-z]{2,})/g)) if (!info.hosts?.has(m[1])) (info.hosts ||= new Map()).set(m[1], lineAt(starts, m.index));
}
function rustLike(dir, segs) { const b = posix.join(dir, ...segs); return codeSet.has(`${b}.py`) ? `${b}.py` : codeSet.has(`${b}/__init__.py`) ? `${b}/__init__.py` : null; }

// ---- aggregate to nodes ------------------------------------------------------------
const TEST_RE = /(^|\/)(tests?|__tests__|e2e|spec|fixtures?|__mocks__|testdata)(\/|$)|\.(test|spec)\.[a-z]+$|_test\.go$/;
const TOOL_RE = /(^|\/)(scripts?|tooling|bench(mark)?s?|examples?|docs?|ci)(\/|$)/;
const isTest = (f) => TEST_RE.test(f);
// labels: package name for roots, "parent/child" for splits, minus a shared prefix such as "octocode-"
const rootNames = [...members.keys()].filter((c) => !parentOf.has(c)).map((c) => basename(c));
const PREFIX = (() => { const tally = new Map(); for (const n of rootNames) { const m = n.match(/^([a-z0-9]+[-_])./i); if (m) tally.set(m[1], (tally.get(m[1]) || 0) + 1); }
  const [p, n] = [...tally].sort((x, y) => y[1] - x[1])[0] || ['', 0]; return n >= 3 && n >= rootNames.length * 0.4 ? p : ''; })();
const short = (d) => { const b = basename(d); const s = b === 'src' || b === 'lib' ? basename(dirname(d)) : b; return PREFIX && s.startsWith(PREFIX) && s.length > PREFIX.length ? s.slice(PREFIX.length) : s; };
function labelOf(cid) {
  if (cid === '.') return comps.get('.')?.name || basename(ROOT);
  if (!parentOf.has(cid)) return comps.get(cid)?.name && !comps.get(cid).name.includes('/') ? comps.get(cid).name : short(cid);
  const p = short(parentOf.get(cid)), c = short(cid);
  return p === c ? basename(cid) : `${p}/${c}`;
}
const nodes = []; const idx = new Map();
const compIds = [...members.keys()].filter((c) => members.get(c).length || [...parentOf.values()].includes(c)).sort();
for (const cid of compIds) {
  const files = members.get(cid) || []; const c = comps.get(cid);
  const langs = {}; let loc = 0; const ext = new Map(); const sig = {};
  for (const f of files) { const i = fileInfo.get(f); loc += i.loc; langs[i.lang] = (langs[i.lang] || 0) + 1;
    if (!isTest(f)) { i.ext.forEach((e) => ext.set(e, (ext.get(e) || 0) + 1)); for (const [k, line] of Object.entries(i.sig)) { (sig[k] ||= { count: 0, samples: [] }).count++; if (sig[k].samples.length < 3) sig[k].samples.push({ path: f, line }); } } }
  const deps = new Set([...(c?.deps || []), ...ext.keys()]);
  for (const d of deps) for (const [cat, re] of Object.entries(SIGNALS)) if (re.test(d)) { (sig[cat] ||= { count: 0, samples: [] }); (sig[cat].deps ||= []).includes(d) || sig[cat].deps.push(d); }
  const testFiles = files.filter(isTest).length;
  const kind = (!c && testFiles > files.length * 0.6) || /(^|\/)(tests?|e2e|fixtures?)$/.test(cid) ? 'test'
    : TOOL_RE.test(`${cid}/`) && !c ? 'tooling'
    : sig.mcp ? 'mcp' : sig.ui?.deps || (sig.ui?.count || 0) > files.length * 0.3 ? 'ui' : sig.api ? 'api'
    : (c?.bins.length || sig.cli?.deps) && !parentOf.has(cid) ? 'cli'
    : sig.db ? 'data-access' : sig.ffi ? 'adapter'
    : /(core|domain|engine|model|shared|common|utils?|lib|types|contract|schema|config)/i.test(basename(cid)) ? 'library' : 'service';
  const layer = { ui: 'interface', cli: 'interface', mcp: 'interface', api: 'interface', service: 'application', library: 'domain', 'data-access': 'infrastructure', adapter: 'infrastructure', test: 'tooling', tooling: 'tooling' }[kind];
  const node = { id: cid, label: labelOf(cid), kind, layer, path: cid, guess: true,
    files: files.length, testFiles, loc, languages: langs, tech: [...deps].filter((d) => Object.values(SIGNALS).some((re) => re.test(d))).slice(0, 12),
    signals: sig, entrypoints: (c?.entries || []).filter((e) => codeSet.has(e)).slice(0, 3) };
  if (parentOf.has(cid)) node.parent = parentOf.get(cid);
  if (c) node.manifest = c.manifests;
  idx.set(cid, nodes.length); nodes.push(node);
}
// stores & externals discovered from dependencies / literal API hosts
const edgeMap = new Map();
const addEdge = (s, t, kind, ev) => { if (s === t) return; const k = `${s}|${t}|${kind}`; const e = edgeMap.get(k) || { source: s, target: t, kind, weight: 0, evidence: [] };
  e.weight++; if (ev && e.evidence.length < 3) e.evidence.push(ev); edgeMap.set(k, e); };
const virtual = (id, label, kind, layer, tech) => { if (!idx.has(id)) { idx.set(id, nodes.length); nodes.push({ id, label, kind, layer, guess: true, files: 0, loc: 0, tech: [tech] }); } else if (!nodes[idx.get(id)].tech.includes(tech)) nodes[idx.get(id)].tech.push(tech); };
for (const n of [...nodes]) {
  if (!n.signals || n.kind === 'test') continue;
  for (const d of n.signals.db?.deps || []) { const s = STORE_OF.find(([re]) => re.test(d)); if (s) { virtual(`store:${s[1]}`, s[1], 'store', 'data', d); addEdge(n.id, `store:${s[1]}`, 'db', n.signals.db.samples[0]); } }
  for (const cat of ['http', 'llm', 'queue']) for (const d of n.signals[cat]?.deps || []) { const s = EXT_OF.find(([re]) => re.test(d)); if (s) { virtual(`ext:${s[1]}`, s[1], cat === 'queue' ? 'queue' : 'external', 'data', d); addEdge(n.id, `ext:${s[1]}`, cat === 'queue' ? 'event' : cat, n.signals[cat].samples[0]); } }
}
for (const [f, i] of fileInfo) if (i.hosts && !isTest(f)) for (const [h, line] of i.hosts) { if (/example|localhost|schemas?\./.test(h)) continue; const cid = fileComp.get(f); const id = `ext:${h}`; virtual(id, h, 'external', 'data', h); addEdge(cid, id, 'http', { path: f, line }); }

// file-level + component-level import edges
const fileIndex = new Map(code.map((f, i) => [f, i]));
const fileEdges = [];
for (const [f, i] of fileInfo) {
  const src = fileComp.get(f); const seen = new Set();
  for (const imp of i.imports) {
    const tgtComp = imp.file ? fileComp.get(imp.file) : imp.comp && (members.has(imp.comp) ? imp.comp : null);
    if (imp.file && !seen.has(imp.file)) { seen.add(imp.file); fileEdges.push(imp.type === 'type' ? [fileIndex.get(f), fileIndex.get(imp.file), 1] : [fileIndex.get(f), fileIndex.get(imp.file)]); }
    if (tgtComp && tgtComp !== src) addEdge(src, tgtComp, imp.type === 'type' ? 'type-import' : 'import', { path: f, line: imp.line });
  }
}
// collapse type-import where a runtime import exists between the same pair
for (const [k, e] of edgeMap) if (e.kind === 'type-import' && edgeMap.has(`${e.source}|${e.target}|import`)) { edgeMap.get(`${e.source}|${e.target}|import`).weight += e.weight; edgeMap.delete(k); }

// ---- optional octocode enrichment ----------------------------------------------------
const findings = []; let topology = null;
if (args.includes('--octocode')) {
  try {
    const q = { queries: [{ reasoning: 'architecture-view: runtime import cycles for the scanned root', operation: 'topology', analysis: 'cycles', path: ROOT, pageSize: 100, maxFiles: Math.min(MAX_FILES, 50000), excludeDir: [...IGNORE_DIRS] }] };
    let out;
    try { out = execFileSync('octocode', ['astTopology', JSON.stringify(q)], { env: { ...process.env, OCTOCODE_BETA: 'true' }, maxBuffer: 64 << 20, stdio: ['ignore', 'pipe', 'pipe'], timeout: 180_000 }); }
    catch (err) { if (err.status !== 6 || !err.stdout?.length) throw err; out = err.stdout; } // 6 = partial result, still usable
    const data = JSON.parse(out.toString()).results?.[0]?.data; if (!data) throw new Error('no astTopology data');
    const base = data.path && data.path !== '.' ? `${data.path}/` : '';
    for (const r of data.results || []) { if (!r.runtimeCycle) continue;
      const files = r.files.map((x) => base + x).filter((x) => fileComp.has(x)); const cs = [...new Set(files.map((x) => fileComp.get(x)))];
      if (!files.length) continue;
      findings.push({ id: `octocode-cycle-${findings.length + 1}`, severity: 'warn', source: 'octocode astTopology', title: `Runtime import cycle across ${files.length} files`,
        detail: `Strongly connected runtime imports (type-only edges excluded) spanning ${cs.join(', ')}. Break the cycle at its weakest edge.`, nodes: cs,
        evidence: (r.runtimeCycleEdges || r.cycleEdges || []).slice(0, 5).map((e) => ({ path: base + e.from, note: `→ ${base + e.to}` })) }); }
    topology = { tool: 'octocode astTopology cycles', filesScanned: data.filesScanned, components: data.pagination?.totalEntries, runtimeCycles: findings.length, completeness: data.completeness };
    console.error(`scan: octocode astTopology cycles ok (${findings.length} runtime cycle groups)`);
  } catch (err) { console.error(`scan: --octocode skipped (${String(err.message || err).split('\n')[0]}). Install octocode and set OCTOCODE_BETA=true.`); }
}

// ---- write -------------------------------------------------------------------------------
const langTotals = {}; for (const i of fileInfo.values()) langTotals[i.lang] = (langTotals[i.lang] || 0) + 1;
const model = {
  version: 1,
  meta: { name: comps.get('.')?.name || basename(ROOT), root: ROOT, generatedAt: new Date().toISOString(),
    scanner: { files: all.length, codeFiles: code.length, truncated, languages: langTotals, unresolvedImports: unresolved, unresolvedSamples, topology } },
  nodes, edges: [...edgeMap.values()], findings,
  files: { list: code.map((f) => [f, idx.get(fileComp.get(f)) ?? -1, fileInfo.get(f).lang, fileInfo.get(f).loc]), edges: fileEdges },
};
mkdirSync(dirname(OUT), { recursive: true });
writeFileSync(OUT, JSON.stringify(model));

// compact summary for the agent
const deg = new Map(); for (const e of model.edges) { deg.set(e.source, (deg.get(e.source) || [0, 0])); deg.set(e.target, (deg.get(e.target) || [0, 0])); deg.get(e.source)[1]++; deg.get(e.target)[0]++; }
const rows = nodes.map((n) => `${n.id}\t${n.kind}/${n.layer}\t${n.files}f ${n.loc}loc\tin${deg.get(n.id)?.[0] || 0}/out${deg.get(n.id)?.[1] || 0}${n.parent ? `\t<${n.parent}` : ''}\t${Object.keys(n.signals || {}).join(',')}${n.tech?.length ? `\t[${n.tech.join(',')}]` : ''}`);
console.log(`scan: ${OUT}\nroot=${ROOT} files=${all.length} code=${code.length}${truncated ? ' TRUNCATED' : ''} langs=${JSON.stringify(langTotals)} unresolvedImports=${unresolved}`);
console.log(`nodes=${nodes.length} edges=${model.edges.length} fileEdges=${fileEdges.length} findings=${findings.length}\n# id\tkind/layer\tsize\tdegree\tparent\tsignals\ttech`);
console.log(rows.join('\n'));
const top = [...model.edges].sort((a, b) => b.weight - a.weight).slice(0, 25).map((e) => `${e.source} -${e.kind}(${e.weight})-> ${e.target}`);
console.log(`# heaviest edges\n${top.join('\n')}`);
