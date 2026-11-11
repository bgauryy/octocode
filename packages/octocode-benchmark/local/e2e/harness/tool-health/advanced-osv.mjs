// securityCheck through both surfaces: every OSV state, and the proof of what
// reached OSV, read from the proxy's record of its requests.
import { recordedJson } from './recorded-answers.mjs';

const OSV_NAMES = { npm: 'npm', pypi: 'PyPI', crates: 'crates.io', maven: 'Maven', nuget: 'NuGet', go: 'Go', packagist: 'Packagist', rubygems: 'RubyGems' };
const coordinate = kind => ({ maven: 'org.proof:proof', go: 'example.test/proof', packagist: 'proof/proof' })[kind] ?? 'proof';
const exact = kind => (kind === 'go' ? 'v1.0.0' : '1.0.0');
const COMMIT = '6879efc2c1596d11a6a6ad296f80063b558d5e0f';
const data = result => result.json?.results?.[0]?.data ?? {};
const row = (kind, fields = {}) => ({ operation: 'package', ecosystem: OSV_NAMES[kind], packageName: coordinate(kind), version: exact(kind), ...fields });
// The default view is the summary: advisory lists, answers and upgrade notes are `view: "details"`.
const details = query => ({ ...query, view: 'details' });
// Row fields only `details` states (security_check/summary.rs DETAIL_FIELDS).
const DETAIL_ONLY = ['upgradeNote', 'advisories', 'moduleOnly', 'repository', 'latestVersion', 'latestPublishedAt', 'currentPublishedAt', 'isLatest', 'upgradeKind'];
// Summary fields drawn from the finished advisory list rather than copied from the row.
const DERIVED = ['advisoryCount', 'highestSeverity', 'unfixedInMajorCount', 'link'];

/**
 * Why `lean` (the summary) is not `whole` (details) narrowed: no detail field, the counts and the most
 * severe advisory's link drawn from the details list, the verdict and every other field unchanged
 * (the echoed name and version and a verdict's answer may go), and one lead back to the details of `query`.
 */
function summaryProblem(lean, whole, query) {
  const counted = (whole.advisories ?? []).filter(item => !item.withdrawn);
  const leaked = DETAIL_ONLY.filter(field => Object.hasOwn(lean, field));
  if (leaked.length) return `detail fields in the summary: ${leaked.join(', ')}`;
  if (lean.advisoryCount !== counted.length) return `advisoryCount ${lean.advisoryCount}, details lists ${counted.length}`;
  if (counted.length && !counted.some(item => item.link === lean.link && item.severity === lean.highestSeverity)) return 'link and highestSeverity name no listed advisory';
  const echoes = { packageName: query.packageName, version: query.version };
  const changed = Object.keys(lean).filter(field => ![...DERIVED, 'sources', 'related'].includes(field) && json(lean[field]) !== json(whole[field]));
  if (changed.length) return `summary fields differ from details: ${changed.join(', ')}`;
  const lost = Object.keys(whole).filter(field => !Object.hasOwn(lean, field) && !DETAIL_ONLY.includes(field) && field !== 'related' && field !== 'sources'
    && !(field === 'answer' && ('affected' in lean || lean.status === 'empty')) && echoes[field] !== whole[field]);
  if (lost.length) return `summary dropped non-detail fields: ${lost.join(', ')}`;
  const { view: _view, ...target } = query;
  const lead = lean.related?.[0];
  return lead?.tool === 'securityCheck' && lead.input?.queries?.length === 1 && sorted(lead.input.queries[0]) === sorted(details(target))
    ? '' : `first lead is not the details call: ${json(lead)}`;
}
const sorted = value => json(Object.fromEntries(Object.entries(value ?? {}).toSorted(([a], [b]) => (a < b ? -1 : 1))));
// OSV's real record (test-utils/provider-answers), its identity and fix moved to the proof package.
// Its `modified` stamp stays behind: the proxy stamps each new content itself, since
// the engine keeps a record under (id, modified) for good.
const { modified: _stamp, ...REAL } = recordedJson('osv/advisory-ghsa', '/v1/vulns/');
// Records that share an alias (a CVE) are one advisory, so a record of its own id has no alias.
const vuln = (id, name = 'proof', ecosystem = 'npm', aliases = []) => ({ ...REAL, id, aliases, affected: [{ ...REAL.affected[0], package: { name, ecosystem }, ranges: [{ type: 'SEMVER', events: [{ introduced: '0' }, { fixed: '1.2.3' }] }] }] });
const json = value => JSON.stringify(value);
const ids = rows => (rows ?? []).map(item => item.id);

const check = (ctx, surface, input) => ctx.callOn(surface)('securityCheck', input);

/** One row with `answers` queued for OSV, and the bodies OSV received for it. */
async function asked(ctx, surface, query, answers = []) {
  const { registry } = ctx;
  registry.osvBodies.length = 0;
  registry.osvAnswers.push(...answers);
  const result = await check(ctx, surface, ctx.q(query));
  const bodies = [...registry.osvBodies];
  registry.osvAnswers.length = 0;
  return { row: data(result), status: result.json?.results?.[0]?.status, bodies };
}

/** The explicit summary of the `found` row: its details narrowed exactly, still affected. */
async function summaryOf(ctx, surface, { kind, found, answers }, record) {
  const query = row(kind, { view: 'summary' });
  const lean = await asked(ctx, surface, query, answers());
  const problem = summaryProblem(lean.row, found, query);
  record(`osv.${kind}.summary`, surface, !problem && lean.row.affected === true && lean.row.advisoryCount === 1, `${problem}; ${json({ summary: lean.row, details: found })}`);
}

async function everyRegistry(ctx, surface, record) {
  for (const kind of Object.keys(OSV_NAMES)) {
    const answers = () => [{ body: { vulns: [vuln('GHSA-1', coordinate(kind), OSV_NAMES[kind])] } }, { body: {} }];
    const { row: found, bodies } = await asked(ctx, surface, details(row(kind)), answers());
    const sent = json(bodies[0]) === json({ package: { name: coordinate(kind), ecosystem: OSV_NAMES[kind] }, version: exact(kind) });
    record(`osv.${kind}.found`, surface, sent && found.affected === true && found.advisories?.[0]?.id === 'GHSA-1' && found.upgradeTo?.replace(/^v/, '') === '1.2.3', json({ found, bodies }));
    await summaryOf(ctx, surface, { kind, found, answers }, record);
    const none = await asked(ctx, surface, details(row(kind)));
    record(`osv.${kind}.none`, surface, none.status === 'empty' && json(none.row.advisories) === '[]' && none.row.affected === false, json(none));
    const missing = await asked(ctx, surface, { ...row(kind), packageName: kind === 'maven' ? 'org.proof:absent' : kind === 'go' ? 'example.test/absent' : kind === 'packagist' ? 'proof/absent' : 'absent' });
    record(`osv.${kind}.notOnRegistry`, surface, missing.row.errorCode === 'notFound' && missing.bodies.length === 0 && !('advisories' in missing.row), json(missing.row));
  }
}

/**
 * OSV failed on a package row's first page: the row keeps what the registry and
 * GitHub answered, and still never reads as clean. OSV is the first source,
 * `failed`, with the code and (when it answered at all) the HTTP status in its
 * `why`, which holds none of OSV's own text; the row is partial (`sourceFailed`)
 * and retryable, with the same row as `next.retry`; it states no `affected`, no
 * `status: "empty"`, no upgrade and no advisories GitHub did not match; and the
 * answer says what happened (OSV did not answer, or answered with what could
 * not be read) and that the package was not checked against it.
 */
const unanswered = (failed, status, code, httpStatus, said = /OSV did not answer/) => {
  const osv = failed.sources?.[0] ?? {};
  return osv.source === 'osv' && osv.status === 'failed' && osv.errorCode === code
    && (httpStatus === undefined ? !/HTTP \d/.test(osv.why ?? '') : (osv.why ?? '').includes(`HTTP ${httpStatus}`))
    && typeof osv.why === 'string' && !osv.why.includes('Fixture') && !osv.why.includes('not json')
    && failed.isPartial === true && (failed.partialReasons ?? []).includes('sourceFailed') && failed.retryable === true && !!failed.next?.retry
    && !('affected' in failed) && status !== 'empty' && failed.status !== 'empty' && !('upgradeTo' in failed) && !('advisories' in failed)
    && said.test(failed.answer ?? '') && /not checked against it/.test(failed.answer ?? '');
};

async function failures(ctx, surface, record) {
  for (const [status, code] of [[429, 'rateLimited'], [503, 'server']]) {
    ctx.registry.failures.set('api.osv.dev', { status, body: { message: `Fixture ${status}` }, headers: { 'retry-after': '60' } });
    try {
      const { row: failed, status: rowStatus } = await asked(ctx, surface, row('pypi'));
      record(`osv.http${status}`, surface, unanswered(failed, rowStatus, code, status), json(failed));
    } finally { ctx.registry.failures.delete('api.osv.dev'); }
  }
  const malformed = await asked(ctx, surface, row('pypi'), [{ body: 'not json' }]);
  record('osv.malformed', surface, unanswered(malformed.row, malformed.status, 'providerError', undefined, /OSV's answer could not be read/), json(malformed.row));
  ctx.registry.failures.set('pypi.org', { status: 503, body: { error: 'Fixture outage' } });
  try {
    const down = await asked(ctx, surface, row('pypi'));
    record('osv.registryDown', surface, !!down.row.errorCode && down.bodies.length === 0 && /nothing was sent to OSV/.test(down.row.error), json(down.row));
  } finally { ctx.registry.failures.delete('pypi.org'); }
}

async function gate(ctx, surface, record) {
  const unpublished = await asked(ctx, surface, details(row('pypi', { version: '9.9.9' })), [{ body: { vulns: [vuln('GHSA-2')] } }]);
  record('osv.versionNotPublished', surface, json(unpublished.bodies) === json([{ package: { name: 'proof', ecosystem: 'PyPI' } }]) && unpublished.row.versionEvaluated === false && unpublished.row.advisories?.[0]?.id === 'GHSA-2' && !('affected' in unpublished.row), json(unpublished));
  const range = await asked(ctx, surface, row('crates', { version: '^1.0' }));
  record('osv.rangeResolved', surface, range.bodies[0]?.version === '1.2.3' && range.row.version === '1.2.3', json(range));
  const repository = await asked(ctx, surface, { operation: 'url', url: 'https://github.com/acme/private' });
  record('osv.repositoryUrlStaysWithGithub', surface, repository.row.cautions?.[0]?.rule === 'repositoryMissing' && repository.bodies.length === 0, json(repository));
  const unknownHost = await asked(ctx, surface, { operation: 'url', url: 'https://example.com/x' });
  record('osv.unknownHost', surface, /nothing was sent/.test(unknownHost.row.error ?? '') && unknownHost.bodies.length === 0, json(unknownHost));
}

/** `{}` is clean, a token-only page is not: the walk continues to the end. */
async function paging(ctx, surface, record) {
  const tokenOnly = await asked(ctx, surface, details(row('pypi')), [{ body: { next_page_token: 't1' } }, { body: { vulns: [vuln('GHSA-3')], next_page_token: 't2' } }, { body: { next_page_token: 't3' } }, { body: { vulns: [vuln('GHSA-4')] } }, { body: {} }]);
  record('osv.tokenOnlyPages', surface, json(ids(tokenOnly.row.advisories)) === json(['GHSA-3', 'GHSA-4']) && tokenOnly.bodies.slice(0, 4).map(body => body.page_token ?? '').join() === ',t1,t2,t3' && tokenOnly.status !== 'empty', json(tokenOnly));
  // A later page fails: what was read stays, and the cursor reaches the rest.
  ctx.registry.osvBodies.length = 0;
  // `~` is outside the cursor's base64url alphabet, so no cursor can hold
  // this token by chance: its absence from the resume call proves the cursor
  // keeps it server-side. Short, so the output gate never reads it as a secret.
  const token = 'p~1';
  ctx.registry.osvAnswers.push({ body: { vulns: [vuln('GHSA-5')], next_page_token: token } }, { status: 503, body: {}, headers: { 'retry-after': '60' } });
  const stopped = data(await check(ctx, surface, ctx.q(details(row('pypi')))));
  const resume = stopped.next?.nextPage;
  ctx.registry.osvAnswers.length = 0;
  ctx.registry.osvAnswers.push({ body: { vulns: [vuln('GHSA-6')] } }, { body: {} });
  const rest = resume ? data(await check(ctx, surface, resume.input)) : {};
  const lastBody = ctx.registry.osvBodies.at(-1);
  ctx.registry.osvAnswers.length = 0;
  record('osv.resumesThroughCursor', surface, stopped.isPartial === true && !('advisories' in stopped) && json(ids(rest.advisories)) === json(['GHSA-5', 'GHSA-6']) && lastBody?.page_token === token && json(Object.keys(resume?.input ?? {})) === json(['cursor']) && !json(resume ?? {}).includes(token), json({ stopped, rest, lastBody }));
  // A tiny response window rebuilds every record once.
  const many = Array.from({ length: 12 }, (_, i) => vuln(`GHSA-M${i}`));
  ctx.registry.osvAnswers.push({ body: { vulns: many } });
  const report = await ctx.explore('securityCheck', { queries: [details(row('pypi'))], responseChars: 900 }, surface);
  ctx.registry.osvAnswers.length = 0;
  const walked = report.responses.flatMap(page => page.result.json?.results?.flatMap(item => item.data?.advisories ?? []) ?? []);
  record('osv.tinyWindowLossless', surface, !report.failures.length && json(ids(walked)) === json(many.map(item => item.id)), `${report.responses.length} calls; ${json(report.failures)}`);
}

/** A GHSA record and its twin that alias the same CVE are one advisory row. */
async function twins(ctx, surface, record) {
  const shared = ['CVE-2024-0001'];
  const { row: merged } = await asked(ctx, surface, details(row('pypi')), [{ body: { vulns: [vuln('GHSA-T1', 'proof', 'PyPI', [...shared, 'PYSEC-T2']), vuln('PYSEC-T2', 'proof', 'PyPI', [...shared, 'GHSA-T1'])] } }, { body: {} }]);
  record('osv.aliasTwinsMerge', surface, merged.advisories?.length === 1 && merged.advisories[0].aliases?.includes('PYSEC-T2'), json(merged));
}

async function targets(ctx, surface, record) {
  ctx.registry.osvRecords.set('GHSA-w', { body: { ...vuln('GHSA-w'), withdrawn: '2024-01-01T00:00:00Z', details: 'Whole text.' } });
  const whole = await asked(ctx, surface, details({ operation: 'advisory', id: 'GHSA-w' }));
  record('osv.advisoryWhole', surface, whole.row.advisories?.[0]?.withdrawn === '2024-01-01T00:00:00Z' && whole.row.advisories?.[0]?.details === 'Whole text.' && whole.row.advisories?.[0]?.affected?.length === 1, json(whole));
  record('osv.realVectors', surface, REAL.severity.every(vector => json(whole.row).includes(vector.score)), json(whole.row));
  const absent = await asked(ctx, surface, { operation: 'advisory', id: 'GHSA-NONE' });
  record('osv.advisoryUnknown', surface, absent.row.errorCode === 'notFound' && !('advisories' in absent.row), json(absent));
}

async function commits(ctx, surface, record) {
  const commit = await asked(ctx, surface, { operation: 'commit', commit: COMMIT }, [{ body: { vulns: [vuln('GHSA-C')] } }]);
  record('osv.commitFound', surface, json(commit.bodies) === json([{ commit: COMMIT }]) && commit.row.affected === true, json(commit));
  const blank = await asked(ctx, surface, details({ operation: 'commit', commit: COMMIT, repo: 'https://example.test/private' }));
  record('osv.commitUnknownIsNotClean', surface, blank.status === 'empty' && !('affected' in blank.row) && /proves nothing/.test(blank.row.answer) && !json(blank.bodies).includes('example.test'), json(blank));
  // The summary of the same unknown commit still gives no verdict: it never reads as clean.
  const blankLean = await asked(ctx, surface, { operation: 'commit', commit: COMMIT, repo: 'https://example.test/private' });
  record('osv.commitUnknownSummaryIsNotClean', surface, blankLean.status === 'empty' && !('affected' in blankLean.row) && typeof blankLean.row.answer === 'string' && !json(blankLean.bodies).includes('example.test'), json(blankLean));
}

async function advisoryChecks(ctx, surface, record) {
  await everyRegistry(ctx, surface, record);
  await failures(ctx, surface, record);
  await gate(ctx, surface, record);
  await paging(ctx, surface, record);
  await twins(ctx, surface, record);
  await targets(ctx, surface, record);
  await commits(ctx, surface, record);
}

/** The switch gates MCP discovery only: a CLI call is the user's own request and still reaches OSV. */
async function advisoryOffChecks(ctx, record) {
  ctx.registry.osvBodies.length = 0;
  ctx.registry.osvAnswers.push({ body: { vulns: [vuln('GHSA-1')] } }, { body: {} });
  const result = await ctx.cliCall('securityCheck', ctx.q(details(row('npm'))), { extraEnv: { OCTOCODE_SECURITY_CHECKS: 'false' } });
  const bodies = [...ctx.registry.osvBodies];
  ctx.registry.osvAnswers.length = 0;
  record('osv.cliOffSwitchStillAnswers', 'cli', data(result).affected === true && ids(data(result).advisories).join() === 'GHSA-1' && bodies.length > 0, json({ json: result.json, bodies }));
}

/**
 * Every OSV state on both surfaces, then the switch. It shares the registry
 * proxy's failure map and queues with the registry suite, so that suite runs it
 * in sequence rather than beside it.
 */
export async function osvChecks(ctx, record) {
  for (const surface of ctx.surfaces) await advisoryChecks(ctx, surface, record);
  await advisoryOffChecks(ctx, record);
}

/** This tool's own suite: refusals that need no proxy state. */
export async function advancedSecurityCheck(ctx) {
  const checks = [];
  const record = (label, surface, pass, detail) => checks.push([`advanced.${label}.${surface}`, pass, detail]);
  for (const surface of ctx.surfaces) {
    // A github.com repository URL is read by GitHub as a repository, never as a package.
    const repository = data(await check(ctx, surface, ctx.q({ operation: 'url', url: 'https://github.com/acme/private' })));
    record('routes.repositoryUrl', surface, repository.cautions?.[0]?.rule === 'repositoryMissing' && !repository.errorCode, json(repository));
    const short = await check(ctx, surface, ctx.q({ operation: 'commit', commit: '6879efc' }));
    record('refuses.shortCommit', surface, json(short.json).includes('invalidInput'), json(short.json));
    const token = await check(ctx, surface, ctx.q({ operation: 'advisory', id: 'GHSA-happy', resume: 'osv-token' }));
    record('refuses.callerToken', surface, json(token.json).includes('invalidInput'), json(token.json));
  }
  return checks;
}
