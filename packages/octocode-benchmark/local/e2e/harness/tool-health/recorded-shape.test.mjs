import assert from 'node:assert/strict';
import test from 'node:test';
import { declaredUnread, fieldsOf, matcher, outputText, shapeDrift, shapeOf, trimmed, uncarried, uncarriedExchanges, xmlFields } from './recorded-shape.mjs';
import { recordedJson } from './recorded-answers.mjs';

test('a key naming data reads as one path, a declared map too', () => {
  const shape = shapeOf({ versions: { '1.2.3': { dist: 'a' }, '2.0.0': { dist: 'b' } }, tags: { latest: '2.0.0' }, deps: { lodash: '^4' } }, ['deps']);
  assert.deepEqual([...shape.keys()].filter(path => path), ['versions', 'versions.*', 'versions.*.dist', 'tags', 'tags.latest', 'deps', 'deps.*']);
});

test('drift: an addition is a note, a removal or a type change a failure, null neither', () => {
  const recorded = shapeOf({ gone: 1, kept: 'a', changed: 'a', nullable: 'a' });
  const fresh = shapeOf({ kept: 'b', changed: [1], nullable: null, added: true });
  const { notes, failures } = shapeDrift(recorded, fresh);
  assert.deepEqual(failures.map(text => text.split(':')[0]).toSorted((a, b) => a.localeCompare(b)), ['removed gone', 'type changed changed']);
  assert.ok(notes.some(text => text.startsWith('added added')), notes.join());
  assert.ok(!failures.concat(notes).some(text => text.includes('nullable')));
});

test('trimming keeps every path and type, and what the answer names', () => {
  const rows = Array.from({ length: 40 }, (_, index) => ({ version: `1.${index}.0`, ...(index === 17 && { rare: { deep: [1] } }), ...(index === 23 && { version: 7 }) }));
  const answer = { latest: '1.9.0', rows, text: 'x'.repeat(100_000), byVersion: Object.fromEntries(rows.map(row => [String(row.version), { size: 1 }])), content: { encoding: 'base64', data: 'y'.repeat(100_000) } };
  const small = trimmed(answer, { pinned: ['1.30.0'] });
  assert.ok(JSON.stringify({ ...small, content: 0 }).length < JSON.stringify({ ...answer, content: 0 }).length / 10);
  assert.deepEqual(shapeDrift(shapeOf(answer), shapeOf(small)).failures, []);
  assert.deepEqual(shapeDrift(shapeOf(small), shapeOf(answer)).notes, []);
  const versions = small.rows.map(row => row.version);
  for (const named of ['1.0.0', '1.39.0', '1.9.0', '1.30.0']) assert.ok(versions.includes(named), named);
  assert.equal(small.content.data, answer.content.data, 'an encoded text stays decodable');
  assert.equal(trimmed({ rows }, { whole: ['rows'] }).rows.length, rows.length, 'a routing list stays whole');
});

test('a field stays when a sibling key names data', () => {
  const answer = { params: { 'facet.field': 'a', 'fl.x': 'b', 'q.y': 'c', 'r.z': 'd', 's.t': 'e', indent: 'off', wt: 'json', core: 'x' } };
  assert.deepEqual(shapeDrift(shapeOf(answer), shapeOf(trimmed(answer))).failures, []);
});

test('a field whose value never reaches the output is named, unless declared', () => {
  const answer = { id: 'GHSA-1', severity: [{ type: 'CVSS_V3', score: 'CVSS:3.1/AV:N' }], rows: [{ name: 'shown' }, { name: 'dropped' }], dates: '2025-01-02T03:04:05.123Z' };
  const text = outputText({ id: 'ghsa-1', rows: ['shown'], at: '2025-01-02T03:04:05Z' });
  assert.deepEqual(uncarried(answer, text).toSorted((a, b) => a.localeCompare(b)), ['severity[*].score', 'severity[*].type']);
  assert.deepEqual(uncarried(answer, text, { unread: ['severity'] }), [], 'a declared subtree');
  assert.deepEqual(uncarried(answer, text, { unread: ['severity[*].type'] }), ['severity[*].score']);
});

test('a real record without its field in the output fails: the cvss class', () => {
  const record = recordedJson('osv/advisory-ghsa', '/v1/vulns/');
  assert.ok(record.severity?.length, 'OSV sends a list of vectors');
  const whole = outputText(record);
  assert.deepEqual(uncarried(record, whole), []);
  const { severity, ...withoutVectors } = record;
  assert.ok(severity);
  const gaps = uncarried(record, outputText(withoutVectors));
  assert.ok(gaps.includes('severity[*].score'), gaps.join());
});

test('a declaration names the root of a subtree nothing of is shown', () => {
  const answer = { dist: { tarball: 'https://x/y', shasum: 'abcdef' }, kept: { shown: 'visible', hidden: 'not this' } };
  const gaps = uncarried(answer, outputText({ a: 'visible' }));
  assert.deepEqual(declaredUnread(gaps, [answer]), ['dist', 'kept.hidden']);
  assert.ok(matcher(['dist'])('dist.tarball') && matcher(['items[*].*_url'])('items[*].html_url') && !matcher(['dist'])('distant'));
});

test('XML answers have field paths; plain text has none', () => {
  const xml = '<?xml version="1.0"?><project a="b"><dependencies><dependency><artifactId>one</artifactId></dependency><dependency><artifactId>two</artifactId></dependency></dependencies><empty/></project>';
  assert.equal(xmlFields(xml).project.dependencies.dependency[1].artifactId.text, 'two');
  assert.ok(shapeOf(fieldsOf({ body: xml })).has('project.dependencies.dependency[*].artifactId.text'));
  assert.equal(fieldsOf({ body: 'module example.test/x\n' }), undefined);
  assert.deepEqual(fieldsOf({ body: { a: 1 } }), { a: 1 });
});
test('repository carriers retain source identity across URI transports and owner/repo fields', () => {
  const fields = { project: { scm: { url: { text: 'scm:git:ssh://git@GITHUB.com:22/acme/Widget.git/' } } } };
  const repositories = ['project.scm.url.text'];
  const output = outputText({ artifacts: [{ owner: 'acme', repo: 'Widget' }] });
  assert.deepEqual(uncarried(fields, output, { repositories }), []);
  for (const changed of [
    { owner: 'other', repo: 'Widget' }, { owner: 'acme', repo: 'Different' },
    { repository: 'https://elsewhere.test/acme/Widget' },
    { repository: 'https://github.com/acme/Widget?ref=another' },
    { repository: 'https://github.com/acme/Widget/child' },
  ]) assert.deepEqual(uncarried(fields, outputText(changed), { repositories }), repositories);
  assert.deepEqual(uncarried(fields, output), repositories, 'only declared repository fields normalize');
});

test('repository identity does not merge sibling owner/repo fields or arbitrary URI values', () => {
  const fields = { repository: 'ssh://git@example.test:22/team/Package.git/' };
  const repositories = ['repository'];
  assert.deepEqual(uncarried(fields, outputText({ repository: 'https://example.test/team/Package' }), { repositories }), []);
  assert.deepEqual(uncarried(fields, outputText({ repository: 'https://example.test/team/package' }), { repositories }), repositories, 'URI paths remain case-sensitive');
  const github = { repository: 'https://github.com/team/Package.git' };
  assert.deepEqual(uncarried(github, outputText([{ owner: 'team' }, { repo: 'Package' }]), { repositories }), repositories);
  assert.deepEqual(uncarried({ license: 'https://example.test/license.git/' }, outputText({ repository: 'https://example.test/license' }), { repositories }), ['license']);
});
test('a parent exchange omission never waives the same field of the requested child', () => {
  const exchanges = [
    { status: 200, body: { project: { description: 'Child description' } } },
    { status: 200, body: { project: { description: 'Parent build metadata' } }, unread: ['project.description'] },
  ];
  assert.deepEqual(uncarriedExchanges(exchanges, outputText({ description: 'Child description' })), []);
  assert.deepEqual(uncarriedExchanges(exchanges, outputText({ description: 'Parent build metadata' })), ['project.description']);
  assert.deepEqual(uncarriedExchanges(exchanges, '', { unread: ['project.description'] }), []);
});
test('excluding a POM license URL never makes it evidence for the SCM source', () => {
  const exchanges = [{ status: 200, body: '<project><licenses><license><url>https://licenses.example.test/custom</url></license></licenses><scm><url>https://github.com/team/source.git</url></scm></project>', unread: ['project.licenses.license.url'] }];
  const options = { repositories: ['project.scm.url.text'] };
  assert.deepEqual(uncarriedExchanges(exchanges, outputText({ owner: 'team', repo: 'source' }), options), []);
  assert.deepEqual(uncarriedExchanges(exchanges, outputText({ repository: 'https://licenses.example.test/custom' }), options), ['project.scm.url.text']);
});
test('a declared release day keeps its date, and a verified tag keeps its namespace', () => {
  const release = { Time: '2020-01-14T19:28:13Z' };
  assert.deepEqual(uncarried(release, outputText({ publishedAt: '2020-01-14' }), { dates: ['Time'] }), []);
  assert.deepEqual(uncarried(release, outputText({ publishedAt: '2020-01-15' }), { dates: ['Time'] }), ['Time']);
  assert.deepEqual(uncarried(release, outputText({ publishedAt: '2020-01-14' })), ['Time'], 'only declared date projections drop time precision');
  const tag = { ref: 'refs/tags/v1.2.3' };
  assert.deepEqual(uncarried(tag, outputText({ ref: 'v1.2.3', verification: 'tag' })), []);
  assert.deepEqual(uncarried(tag, outputText({ ref: 'v1.2.4', verification: 'tag' })), ['ref']);
  assert.deepEqual(uncarried(tag, outputText({ ref: 'v1.2.3', verification: 'defaultBranch' })), ['ref']);
});
