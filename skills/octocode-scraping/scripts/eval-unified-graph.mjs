#!/usr/bin/env node
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const here = new URL('.', import.meta.url).pathname;
const fetchScript = resolve(here, 'scrapingant-fetch.mjs');
const root = resolve(process.cwd());
const outBase = join(root, '.octocode/tmp/scrape-unified-graph-eval');
const checks = [];
const assert = (name, condition, detail = '') => checks.push({ name, ok: Boolean(condition), detail });

await rm(outBase, { recursive: true, force: true });
await mkdir(outBase, { recursive: true });

const html = `<!doctype html><html><head><title>Hub</title><script src="/app.js"></script></head><body><h1>Hub</h1><nav><a href="/#__skip">Skip to main content</a><a href="#">Products</a><a href="/pricing">Pricing</a><a href="/docs">Docs</a><a href="https://other.example.com/partner">Partner</a></nav><form action="/signup" method="post"><label>Email</label><input type="email" name="email"><button>Start trial</button></form><table><tr><th>Plan</th><th>Price</th></tr></table><nav class="pagination-nav"><a class="pagination-nav__link--next" href="/page/2">Next</a></nav></body></html>`;
const mockFile = join(outBase, 'mock-hub.html');
await writeFile(mockFile, html);

const res = spawnSync(process.execPath, [fetchScript, '--url', 'https://example.com', '--mode', 'html', '--session', 'unified-graph', '--mock-status', '200', '--mock-content-type', 'text/html', '--mock-body-file', mockFile, '--out', outBase], { cwd: root, encoding: 'utf8' });
checks.push({ name: 'fetch runs', ok: res.status === 0, status: res.status, stderr: res.stderr.slice(0, 500) });

const dir = join(outBase, 'unified-graph');
assert('graph.json exists', existsSync(join(dir, 'graph/graph.json')));
assert('schemas/graph.schema.json copied into session', existsSync(join(dir, 'schemas/graph.schema.json')));

const graph = JSON.parse(await readFile(join(dir, 'graph/graph.json'), 'utf8'));
const schema = JSON.parse(await readFile(join(dir, 'schemas/graph.schema.json'), 'utf8'));

// Minimal structural validation against the schema's own required-field contract (no external validator dependency).
function validateAgainstSchema(doc, schemaDef, path = '$') {
  const errors = [];
  for (const key of schemaDef.required || []) if (!(key in doc)) errors.push(`${path}: missing required "${key}"`);
  for (const [key, propSchema] of Object.entries(schemaDef.properties || {})) {
    if (!(key in doc)) continue;
    const value = doc[key];
    if (propSchema.type === 'array' && propSchema.items) {
      if (!Array.isArray(value)) { errors.push(`${path}.${key}: expected array`); continue; }
      value.forEach((item, i) => errors.push(...validateAgainstSchema(item, propSchema.items, `${path}.${key}[${i}]`)));
    } else if (propSchema.type === 'object' && propSchema.properties) {
      errors.push(...validateAgainstSchema(value, propSchema, `${path}.${key}`));
    } else if (propSchema.enum && !propSchema.enum.includes(value)) {
      errors.push(`${path}.${key}: "${value}" not in enum [${propSchema.enum.join(', ')}]`);
    }
  }
  return errors;
}
const schemaErrors = validateAgainstSchema(graph, schema);
assert('graph.json conforms to graph.schema.json', schemaErrors.length === 0, schemaErrors.join('; '));

assert('page node for root exists', graph.nodes.some((n) => n.kind === 'page' && n.url === 'https://example.com'));
assert('graph schema version is v2', graph.schemaVersion === 2);
assert('pricing workflow attached to page node', graph.nodes.find((n) => n.kind === 'page').workflowTypes.includes('pricing'));
assert('same-host link becomes a resolvable edge target', graph.edges.some((e) => e.to.startsWith('link:') && e.to.includes('/pricing') && e.kind === 'navigates_to'));
assert('subdomain/external link present as link node', graph.nodes.some((n) => n.kind === 'link' && n.url.includes('other.example.com')));
assert('form node captured with risk', graph.nodes.some((n) => n.kind === 'form' && n.risk === 'user-data-required'));
assert('button node captured', graph.nodes.some((n) => n.kind === 'button' && /Start trial/.test(n.text || '')));
assert('table node captured', graph.nodes.some((n) => n.kind === 'table'));
assert('resource node captured', graph.nodes.some((n) => n.kind === 'resource' && n.url.endsWith('/app.js')));
assert('pagination edge captured', graph.edges.some((e) => e.kind === 'paginates_to' && e.to.includes('/page/2')));
assert('boilerplate skip/hash links filtered from graph edges', !graph.edges.some((e) => /Skip to main content|Products/.test(e.label || '')));
assert('every edge has source evidence', graph.edges.every((e) => e.source && e.source.file));
assert('totals match actual node/edge counts', graph.totals.nodes === graph.nodes.length && graph.totals.edges === graph.edges.length);

const failed = checks.filter((c) => !c.ok);
console.log(JSON.stringify({ ok: failed.length === 0, checks }, null, 2));
process.exit(failed.length === 0 ? 0 : 1);
