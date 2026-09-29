import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { canonical, grade, withCredential } from './grading.mjs';
import { stableCatalog, toolFailed } from './run.mjs';
const expected = { id: 'any-case', value: 'Drift', valueType: 'variant', enumType: 'GraphAnalysis',
  sourceLine: 'let graph = GraphAnalysis::Drift;', path: '/fixture/source.rs', line: 98 };
const answer = { items: [{ id: expected.id, value: 'Drift', evidence: [{ path: expected.path, line: 98 }] }] };
const call = { event: 'call', admitted: true, name: 'localFetch', args: { queries: [{ path: expected.path }] }, result: { structuredContent: {
  base: '/fixture', results: [{ index: 0, data: { path: 'source.rs', content: expected.sourceLine, sourceLineRanges: [{ start: 98, end: 98 }] } }],
} } };
const check = (want, result, calls) => grade(want, result, calls);
assert.equal(check(expected, answer, [call]), true);
assert.equal(check(expected, answer, []), false);
assert.equal(check(expected, answer, [{ ...call, name: 'clasify' }]), false);
assert.equal(check(expected, answer, [{ ...call, admitted: false }]), false);
assert.equal(check(expected, answer, [{ ...call, result: { ...call.result, isError: true } }]), false);
assert.equal(check(expected, { items: [{ ...answer.items[0], value: 'Dependencies' }] }, [call]), false);
assert.equal(check(expected, { items: [{ ...answer.items[0], value: 'super::GraphAnalysis::Drift' }] }, [call]), true);
assert.equal(check(expected, { items: [{ ...answer.items[0], value: 'WrongEnum::Drift' }] }, [call]), false);
assert.equal(check(expected, { items: [{ ...answer.items[0], evidence: [{ path: expected.path, line: 97 }] }] }, [call]), false);
assert.equal(check(expected, { items: [answer.items[0], answer.items[0]] }, [call]), false);
assert.equal(check(expected, answer, [{ ...call, args: { path: '/outside/source.rs' } }]), false);
assert.equal(check(expected, answer, [{ ...call, result: { structuredContent: {
  results: [{ data: { content: expected.sourceLine, sourceLineRanges: [{ start: 95, end: 100 }] } }],
} } }]), false);
const paged = { ...call, result: { structuredContent: { base: '/fixture', results: [] }, content: [{ type: 'text', text:
  '# Response page 1/2.\nbase: /fixture\n\nresult: 0\ndata:\n  path: source.rs\ncontent (source lines):\n98: ' + expected.sourceLine + '\n' }] } };
assert.equal(check(expected, answer, [paged]), true);
for (const [from, to] of [['98:', '97:'], ['path: source.rs', 'path: ../outside.rs'], ['content (source lines):', 'hints:'], ['result: 0', 'result: 1']]) {
  const changed = structuredClone(paged); changed.result.content[0].text = changed.result.content[0].text.replace(from, to);
  assert.equal(check(expected, answer, [changed]), false);
}
// Current renderer: a sole row has no `result:`/`data:` wrapper and the gutter is `N:`.
const flat = { ...call, result: { structuredContent: { base: '/fixture', results: [] }, content: [{ type: 'text', text:
  '# Response page 1/2.\nbase: /fixture\npath: source.rs\ncontent (source lines):\n98:' + expected.sourceLine + '\n' }] } };
assert.equal(check(expected, answer, [flat]), true);
for (const [from, to] of [['98:', '97:'], ['path: source.rs', 'path: ../outside.rs']]) {
  const changed = structuredClone(flat); changed.result.content[0].text = changed.result.content[0].text.replace(from, to);
  assert.equal(check(expected, answer, [changed]), false);
}
const flatBatch = structuredClone(flat); flatBatch.args.queries.push({ path: expected.path });
assert.equal(check(expected, answer, [flatBatch]), false);
const continuation = { ...call, result: { structuredContent: { results: [], responsePagination: { scope: 'content.text', currentPage: 2 } },
  content: [{ type: 'text', text: '# Response page 2/2.\n98: ' + expected.sourceLine + '\n' }] } };
assert.equal(check(expected, answer, [continuation]), true);
const ambiguousContinuation = structuredClone(continuation); ambiguousContinuation.args.queries.push({ path: expected.path });
assert.equal(check(expected, answer, [ambiguousContinuation]), false);
const jsonText = { ...call, result: { structuredContent: { results: [] }, content: [{ type: 'text', text: JSON.stringify(call.result.structuredContent) }] } };
assert.equal(check(expected, answer, [jsonText]), true);
const shared = structuredClone(call); shared.result.structuredContent.shared = shared.result.structuredContent.results[0].data;
shared.result.structuredContent.results[0].data = {};
assert.equal(check(expected, answer, [shared]), false); // shared belongs to leaf collections, never a fetch body.
const search = { ...call, name: 'localSearch', args: { path: '/fixture' }, result: { structuredContent: { base: '/fixture', results: [{ data: { files: [
  { path: 'source.rs', matches: [{ line: 98, value: expected.sourceLine }] },
] } }] } } };
assert.equal(check(expected, answer, [search]), true);
const wrongSearch = structuredClone(search); wrongSearch.result.structuredContent.results[0].data.files[0].matches[0].line = 99;
assert.equal(check(expected, answer, [wrongSearch]), false);
assert.equal(canonical('a, b, c', { valueType: 'identifiers' }), 'a,b,c');
assert.notEqual(canonical('b,a,c', { valueType: 'identifiers' }), 'a,b,c');
assert.equal(canonical('a,,c', { valueType: 'identifiers' }), null);
assert.equal(canonical('1e2', { valueType: 'line' }), null);
const directory = mkdtempSync(join(tmpdir(), 'graph-grader-test-'));
try {
  const source = join(directory, 'source'), destination = join(directory, 'copy'); writeFileSync(source, 'test-only');
  await withCredential(source, destination, async () => { assert.equal(existsSync(destination), true); });
  assert.equal(existsSync(destination), false);
  await assert.rejects(withCredential(source, destination, async () => { throw new Error('failure'); }), /failure/);
  assert.equal(existsSync(destination), false);
} finally { rmSync(directory, { recursive: true, force: true }); }
assert.equal(toolFailed({ structuredContent: { queries: [{ resources: [{ pages: [{ error: { code: 'provider' } }] }] }] } }), true);
assert.equal(toolFailed({ structuredContent: { queries: [{ resources: [{ coverage: 'partial', pages: [{}] }] }] } }), false);
const frozenCatalog = [{ name: 'localFetch', inputSchema: { type: 'object' } }];
const catalogEvent = { event: 'catalog', tools: frozenCatalog };
assert.equal(stableCatalog([], frozenCatalog), false);
assert.equal(stableCatalog([catalogEvent, structuredClone(catalogEvent)], frozenCatalog), true);
assert.equal(stableCatalog([catalogEvent, { event: 'catalog', tools: [] }], frozenCatalog), false);
console.log('Graph-research canonical grading, source-evidence and credential-cleanup checks passed.');
