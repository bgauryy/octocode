import assert from 'node:assert/strict';
import test from 'node:test';
import { declaredUnread, fieldsOf, matcher, outputText, shapeDrift, shapeOf, trimmed, uncarried, uncarriedExchanges, xmlFields } from './recorded-shape.mjs';
import { readCase, recordedJson } from './recorded-answers.mjs';

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
});

test('nothing another request of the call carries is trimmed, at any depth, list or map', () => {
  const ids = Array.from({ length: 40 }, (_, index) => `ID-${String(index).padStart(3, '0')}`);
  const answer = {
    results: [{ vulns: ids.map(id => ({ id, modified: '2024-01-01T00:00:00Z', package: 'shared-name' })) }],
    versions: Object.fromEntries(ids.map((id, index) => [`2.${index}.0`, { name: 'shared-name' }])),
    readme: `${'z'.repeat(2000)}-routed`,
  };
  const routed = [ids[7], ids[23], ids[31]];
  const requests = [...routed.map(id => ({ url: `https://provider.test/v1/items/${encodeURIComponent(id)}` })),
    { url: 'https://provider.test/v1/batch', body: { queries: [{ version: '2.17.0' }] } },
    { url: 'https://provider.test/v1/text', body: answer.readme }];
  const small = trimmed(answer, { requests });
  const kept = small.results[0].vulns.map(row => row.id);
  for (const id of routed) assert.ok(kept.includes(id), `${id} routed a request`);
  assert.ok(kept.length < ids.length, 'a value every row holds routes to no row in particular');
  assert.ok(Object.hasOwn(small.versions, '2.17.0'), 'a data key a request names');
  assert.equal(small.readme, answer.readme, 'a text a request carries whole');
  assert.ok(trimmed(answer).results[0].vulns.length < ids.length, 'with no other request the list is cut');
});

/** The rows of `rows` a trim keeps when `url` is another request of the call. */
const keptFor = (rows, url, key = 'id') => trimmed({ rows }, { requests: [{ url }] }).rows.map(row => row[key]);
const rowsOf = make => Array.from({ length: 20 }, (_, index) => make(index));

test('a request carries a value in its own spelling: case, escapes, plus signs, parts and small numbers', () => {
  const cases = [
    ['a case-folded id', rowsOf(i => ({ id: `Pkg.Name${i}` })), 'https://api.nuget.org/v3/registration5-gz-semver2/pkg.name9/index.json', 'Pkg.Name9'],
    ['a small number', rowsOf(i => ({ id: i + 1 })), 'https://api.github.com/repos/o/r/pulls/12', 12],
    ['a plus-encoded query', rowsOf(i => ({ id: `psr log ${i}` })), 'https://packagist.test/search.json?q=psr+log+9', 'psr log 9'],
    ['an escape beside a malformed one', rowsOf(i => ({ id: `@scope/p${i}` })), 'https://registry.test/%40scope%2Fp9?x=100%', '@scope/p9'],
    ['a two-character id', rowsOf(i => ({ id: `a${i}` })), 'https://registry.test/items/a9', 'a9'],
  ];
  for (const [label, rows, url, routed] of cases) {
    const kept = keptFor(rows, url);
    assert.ok(kept.includes(routed), `${String(label)}: ${JSON.stringify(kept)}`);
    assert.ok(kept.length < rows.length, `${String(label)}: the rest is still cut`);
  }
  const repositories = rowsOf(i => ({ id: `n${i}`, repository: `git+https://github.com/own${i}/repo${i}.git` }));
  assert.ok(keptFor(repositories, 'https://api.github.com/repos/own9/repo9').includes('n9'), 'a request built from a part of a value');
  const readme = `${'x'.repeat(1500)} https://github.com/o9/r9 `;
  assert.equal(trimmed({ readme }, { requests: [{ url: 'https://api.github.com/repos/o9/r9' }] }).readme, readme.trimEnd(), 'a long text keeps the part a request carries');
});

test('only what this answer could have given routes, and a routed long text keeps only up to its last routed value', () => {
  const words = ['fix', 'log', 'add', 'test', 'docs', 'api', 'json', 'bump'];
  const commits = rowsOf(i => ({ sha: `${(i * 7919).toString(16).padStart(8, '0')}deadbeef${i}`, message: `${words[i % 8]} thing ${i}`, comments: i % 5, total: i }));
  const many = Array.from({ length: 100 }, (_, i) => commits[i % 20] && { ...commits[i % 20], sha: `${commits[i % 20].sha}-${i}`, total: i });
  const kept = (requests, own) => trimmed({ rows: many }, { requests, own }).rows.length;
  const alone = kept([]);
  const paging = { url: 'https://api.github.com/repos/o/r/commits?per_page=100' };
  assert.ok(kept([{ url: 'https://api.github.com/repos/o/r/commits?per_page=100&page=2' }], paging) <= alone + 2, 'the next page pins at most one row per value it carries');
  assert.ok(kept([{ url: 'https://registry.test/search?q=json+log' }]) <= alone + 2, 'two common words pin one row each at most');
  const sha = many[40].sha;
  assert.ok(trimmed({ rows: many }, { requests: [{ url: `https://api.github.com/repos/o/r/commits/${sha}` }] }).rows.some(row => row.sha === sha), 'the row a request was built from stays');

  const readme = 'Requests is a simple HTTP library for Python. Install it with pip and read the JSON docs. '.repeat(40);
  const own = { url: 'https://pypi.org/pypi/requests/2.19.0/json' };
  const osv = { url: 'https://api.osv.dev/v1/querybatch', body: { queries: [{ package: { name: 'requests', ecosystem: 'PyPI' }, version: '2.19.0' }] } };
  assert.equal(trimmed({ readme }, { requests: [osv], own }).readme.length, 1200, 'words the own request already carried route nothing');
  const linked = `${'x'.repeat(1500)} https://github.com/o9/r9 ${'y'.repeat(2000)}`;
  const cut = trimmed({ linked }, { requests: [{ url: 'https://api.github.com/repos/o9/r9' }] }).linked;
  assert.ok(cut.endsWith('https://github.com/o9/r9'), `cut just past the routed value: ${cut.slice(-30)}`);
  assert.equal(trimmed({ linked: `${'x'.repeat(100)} o9 ${'y'.repeat(3000)}` }, { requests: [{ url: 'https://x.test/o9' }] }).linked.length, 1200, 'never shorter than the plain cut');
});

test('a row a request combines two values of survives, and so does a hit that extends the asked name', () => {
  const versions = [{ n: 'aaa', v: '9.0.0' }, { n: 'xpkg', v: '1.3.0', extra: 'adds-a-path' }, ...rowsOf(i => ({ n: `f${i}x`, v: `5.${i}.0` })).slice(0, 6), { n: 'xpkg', v: '1.2.0' }, { n: 'ypkg', v: '1.2.0' }];
  const tarball = trimmed({ rows: versions }, { requests: [{ url: 'https://reg.test/xpkg/-/xpkg-1.2.0.tgz' }], own: { url: 'https://reg.test/-/v1/search?text=pkg' } }).rows;
  assert.ok(tarball.some(row => row.n === 'xpkg' && row.v === '1.2.0'), `the row both values came from: ${JSON.stringify(tarball)}`);
  const hits = ['underscore', 'lodash-es', 'lodash.merge', 'lodash', 'lodash.get', 'lodash-fp', 'ramda'].map(name => ({
    name, repository: `git+https://github.com/${name === 'lodash' || name.startsWith('lodash.') ? 'lodash/lodash' : `lodash/${name}`}.git`,
  }));
  hits[0].repository = 'git+https://github.com/jashkenas/underscore.git';
  hits.at(-1).repository = 'git+https://github.com/ramda/ramda.git';
  const search = { url: 'https://registry.npmjs.org/-/v1/search?text=lodash' };
  const kept = trimmed({ objects: hits }, { requests: [{ url: 'https://api.github.com/repos/lodash/lodash' }], own: search }).objects;
  assert.ok(kept.some(hit => hit.repository.includes('github.com/lodash/lodash.git')), `a hit whose repository the next request names: ${JSON.stringify(kept.map(hit => hit.name))}`);
  assert.ok(kept.length < hits.length, 'the other hits still go');
});

test('real recordings: every id, service and coordinate another request used survives trimming', () => {
  for (const [id, urlPart, rows] of [
    ['osv/package-score-only', '/v1/querybatch', body => body.results[0].vulns.map(row => row.id)],
    ['nuget/search', '/v3/index.json', body => body.resources.map(row => row['@id'])],
    ['maven/search', '/solrsearch/select', body => body.response.docs.map(row => row.a)],
  ]) {
    const { exchanges } = readCase(id);
    const index = exchanges.findIndex(({ request }) => request.url.includes(urlPart));
    const others = exchanges.filter((exchange, other) => other !== index && !exchange.alternate).map(exchange => exchange.request);
    const used = rows(exchanges[index].body).filter(value => others.some(request => `${decodeURIComponent(request.url)} ${JSON.stringify(request.body ?? '')}`.includes(value)));
    assert.ok(used.length > 0, `${String(id)}: another request uses a row`);
    const kept = rows(trimmed(exchanges[index].body, { requests: others }));
    assert.deepEqual(used.filter(value => !kept.includes(value)), [], `${String(id)}: a used row was trimmed`);
  }
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

test('the tool\'s own observation day is not a provider date it showed', () => {
  const release = { Time: '2020-01-14T10:00:00Z' };
  assert.deepEqual(uncarried(release, outputText({ observedAt: '2020-01-14' }), { dates: ['Time'] }), ['Time']);
  assert.deepEqual(uncarried(release, outputText({ publishedAt: '2020-01-14' }), { dates: ['Time'] }), []);
});
