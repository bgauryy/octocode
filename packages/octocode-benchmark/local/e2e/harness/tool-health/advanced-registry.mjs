// Registry behavior through the same public artifactSearch runner and pager.
import fs from 'node:fs';
import path from 'node:path';
import { sourceLines, structureFiles } from '../responses.mjs';
import { leadsIn } from './calls.mjs';
import { tokenFor, withoutReadinessPings } from './surfaces.mjs';
const equal = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const data = result => result.json?.results?.[0]?.data ?? {};
const coordinate = (kind, name) => ({ maven: `org.proof:${name}`, go: `example.test/${name}`, packagist: `proof/${name}` })[kind] ?? name;
const hosts = { pypi: ['pypi.org'], crates: ['crates.io'], maven: ['repo.maven.apache.org', 'repo1.maven.org', 'central.sonatype.com'], nuget: ['api.nuget.org'], go: ['pkg.go.dev', 'proxy.golang.org'], packagist: ['repo.packagist.org', 'packagist.org'], rubygems: ['rubygems.org'] };
const dependencies = { pypi: ['proof-dep>=2'], crates: ['proof-dep ^2'], maven: ['org.proof:dep:2.0.0'], nuget: ['Proof.Dep [2.0.0, )'], go: ['example.test/dep v2.0.0'], packagist: ['php >=8.1', 'proof/dep ^2.0'], rubygems: ['proof-dep >= 2.0'] };

const version = (kind, value) => kind === 'go' ? `v${value}` : value;
const artifact = result => data(result).artifacts?.[0];

async function resolutionChecks(ctx, surface, kind, record) {
  const packageName = coordinate(kind, 'proof');
  const row = { operation: 'resolve', ecosystem: kind, packageName };
  const result = await ctx.call(ctx.q(row), surface);
  const item = artifact(result);
  record(`${kind}.release`, surface, item?.packageName === packageName && item.version === version(kind, '1.2.3') && equal(item.dependencies, dependencies[kind]), JSON.stringify(result.json));
  if (['pypi', 'crates'].includes(kind)) {
    const resolved = artifact(await ctx.call(ctx.q({ ...row, version: kind === 'pypi' ? '>=1,<2' : '^1.0' }), surface));
    record(`${kind}.range`, surface, resolved?.version === '1.2.3' && resolved.optionalDependencies?.length === 1, JSON.stringify(resolved));
  }
  if (!['packagist', 'rubygems'].includes(kind)) {
    const exact = artifact(await ctx.call(ctx.q({ ...row, version: version(kind, '1.0.0') }), surface));
    record(`${kind}.exact`, surface, exact?.version === version(kind, '1.0.0'), JSON.stringify(exact));
  }
}

async function searchChecks(ctx, surface, kind, record) {
  if (kind === 'pypi') return;
  const report = await ctx.explore('artifactSearch', { queries: [{ operation: 'search', ecosystem: kind, keywords: ['proof'] }], responseChars: 3000 }, surface);
  const names = report.responses.flatMap(row => row.result.json?.results?.flatMap(item => (item.data?.artifacts ?? []).map(entry => entry.packageName)) ?? []);
  const expected = Array.from({ length: 45 }, (_, i) => coordinate(kind, `proof-${String(i).padStart(2, '0')}`));
  record(`${kind}.nativeAndResponsePaging`, surface, !report.failures.length && equal(names, expected), `${names.length} rows in ${report.responses.length} calls; ${JSON.stringify(report.failures)}`);
}

async function refusalChecks(ctx, surface, kind, record) {
  for (const [status, code] of [[401, 'authentication'], [429, 'rateLimited']]) {
    for (const host of hosts[kind]) ctx.registry.failures.set(host, { status, body: { error: `Fixture ${kind} refusal` }, headers: { 'retry-after': '60' } });
    try {
      const failure = data(await ctx.call(ctx.q({ operation: 'resolve', ecosystem: kind, packageName: coordinate(kind, `proof-failure-${status}`) }), surface));
      record(`${kind}.http${status}`, surface, failure.errorCode === code && failure.httpStatus === status && failure.error.includes(kind), JSON.stringify(failure));
    } finally { for (const host of hosts[kind]) ctx.registry.failures.delete(host); }
  }
}

async function trustStoreChecks(ctx, record) {
  const certDir = path.join(ctx.tmp, 'explicit-cert-directory');
  fs.mkdirSync(certDir, { recursive: true });
  fs.copyFileSync(ctx.registry.caFile, path.join(certDir, 'ca.pem'));
  const query = ctx.q({ operation: 'resolve', ecosystem: 'pypi', packageName: 'proof' });
  const beforeDirectory = ctx.registry.requests.length;
  const directory = await ctx.cliCall('artifactSearch', query, { env: { SSL_CERT_FILE: '', SSL_CERT_DIR: certDir, OCTOCODE_HOME: path.join(ctx.tmp, 'directory-trust-home') } });
  record('tls.directoryRoots', 'cli', artifact(directory)?.version === '1.2.3' && ctx.registry.requests.length > beforeDirectory, JSON.stringify(directory.json));
  const malformed = path.join(ctx.tmp, 'malformed-ca.pem');
  fs.writeFileSync(malformed, 'not a certificate');
  for (const [label, file] of [['malformedRoots', malformed], ['missingRoots', path.join(ctx.tmp, 'absent-ca.pem')]]) {
    const before = ctx.registry.requests.length;
    const failed = await ctx.cliCall('artifactSearch', query, { env: { SSL_CERT_FILE: file, SSL_CERT_DIR: '', OCTOCODE_HOME: path.join(ctx.tmp, label) } });
    record(`tls.${label}`, 'cli', data(failed).errorCode === 'providerError' && before === ctx.registry.requests.length, JSON.stringify(failed.json));
  }
}

const packageRow = (ctx, packageName, fields = {}) => ({ operation: 'resolve', ecosystem: 'npm', packageName, registryUrl: ctx.mock.url, ...fields });
const resultRows = report => report.responses.flatMap(response => response.result.json?.results ?? []);
const artifactRows = report => resultRows(report).flatMap(row => row.data?.artifacts ?? []);
const detailOf = report => `${report.responses.length} calls; ${JSON.stringify(report.failures)}`;
const providerCalls = (ctx, surface) => withoutReadinessPings(ctx.mock.requestsFrom.filter(entry => entry.token === null
  || entry.token === tokenFor('artifactSearch', surface))).length + ctx.registry.requests.length;

function npmFixtures(ctx) {
  const large = {
    name: '@flow/large', version: '2.0.0', versions: ['1.9.0', '1.10.0', '2.0.0', '1.2.0', '1.11.0-beta.1'],
    description: 'Version and nested dependency evidence', keywords: ['flowlarge'],
    dependencies: Object.fromEntries(Array.from({ length: 90 }, (_, i) => [`@nested/dependency-${String(i).padStart(3, '0')}`, `^${i}.0.0`])),
  };
  const search = Array.from({ length: 45 }, (_, i) => ({
    name: `flow-page-${String(i).padStart(2, '0')}`, version: `1.0.${i}`, versions: [`1.0.${i}`],
    description: 'Exact registry pagination evidence', keywords: ['flowpaging'], dependencies: {},
  }));
  ctx.mock.model.packages.push(large, ...search);
  return { large, search };
}

async function npmVersions(ctx, surface, record) {
  const choices = [[undefined, '2.0.0'], ['latest', '2.0.0'], ['1.9.0', '1.9.0'], ['^1', '1.10.0'],
    ['~1.9', '1.9.0'], ['>=1.9 <2', '1.10.0'], ['1.x', '1.10.0'], ['1.2 - 1.10', '1.10.0'],
    ['1.9.0 || 1.10.0', '1.10.0'], ['1.11.0-beta.1', '1.11.0-beta.1']];
  for (const [requested, expected] of choices) {
    const item = artifact(await ctx.call(ctx.q(packageRow(ctx, '@flow/large', requested ? { version: requested } : {})), surface));
    record(`npm.version.${requested ?? 'default'}`, surface, item?.packageName === '@flow/large' && item.version === expected, JSON.stringify(item));
  }
}

async function npmSearchPages(ctx, surface, expected, record) {
  const query = { operation: 'search', ecosystem: 'npm', keywords: ['flowpaging'], registryUrl: ctx.mock.url };
  for (const responseChars of [1, 700, 50_000]) {
    const report = await ctx.explore('artifactSearch', { queries: [query], responseChars }, surface);
    const found = artifactRows(report).map(item => [item.packageName, item.version]);
    const exact = equal(found, expected.map(item => [item.name, item.version]));
    record(`npm.search.budget${responseChars}`, surface, !report.failures.length && exact
      && resultRows(report).every(row => row.index === 0), `${found.length} artifacts; ${detailOf(report)}`);
  }
}

async function npmDependencies(ctx, surface, fixture, record) {
  const report = await ctx.explore('artifactSearch', { queries: [packageRow(ctx, fixture.name, { version: '^1' })], responseChars: 700 }, surface);
  const rows = resultRows(report);
  const artifacts = artifactRows(report);
  const expected = Object.entries(fixture.dependencies).map(([name, range]) => `${name}@${range}`);
  const actual = artifacts.flatMap(item => item.dependencies ?? []);
  const parts = rows.map(row => row.rowPart);
  const identity = artifacts.every(item => item.packageName === fixture.name && item.owner === 'octo' && item.repo === 'demo'
    && (item.version === undefined || item.version === '1.10.0'));
  const geometry = parts.length > 1 && parts.every((part, i) => part?.part === i + 1 && part.of === parts.length
    && (i === 0 || equal(part.continues, { array: '/data/artifacts', index: 0 })));
  record('npm.dependencies.complete', surface, !report.failures.length && equal(actual, expected) && identity && geometry
    && artifacts[0]?.version === '1.10.0' && rows.every(row => row.index === 0), `${actual.length} dependencies; ${detailOf(report)}`);
  const replayInput = report.responses[1]?.input;
  if (!replayInput) { record('npm.dependencies.replay', surface, false, 'No display continuation issued'); return; }
  const before = providerCalls(ctx, surface);
  const replay = await ctx.call(replayInput, surface);
  record('npm.dependencies.replay', surface, equal(replay.json?.results, report.responses[1].result.json?.results)
    && providerCalls(ctx, surface) === before, 'Replaying the saved page preserves its exact indexed fragment without provider work');
  await npmSourceHandoff(ctx, surface, report, record);
}

async function npmSourceHandoff(ctx, surface, report, record) {
  const calls = leadsIn(report.responses[0].result.json, ctx.schemaOf('artifactSearch'));
  const lead = calls.find(call => call.kind === 'related' && call.tool === 'ghStructure');
  const query = lead?.input.queries?.[0];
  const pin = ctx.mock.model.head.oid;
  const packageArtifact = artifactRows(report)[0];
  record('npm.source.coordinate', surface, query?.owner === 'octo' && query.repo === 'demo' && query.ref === pin
    && packageArtifact?.ref === pin && !Object.hasOwn(packageArtifact, 'sourceRef'), JSON.stringify({ packageArtifact, lead }));
  if (!lead) return;
  const tree = await ctx.explore(lead.tool, lead.input, surface);
  const files = resultRows(tree).flatMap(row => structureFiles(row.data?.files)).map(file => file.path).toSorted();
  const expectedFiles = [...new Set(Object.keys(ctx.mock.model.files).map(name => name.split('/')[0]))].toSorted();
  record('npm.source.files', surface, !tree.failures.length && equal(files, expectedFiles), `${JSON.stringify(files)}; ${detailOf(tree)}`);
  await sourceRead(ctx, surface, { tree, query }, record);
}

async function sourceRead(ctx, surface, { tree, query }, record) {
  const read = tree.responses.flatMap(response => leadsIn(response.result.json, ctx.schemaOf('ghStructure')))
    .find(call => call.kind === 'related' && call.tool === 'ghGetFileContent');
  const readQuery = read?.input.queries?.[0];
  record('npm.source.readCoordinate', surface, readQuery?.owner === query.owner && readQuery.repo === query.repo && readQuery.ref === query.ref, JSON.stringify(read));
  if (!read) return;
  const content = await ctx.explore(read.tool, read.input, surface);
  const lines = resultRows(content).flatMap(row => sourceLines(row.data));
  const expected = ctx.mock.model.files[readQuery.path].replace(/\n$/, '').split('\n').map((text, index) => [index + 1, text]);
  record('npm.source.exactRead', surface, !content.failures.length && equal(lines, expected), detailOf(content));
}

async function npmBatch(ctx, surface, expected, record) {
  const queries = [packageRow(ctx, 'tiny-health', { matchString: ['health'] }), packageRow(ctx, 'tiny-health'),
    { operation: 'search', ecosystem: 'npm', keywords: ['flowpaging'], registryUrl: ctx.mock.url },
    packageRow(ctx, '@flow/large', { version: '9.9.9' }), packageRow(ctx, 'tiny-greet', { version: '2.0.0' })];
  const report = await ctx.explore('artifactSearch', { queries, responseChars: 50_000 }, surface);
  const rows = resultRows(report);
  const at = index => rows.filter(row => row.index === index);
  const searched = at(2).flatMap(row => row.data?.artifacts ?? []).map(item => item.packageName);
  const exact = equal(searched, expected.map(item => item.name)) && equal([...new Set(rows.map(row => row.index))].toSorted((a, b) => a - b), [0, 1, 2, 3, 4]);
  record('npm.batch.indices', surface, !report.failures.length && exact && batchRowsValid(at),
    `${JSON.stringify(rows.map(row => ({ index: row.index, errorCode: row.data?.errorCode, count: row.data?.artifacts?.length })))}; ${detailOf(report)}`);
}

function batchRowsValid(at) {
  return [0, 1, 3, 4].every(index => at(index).length === 1)
    && at(0)[0].data?.errorCode === 'invalidInput' && at(3)[0].data?.errorCode === 'versionNotFound'
    && at(1)[0].data?.artifacts?.[0]?.packageName === 'tiny-health' && at(4)[0].data?.artifacts?.[0]?.version === '2.0.0';
}

function unsupportedRows(ctx) {
  const resolve = packageRow(ctx, 'tiny-health');
  const search = { operation: 'search', ecosystem: 'npm', keywords: ['health'], registryUrl: ctx.mock.url };
  return Object.entries({ matchString: { ...resolve, matchString: ['health'] }, minify: { ...resolve, minify: true },
    dependenciesOperation: { ...resolve, operation: 'dependencies' }, filesOperation: { ...resolve, operation: 'files' },
    embeddedVersion: { ...resolve, packageName: 'tiny-health@1.2.3' }, rowCursor: { ...search, cursor: 'opaque' },
    providerCursor: { ...search, registryCursor: 'opaque' }, page: { ...search, page: 2 },
    pypiSearch: { operation: 'search', ecosystem: 'pypi', keywords: ['proof'] },
    foreignRegistry: { operation: 'resolve', ecosystem: 'crates', packageName: 'proof', registryUrl: ctx.mock.url },
    unversionedRegistry: { operation: 'resolve', ecosystem: 'rubygems', packageName: 'proof', version: '1.0.0' },
    exactOnlyRange: { operation: 'resolve', ecosystem: 'maven', packageName: 'org.proof:proof', version: '^1' } });
}

async function npmAdmission(ctx, surface, record) {
  for (const [label, query] of unsupportedRows(ctx)) {
    const before = providerCalls(ctx, surface);
    const result = await ctx.call(ctx.q(query), surface);
    const code = result.json?.errorCode ?? data(result).errorCode;
    record(`npm.reject.${label}`, surface, ctx.validateInput('artifactSearch', ctx.q(query)) !== null
      && code === 'invalidInput' && providerCalls(ctx, surface) === before, JSON.stringify(result.json));
  }
}

async function npmChecks(ctx, record) {
  const fixtures = npmFixtures(ctx);
  try {
    for (const surface of ctx.surfaces) {
      await npmVersions(ctx, surface, record);
      await npmSearchPages(ctx, surface, fixtures.search, record);
      await npmDependencies(ctx, surface, fixtures.large, record);
      await npmBatch(ctx, surface, fixtures.search, record);
      await npmAdmission(ctx, surface, record);
    }
  } finally {
    const inserted = new Set([fixtures.large, ...fixtures.search]);
    ctx.mock.model.packages = ctx.mock.model.packages.filter(item => !inserted.has(item));
  }
}

export async function advancedRegistries(ctx) {
  const checks = [];
  const record = (label, surface, pass, detail) => checks.push([`advanced.registry.${label}.${surface}`, pass, detail]);
  for (const surface of ctx.surfaces) for (const kind of Object.keys(hosts)) {
    await resolutionChecks(ctx, surface, kind, record);
    await searchChecks(ctx, surface, kind, record);
    await refusalChecks(ctx, surface, kind, record);
  }
  await trustStoreChecks(ctx, record);
  await npmChecks(ctx, record);
  record('routes.closed', 'all', ctx.registry.unhandled.length === 0 && ctx.registry.denied.length === 0, JSON.stringify({ unhandled: ctx.registry.unhandled, denied: ctx.registry.denied }));
  return checks;
}
