// Provider wire fixtures behind the hermetic TLS proxy. Coordinates and search
// windows share one model; these routes do not know which tool invoked them.
export const REGISTRY_HOSTS = ['pypi.org', 'crates.io', 'central.sonatype.com', 'repo.maven.apache.org', 'repo1.maven.org', 'api.nuget.org', 'pkg.go.dev', 'proxy.golang.org', 'repo.packagist.org', 'packagist.org', 'rubygems.org'];
const VERSION = '1.2.3', DATE = '2025-02-03T12:00:00Z';
const versions = ['1.0.0', VERSION];
const names = Array.from({ length: 45 }, (_, i) => `proof-${String(i).padStart(2, '0')}`);
const ok = body => ({ body });
const missing = () => ({ status: 404, body: { error: 'Package not found' } });
const slice = (items, start, size) => items.slice(Number(start), Number(start) + Number(size));
const keywordNames = (url, field = 'q') => names.filter(name => (url.searchParams.get(field) ?? '').split(/\s+/).every(term => name.includes(term)));
const page = (url, list, size = url.searchParams.get('per_page') ?? 20) => slice(list, ((url.searchParams.get('page') ?? 1) - 1) * size, size);
const crate = name => ({ name, max_version: VERSION, default_version: VERSION, description: 'Proof crate', license: 'MIT' });
const crateVersion = num => ({ num, created_at: DATE, yanked: false, license: 'MIT', rust_version: '1.80' });
const gem = name => ({ name, version: VERSION, info: 'Proof gem', licenses: ['MIT'], version_created_at: DATE, dependencies: { runtime: [{ name: 'proof-dep', requirements: '>= 2.0' }], development: [{ name: 'dev-only', requirements: '>= 1' }] } });
const composer = name => ({ name: `proof/${name}`, version: VERSION, description: 'Proof package', license: ['MIT'], time: DATE, require: { php: '>=8.1', 'proof/dep': '^2.0' } });
const nuget = name => ({ id: name, version: VERSION, description: 'Proof package', published: DATE, listed: true, licenseExpression: 'MIT', repository: 'https://gitlab.com/example/proof', dependencyGroups: [{ dependencies: [{ id: 'Proof.Dep', range: '[2.0.0, )' }] }] });

function pypi(url) {
  const m = /^\/pypi\/([^/]+)\/(?:([^/]+)\/)?json$/.exec(url.pathname);
  if (!m || m[1] !== 'proof' || (m[2] && !versions.includes(m[2]))) return missing();
  return ok({ info: { name: 'proof', version: m[2] ?? VERSION, summary: 'Proof Python package', license_expression: 'MIT', requires_python: '>=3.10', requires_dist: ['proof-dep>=2', 'proof-extra; extra == "fast"'] }, urls: [{ upload_time_iso_8601: DATE }], releases: Object.fromEntries(versions.map(v => [v, [{ yanked: false }]])) });
}
function crates(url) {
  if (url.pathname === '/api/v1/crates') {
    const hits = keywordNames(url);
    return ok({ crates: page(url, hits).map(crate), meta: { total: hits.length } });
  }
  const m = /^\/api\/v1\/crates\/([^/]+)(?:\/([^/]+)(?:\/(dependencies))?)?$/.exec(url.pathname);
  if (!m || m[1] !== 'proof' || (m[2] && !versions.includes(m[2]))) return missing();
  if (m[3]) return ok({ dependencies: [{ crate_id: 'proof-dep', req: '^2', kind: 'normal', optional: false }, { crate_id: 'proof-extra', req: '^1', kind: 'normal', optional: true }, { crate_id: 'dev-only', req: '*', kind: 'dev', optional: false }] });
  return ok(m[2] ? { version: crateVersion(m[2]) } : { crate: crate('proof'), versions: versions.map(crateVersion) });
}
function maven(url) {
  if (url.hostname === 'central.sonatype.com') {
    const q = url.searchParams.get('q') ?? '';
    const found = q.includes('g:') ? [{ g: 'org.proof', a: 'proof', v: VERSION, timestamp: Date.parse(DATE) }] : keywordNames(url).map(name => ({ g: 'org.proof', a: name, latestVersion: VERSION }));
    return ok({ response: { numFound: found.length, docs: slice(found, url.searchParams.get('start') ?? 0, url.searchParams.get('rows') ?? 20) } });
  }
  if (url.pathname === '/maven2/org/proof/proof/maven-metadata.xml') return ok(`<metadata><groupId>org.proof</groupId><artifactId>proof</artifactId><versioning><release>${VERSION}</release><versions>${versions.map(v => `<version>${v}</version>`).join('')}</versions></versioning></metadata>`);
  if (/^\/maven2\/org\/proof\/proof[^/]*\/(1\.0\.0|1\.2\.3)\/[^/]+\.pom$/.test(url.pathname)) return ok('<project><dependencies><dependency><groupId>org.proof</groupId><artifactId>dep</artifactId><version>2.0.0</version></dependency><dependency><groupId>org.proof</groupId><artifactId>dev-only</artifactId><scope>test</scope></dependency></dependencies></project>');
  return missing();
}
function nugetRoute(url) {
  if (url.pathname === '/v3/index.json') return ok({ resources: [{ '@type': 'SearchQueryService/3.5.0', '@id': 'https://api.nuget.org/search' }, { '@type': 'RegistrationsBaseUrl/3.6.0', '@id': 'https://api.nuget.org/registration' }] });
  if (url.pathname === '/search') {
    const hits = keywordNames(url);
    return ok({ totalHits: hits.length, data: slice(hits, url.searchParams.get('skip') ?? 0, url.searchParams.get('take') ?? 20).map(nuget) });
  }
  if (url.pathname === '/registration/proof/index.json') return ok({ items: [{ lower: '1.0.0', upper: VERSION, items: versions.map(version => ({ catalogEntry: { ...nuget('proof'), version } })) }] });
  return missing();
}
function go(url) {
  if (url.pathname === '/v1/search') {
    const hits = keywordNames(url), start = Number(url.searchParams.get('token') ?? 0), size = Number(url.searchParams.get('limit') ?? 20);
    return ok({ total: hits.length, items: slice(hits, start, size).map(name => ({ packagePath: `example.test/${name}`, modulePath: `example.test/${name}`, version: `v${VERSION}`, synopsis: 'Proof Go module' })), ...(start + size < hits.length && { nextPageToken: String(start + size) }) });
  }
  if (url.pathname === '/v1/versions/example.test/proof') return ok({ items: versions.map(version => ({ version: `v${version}` })) });
  if (url.pathname === '/v1/module/example.test/proof' && (!url.searchParams.has('version') || versions.map(v => `v${v}`).includes(url.searchParams.get('version')))) return ok({ path: 'example.test/proof', version: url.searchParams.get('version') ?? `v${VERSION}`, synopsis: 'Proof Go module', commitTime: DATE });
  if (/^\/example.test\/proof\/@v\/v(1\.0\.0|1\.2\.3)\.info$/.test(url.pathname)) return ok({ Origin: { Hash: 'a'.repeat(40) } });
  if (/^\/example.test\/proof\/@v\/v(1\.0\.0|1\.2\.3)\.mod$/.test(url.pathname)) return ok('module example.test/proof\nrequire example.test/dep v2.0.0\nrequire example.test/indirect v1.0.0 // indirect\n');
  return missing();
}
function packagist(url) {
  if (url.pathname === '/p2/proof/proof.json') return ok({ packages: { 'proof/proof': [composer('proof')] } });
  if (url.pathname === '/search.json') {
    const hits = keywordNames(url), selected = page(url, hits), current = Number(url.searchParams.get('page') ?? 1), size = Number(url.searchParams.get('per_page') ?? 20);
    return ok({ results: selected.map(composer), total: hits.length, ...(current * size < hits.length && { next: `https://packagist.org/search.json?q=proof&page=${current + 1}` }) });
  }
  return missing();
}
function rubygems(url) {
  if (url.pathname === '/api/v1/gems/proof.json') return ok(gem('proof'));
  if (url.pathname === '/api/v1/search.json') return ok(page(url, keywordNames(url, 'query'), 30).map(gem));
  return missing();
}
export function registryResponse(url) {
  const host = url.hostname;
  if (host === 'pypi.org') return pypi(url);
  if (host === 'crates.io') return crates(url);
  if (host.includes('maven') || host === 'central.sonatype.com') return maven(url);
  if (host === 'api.nuget.org') return nugetRoute(url);
  if (host === 'pkg.go.dev' || host === 'proxy.golang.org') return go(url);
  if (host.includes('packagist')) return packagist(url);
  if (host === 'rubygems.org') return rubygems(url);
  return null;
}
