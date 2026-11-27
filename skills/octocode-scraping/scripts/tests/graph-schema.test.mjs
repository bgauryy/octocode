import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { buildSiteGraph, buildUnifiedGraph, buildWorkflowIndex } from '../lib/analyzers.mjs';

const schema = JSON.parse(readFileSync(new URL('../schemas/graph.schema.json', import.meta.url), 'utf8'));

// The schema keywords graph.schema.json uses; an unknown keyword fails so the check cannot silently pass.
const KNOWN = new Set(['$schema', '$id', 'title', 'description', 'type', 'required', 'properties', 'items', 'enum', 'const', 'minimum', 'format', 'additionalProperties']);
const typeOf = (v) => (v === null ? 'null' : Array.isArray(v) ? 'array' : Number.isInteger(v) ? 'integer' : typeof v);

function typeErrors(value, s, at) {
  if (!s.type) return [];
  const types = [s.type].flat();
  const t = typeOf(value);
  return types.includes(t) || (t === 'integer' && types.includes('number')) ? [] : [`${at}: ${t} is not ${types.join('|')}`];
}

function valueErrors(value, s, at) {
  const errors = [];
  if ('const' in s && value !== s.const) errors.push(`${at}: expected ${s.const}`);
  if (s.enum && !s.enum.includes(value)) errors.push(`${at}: ${value} not in enum`);
  if (s.minimum !== undefined && value < s.minimum) errors.push(`${at}: below ${s.minimum}`);
  for (const key of s.required ?? []) if (!(key in value)) errors.push(`${at}: missing ${key}`);
  return errors;
}

function validate(value, s, at = '$') {
  for (const key of Object.keys(s)) if (!KNOWN.has(key)) throw new Error(`${at}: unsupported schema keyword ${key}`);
  const wrongType = typeErrors(value, s, at);
  if (wrongType.length) return wrongType;
  const errors = valueErrors(value, s, at);
  for (const [key, sub] of Object.entries(s.properties ?? {})) if (value && key in value) errors.push(...validate(value[key], sub, `${at}.${key}`));
  if (s.items && Array.isArray(value)) value.forEach((item, i) => errors.push(...validate(item, s.items, `${at}[${i}]`)));
  return errors;
}

test('the automation graph the corpus writes matches graph.schema.json', () => {
  const rootUrl = 'https://site.test/';
  const pageMaps = [
    { pageId: 'p0', url: 'https://site.test/', title: 'Home', status: 200 },
    { pageId: 'p1', url: 'https://site.test/login', title: 'Login', status: 200 },
  ];
  const sources = pageMaps.map((p) => ({ pageId: p.pageId, url: p.url }));
  const linksAll = [{ pageId: 'p0', href: 'https://site.test/login', text: 'Sign in' }];
  const headingsAll = [{ pageId: 'p1', level: 1, text: 'Sign in' }];
  const elementRows = [
    { _file: 'forms', pageId: 'p1', action: 'https://site.test/session', selector: 'form#login', workflowHint: 'auth', inputs: [{ name: 'email', type: 'email' }] },
    { _file: 'buttons', pageId: 'p0', selector: 'button.cta', text: 'Start' },
  ];
  const resourcesAll = [{ pageId: 'p0', src: 'https://site.test/report.pdf', kind: 'link' }];
  const siteGraph = buildSiteGraph({ rootUrl, pageMaps, sources, linksAll, headingsAll, elementRows });
  const workflowIndex = buildWorkflowIndex({ rootUrl, pageMaps, linksAll, headingsAll, elementRows });
  const graph = buildUnifiedGraph({ rootUrl, pageMaps, siteGraph, workflowIndex, elementRows, resourcesAll });
  assert.deepEqual(validate(graph, schema), []);
  // Every node kind the producer emitted is one the schema names.
  assert.ok(graph.nodes.some((n) => n.kind === 'form') && graph.nodes.some((n) => n.kind === 'input'));
});

test('the schema check rejects drift', () => {
  assert.notDeepEqual(validate({ schemaVersion: 1, rootUrl: 'x', totals: { nodes: 0, edges: 0 }, nodes: [{ id: 'a', kind: 'widget', url: 'u' }], edges: [] }, schema), []);
});
