// Registry behavior through the same public artifactSearch runner and pager.
import fs from 'node:fs';
import path from 'node:path';
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

export async function advancedRegistries(ctx) {
  const checks = [];
  const record = (label, surface, pass, detail) => checks.push([`advanced.registry.${label}.${surface}`, pass, detail]);
  for (const surface of ctx.surfaces) for (const kind of Object.keys(hosts)) {
    await resolutionChecks(ctx, surface, kind, record);
    await searchChecks(ctx, surface, kind, record);
    await refusalChecks(ctx, surface, kind, record);
  }
  await trustStoreChecks(ctx, record);
  record('routes.closed', 'all', ctx.registry.unhandled.length === 0 && ctx.registry.denied.length === 0, JSON.stringify({ unhandled: ctx.registry.unhandled, denied: ctx.registry.denied }));
  return checks;
}
