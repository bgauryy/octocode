#!/usr/bin/env node
// Runs the per-tool check files in this folder against a local `octocode` binary over MCP stdio.
// Each tool file checks the tool's description and schema as served, then runs research queries and
// follows the calls the responses offer (related[], next.*, responsePagination.next) into the other tools.
//
//   node packages/octocode-benchmark/tool-checks/run.mjs [tool ...] [--bin PATH] [--billed] [--out FILE]
//
// --bin    the binary (default: the staged local build for this platform)
// --billed also run steps marked "billed" (clasify provider requests); unbilled steps always run
// --out    write the full JSON report there (default: a file in the OS temp folder)
import { spawn } from 'node:child_process';
import { mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync, existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import { homedir, tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '../../..');
const require = createRequire(import.meta.url);

const argv = process.argv.slice(2);
const flag = name => argv.includes(name);
const option = name => (argv.includes(name) ? argv[argv.indexOf(name) + 1] : undefined);
const wanted = argv.filter((arg, i) => !arg.startsWith('--') && !['--bin', '--out'].includes(argv[i - 1]));

function localBinary() {
  const { getPlatformSuffix, executableName } = require(path.join(ROOT, 'packages/octocode-native/bin/platform.cjs'));
  return path.join(ROOT, 'packages/octocode-native/npm', getPlatformSuffix(), executableName('octocode'));
}

function classificationKey() {
  const file = path.join(homedir(), '.octocode/.env');
  if (!existsSync(file)) return undefined;
  const line = readFileSync(file, 'utf8').split('\n').find(l => l.startsWith('OCTOCODE_CLASSIFICATION_API='));
  return line?.slice(line.indexOf('=') + 1).trim().replace(/^['"]|['"]$/g, '') || undefined;
}

function isolatedEnv(home) {
  const drop = ['GITHUB_', 'GH_', 'OCTOCODE_', 'NPM_CONFIG_', 'npm_config_', 'GIT_'];
  const env = Object.fromEntries(Object.entries(process.env).filter(([k]) => !drop.some(p => k.startsWith(p))));
  Object.assign(env, {
    OCTOCODE_HOME: home, OCTOCODE_STORAGE_MODE: 'persistent', OCTOCODE_GRAPH_HOME: path.join(home, 'graph'),
    OCTOCODE_LSP_AUTO_INSTALL: 'off', OCTOCODE_LSP_PREWARM: 'off', OCTOCODE_BETA: 'true', OCTOCODE_ENABLE_CLONE: 'true',
    OCTOCODE_SECURITY_CHECKS: 'true', GIT_TERMINAL_PROMPT: '0', GIT_CONFIG_NOSYSTEM: '1', NO_COLOR: '1',
  });
  const key = classificationKey();
  if (key) env.OCTOCODE_CLASSIFICATION_API = key; // never printed
  return env;
}

class Mcp {
  constructor(bin, env) {
    this.child = spawn(bin, ['mcp'], { cwd: ROOT, env, stdio: ['pipe', 'pipe', 'ignore'] });
    this.buffer = '';
    this.waiting = new Map();
    this.next = 0;
    this.child.stdout.on('data', chunk => {
      this.buffer += chunk;
      let at;
      while ((at = this.buffer.indexOf('\n')) >= 0) {
        const line = this.buffer.slice(0, at);
        this.buffer = this.buffer.slice(at + 1);
        let message;
        try { message = JSON.parse(line); } catch { continue; }
        const resolve = this.waiting.get(message.id);
        if (resolve && ('result' in message || 'error' in message)) { this.waiting.delete(message.id); resolve(message); }
      }
    });
  }
  rpc(method, params, ms = 600_000) {
    const id = ++this.next;
    this.child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id, method, params })}\n`);
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { this.waiting.delete(id); reject(new Error(`timeout ${method}`)); }, ms);
      this.waiting.set(id, m => { clearTimeout(timer); resolve(m); });
    });
  }
  async start() {
    await this.rpc('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'tool-checks', version: '1' } });
    this.child.stdin.write('{"jsonrpc":"2.0","method":"notifications/initialized"}\n');
  }
  async call(tool, args) {
    const started = performance.now();
    const message = await this.rpc('tools/call', { name: tool, arguments: args });
    const wall = (performance.now() - started) / 1000;
    if (message.error) return { wall, chars: 0, content: null, protocolError: message.error.message };
    const result = message.result;
    const chars = (result.content ?? []).reduce((n, part) => n + (part.text?.length ?? 0), 0);
    let content = result.structuredContent;
    if (content === undefined) { try { content = JSON.parse(result.content[0].text); } catch { content = null; } }
    return { wall, chars, content };
  }
  close() { this.child.stdin.end(); setTimeout(() => this.child.kill(), 5000).unref(); }
}

const wrap = row => (row.queries || row.cursor ? row : { queries: [{ mainGoal: 'tool check', reasoning: 'tool check', ...row }] });
const rowOf = content => content?.results?.[0] ?? {};
const errorOf = content => content?.errorCode ?? rowOf(content)?.data?.errorCode ?? rowOf(content)?.errorCode;

/** Values at a path: `data.files[].path` from the first row, `$.x` from the response root. */
function at(content, dotted) {
  let values = [dotted.startsWith('$.') ? content : rowOf(content)];
  for (const part of dotted.replace(/^\$\./, '').split('.')) {
    const many = part.endsWith('[]');
    const key = many ? part.slice(0, -2) : part;
    values = values.flatMap(v => {
      const x = v?.[key];
      return many ? (Array.isArray(x) ? x : []) : x === undefined ? [] : [x];
    });
  }
  return values;
}

/** Calls the response offers: related[] (row and envelope), next.* and responsePagination.next. */
function offered(content) {
  const row = rowOf(content).data ?? {};
  const calls = [];
  for (const c of [...(row.related ?? []), ...(content?.related ?? []), ...(content?.shared?.related ?? [])]) {
    if (c?.tool && c.input) calls.push({ kind: `related:${c.tool}`, tool: c.tool, input: c.input });
  }
  for (const [key, c] of Object.entries(row.next ?? {})) {
    if (c?.tool && c.input) calls.push({ kind: `next:${key}`, tool: c.tool, input: c.input });
  }
  const page = content?.responsePagination?.next;
  if (page) calls.push({ kind: 'page', tool: null, input: page.input ?? page });
  return calls;
}

function judge(step, result) {
  const failures = [];
  const e = step.expect ?? {};
  const err = errorOf(result.content);
  if (result.protocolError) failures.push(`protocol error: ${result.protocolError}`);
  if (e.error) {
    if (err !== e.error) failures.push(`expected error ${e.error}, got ${err ?? 'none'}`);
    const c = result.content ?? {};
    if (!(c.hints?.length || c.details?.length || rowOf(c).data?.hints?.length)) failures.push('error gives no hint or example');
  } else if (err) failures.push(`unexpected error ${err}: ${(result.content?.error ?? '').slice(0, 160)}`);
  if (e.status && (rowOf(result.content).status ?? 'ok') !== e.status) failures.push(`status ${rowOf(result.content).status ?? 'ok'} != ${e.status}`);
  for (const p of e.has ?? []) if (!at(result.content, p).length) failures.push(`missing ${p}`);
  for (const [p, n] of Object.entries(e.min ?? {})) {
    const v = at(result.content, p);
    const count = v.length === 1 && typeof v[0] === 'number' ? v[0] : v.length;
    if (count < n) failures.push(`${p} = ${count} < ${n}`);
  }
  for (const s of e.contains ?? []) if (!JSON.stringify(result.content ?? '').includes(s)) failures.push(`no "${s}"`);
  if (e.maxChars && result.chars > e.maxChars) failures.push(`${result.chars} chars > ${e.maxChars}`);
  if (e.offers && !offered(result.content).some(c => c.kind.startsWith(e.offers))) failures.push(`offers no ${e.offers}`);
  return failures;
}

function surface(tool, listed, enabled) {
  const def = listed.find(t => t.name === tool);
  if (!def) return { listed: false };
  const items = def.inputSchema?.properties?.queries?.items;
  const variants = items?.oneOf ?? items?.anyOf ?? [items ?? {}];
  const fields = new Map();
  for (const v of variants) for (const [k, s] of Object.entries(v?.properties ?? {})) {
    if (!fields.has(k) || s.description) fields.set(k, s);
  }
  // Names that say what they hold; describing them only adds bytes every agent pays for.
  const selfExplanatory = ['mainGoal', 'reasoning', 'owner', 'repo', 'path', 'limit', 'number', 'ref', 'version', 'packageName'];
  const undescribed = [...fields].filter(([k, s]) => !s.description && !selfExplanatory.includes(k) && !s.enum && !s.const).map(([k]) => k);
  const text = def.description ?? '';
  const peers = enabled.filter(n => n !== tool && text.includes(n));
  return {
    listed: true, descriptionChars: text.length, schemaChars: JSON.stringify(def.inputSchema ?? {}).length,
    fields: fields.size, undescribed, hasExample: /Example/.test(text), hasWhen: /<when>|Use (for|when|to)/.test(text), mentions: peers,
  };
}

async function runTool(file, server, listed, enabled, billed) {
  const spec = JSON.parse(readFileSync(path.join(HERE, file), 'utf8'));
  const report = { tool: spec.tool, surface: surface(spec.tool, listed, enabled), steps: [] };
  const results = [];
  for (const [i, step] of spec.steps.entries()) {
    if (step.billed && !billed) { report.steps.push({ name: step.name, skipped: 'billed' }); results.push(null); continue; }
    let tool = step.tool;
    let args = step.args && wrap(step.args);
    if (step.follow) {
      const source = results[step.from ?? i - 1];
      const call = source && offered(source.content).find(c => c.kind === step.follow || c.kind.startsWith(step.follow));
      if (!call) { report.steps.push({ name: step.name, failures: [`no offered call ${step.follow}`] }); results.push(null); continue; }
      tool = call.tool ?? source.tool;
      args = call.input;
    }
    const result = { ...(await server.call(tool, args)), tool };
    results.push(result);
    report.steps.push({
      name: step.name, tool, follow: step.follow ?? null, expectsError: Boolean(step.expect?.error), chars: result.chars, wall: Number(result.wall.toFixed(2)),
      offers: offered(result.content).map(c => c.kind), failures: judge(step, result),
    });
  }
  return report;
}

async function main() {
  const bin = option('--bin') ?? localBinary();
  const billed = flag('--billed');
  const files = readdirSync(HERE).filter(f => f.endsWith('.json')).filter(f => !wanted.length || wanted.includes(f.replace(/\.json$/, '')));
  const home = mkdtempSync(path.join(tmpdir(), 'octocode-tool-checks-'));
  const server = new Mcp(bin, isolatedEnv(home));
  const reports = [];
  try {
    await server.start();
    const listed = (await server.rpc('tools/list', {})).result.tools;
    const enabled = listed.map(t => t.name);
    for (const file of files) reports.push(await runTool(file, server, listed, enabled, billed));
  } finally {
    server.close();
  }
  const out = option('--out') ?? path.join(tmpdir(), `octocode-tool-checks-${Date.now()}.json`);
  writeFileSync(out, `${JSON.stringify({ bin, at: new Date().toISOString(), reports }, null, 2)}\n`);
  console.log('| Tool | Description (chars, example, when, mentions) | Schema (chars, fields, undescribed) | Steps passed | Follow-ups ran | Clear errors | Chars | Wall s |');
  console.log('|---|---|---|---|---|---|---|---|');
  let failed = 0;
  for (const r of reports) {
    const s = r.surface;
    const run = r.steps.filter(x => !x.skipped);
    const pass = run.filter(x => !x.failures?.length).length;
    const follows = run.filter(x => x.follow);
    const followOk = follows.filter(x => !x.failures?.length).length;
    failed += run.length - pass;
    const errors = run.filter(x => x.expectsError);
    const clear = errors.filter(x => !x.failures?.some(f => f.includes('no hint or example'))).length;
    console.log(`| ${r.tool} | ${s.listed ? `${s.descriptionChars}, ${s.hasExample ? 'yes' : 'no'}, ${s.hasWhen ? 'yes' : 'no'}, ${s.mentions.join(' ') || '-'}` : 'not listed'} | ${s.listed ? `${s.schemaChars}, ${s.fields}, ${s.undescribed.join(' ') || '0'}` : '-'} | ${pass}/${run.length} | ${followOk}/${follows.length} | ${clear}/${errors.length} | ${run.reduce((n, x) => n + (x.chars ?? 0), 0)} | ${run.reduce((n, x) => n + (x.wall ?? 0), 0).toFixed(1)} |`);
  }
  for (const r of reports) for (const x of r.steps) if (x.failures?.length) console.log(`FAIL ${r.tool} · ${x.name}: ${x.failures.join('; ')}`);
  console.log(`\nreport: ${out}`);
  rmSync(home, { recursive: true, force: true });
  process.exitCode = failed ? 1 : 0;
}

main().catch(error => { console.error(error); process.exitCode = 2; });
