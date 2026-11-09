// The securityCheck RFC's live-evidence probes E1-E21 (.octocode/security-check/rfc-robust.md,
// section 3) as the shipped contract answers them. Each probe asks the provider in the same run
// and compares the tool's row with it through security-live-oracle.mjs.
import { contract } from './contract.mjs';
import { cli, GITHUB, OSV, osvIds, provider, registry, startProbe, unreadReason } from './security-live-calls.mjs';
import {
  advisoriesNaming,
  contractPolicy,
  cautionShapeProblem,
  cvssShapeProblem,
  expectedRepositoryRules,
  githubAdvisoryProblem,
  githubSource,
  osvCvssProblem,
  osvVerdictProblem,
  repositoryFactsProblem,
  ruleSetProblem,
  subsetProblem,
} from './security-live-oracle.mjs';

const TRUST_RULES = new Set(['lowTraction', 'unmaintained', 'archived', 'repositoryMoved', 'repositoryMissing']);
const HARFBUZZ = '6879efc2c1596d11a6a6ad296f80063b558d5e0f';
export const osvQuery = (ecosystem, name, version, release) => ({ package: { name, ecosystem: release ? `${ecosystem}:${release}` : ecosystem }, ...(version && { version }) });
const today = row => row?.repository?.observedAt ?? new Date().toISOString().slice(0, 10);
const policy = contractPolicy(contract);
const sorted = list => JSON.stringify([...(list ?? [])].map(String).sort());

/**
 * Provider truth reads GitHub with the caller's token when one exists, so an anonymous pass spends
 * GitHub's 60 anonymous requests on the tool alone; `token` overrides it (E16, E17 probe anonymity).
 */
const gh = (mode, route, options = {}) => provider(`${GITHUB}${route}`, { token: mode.truthToken, ...options });
export const contractOf = (probe, name, result) => probe.expect(`${name}.contract`, result.problem);
const shared = {};

/** The GitHub source is read, or refused with its disclosure; true when facts can be checked. */
export function githubRead(probe, name, row) {
  const source = githubSource(row);
  probe.expect(`${name}.githubDisclosed`, source.problem, source.state);
  if (source.state === 'disclosed') probe.skip(`${name}.githubFacts`, `GitHub refused the tool: ${source.entry.errorCode}, ${source.entry.why}`);
  return source.state === 'read';
}

function trustCheck(probe, name, { row, truth, requested }) {
  const expected = expectedRepositoryRules(truth, requested, policy, today(row));
  probe.expect(name, ruleSetProblem(row.cautions, expected, TRUST_RULES), [...expected].join(','));
}

export async function e1(mode) {
  const probe = startProbe(mode, 'E1', 'artifactSearch resolve npm lodash: latest, owner/repo, ref and its registryRef warning');
  const truth = await registry('lodash');
  const latest = truth['dist-tags'].latest;
  const result = await cli(mode, 'artifactSearch', { queries: [{ ecosystem: 'npm', operation: 'resolve', packageName: 'lodash' }] });
  contractOf(probe, 'resolve', result);
  const artifact = result.data?.artifacts?.[0] ?? {};
  probe.check('latest', artifact.version === latest, `${artifact.version} vs registry ${latest}`);
  probe.check('repo', `${artifact.owner}/${artifact.repo}` === 'lodash/lodash', `${artifact.owner}/${artifact.repo}`);
  probe.check('ref', artifact.ref === truth.versions[latest].gitHead, `${artifact.ref} vs gitHead ${truth.versions[latest].gitHead}`);
  const warned = (result.data?.warnings ?? []).some(warning => warning.includes(artifact.ref));
  probe.check('verification', artifact.verification === 'registryRef' ? warned : ['tag', 'provenance', 'defaultBranch'].includes(artifact.verification), `${artifact.verification}; ${JSON.stringify(result.data?.warnings)}`);
}

export async function e2(mode) {
  const probe = startProbe(mode, 'E2', 'artifactSearch resolve deprecated packages: left-pad, request, node-uuid, leftpad');
  const names = ['left-pad', 'request', 'node-uuid', 'leftpad'];
  const result = await cli(mode, 'artifactSearch', { queries: names.map(packageName => ({ ecosystem: 'npm', operation: 'resolve', packageName })) });
  contractOf(probe, 'resolve', result);
  for (const [i, name] of names.entries()) {
    const truth = await registry(name);
    const want = truth.versions[truth['dist-tags'].latest].deprecated;
    const artifact = result.rows.find(row => row.index === i)?.data?.artifacts?.[0];
    probe.check(`${name}.deprecated`, !!want && artifact?.deprecated === want, `${JSON.stringify(artifact?.deprecated)} vs registry ${JSON.stringify(want)}`);
  }
  probe.check('leftpad.owner', result.rows.find(row => row.index === 3)?.data?.artifacts?.[0]?.owner === 'tmcw');
}

export async function e3(mode) {
  const probe = startProbe(mode, 'E3', 'ghSearchRepo get left-pad/left-pad: pushedAt, not updatedAt, is the maintenance date');
  const truth = await gh(mode, '/repos/left-pad/left-pad');
  const result = await cli(mode, 'ghSearchRepo', { queries: [{ operation: 'get', owner: 'left-pad', repo: 'left-pad' }] });
  contractOf(probe, 'get', result);
  if (truth.status !== 200) return probe.skip('facts', unreadReason(truth, 'REST truth unread'));
  const repo = result.data?.repositories?.[0] ?? {};
  return probe.check('facts', repo.archived === truth.json.archived && repo.pushedAt === truth.json.pushed_at.slice(0, 10), `${repo.archived} ${repo.pushedAt} vs ${truth.json.archived} ${truth.json.pushed_at}`);
}

/** E4-E7 and the B169 trust rows: GitHub's REST record against the `repository` row. */
export async function repository(mode, id, requested) {
  const probe = startProbe(mode, id, `securityCheck repository ${requested} against REST repos/${requested}`);
  const truth = await gh(mode, `/repos/${requested}`);
  const result = await cli(mode, 'securityCheck', { queries: [{ operation: 'repository', repo: requested }] });
  contractOf(probe, 'repository', result);
  const row = result.data;
  probe.expect('cautionShape', cautionShapeProblem(row?.cautions));
  if (!githubRead(probe, 'repository', row)) return undefined;
  if (truth.status === 404) return probe.check('missing', (row.cautions ?? []).some(c => c.rule === 'repositoryMissing'), JSON.stringify(row.cautions));
  if (truth.status !== 200) return probe.skip('facts', unreadReason(truth, 'REST truth unread'));
  probe.expect('facts', repositoryFactsProblem(row.repository, truth.json));
  return trustCheck(probe, 'cautions', { row, truth: truth.json, requested });
}

export async function e8(mode) {
  const probe = startProbe(mode, 'E8', 'securityCheck package npm left-pad@1.3.0: none known, yet deprecated and trust cautions');
  const [ids] = await osvIds([osvQuery('npm', 'left-pad', '1.3.0')]);
  const pkg = await registry('left-pad');
  const result = await cli(mode, 'securityCheck', { queries: [{ operation: 'package', ecosystem: 'npm', packageName: 'left-pad', version: '1.3.0' }] });
  contractOf(probe, 'package', result);
  const row = result.data ?? {};
  probe.expect('osvVerdict', osvVerdictProblem(row, result.status, ids), `OSV ${ids.length}`);
  probe.check('deprecated', (row.cautions ?? []).some(c => c.rule === 'deprecated') === !!pkg.versions['1.3.0'].deprecated, JSON.stringify(row.cautions?.map(c => c.rule)));
  probe.check('latest', row.latestVersion === pkg['dist-tags'].latest && row.isLatest === (pkg['dist-tags'].latest === '1.3.0'), `${row.latestVersion} isLatest ${row.isLatest}`);
  if (!githubRead(probe, 'package', row)) return undefined;
  const linked = pkg.versions['1.3.0'].repository.url.match(/github\.com[/:]([^/]+\/[^/.]+)/)[1];
  const truth = await gh(mode, `/repos/${linked}`);
  if (truth.status !== 200) return probe.skip('trust', unreadReason(truth, 'REST truth unread'));
  return trustCheck(probe, 'trust', { row, truth: truth.json, requested: linked });
}

async function e10Url(mode, probe, published) {
  const result = await cli(mode, 'securityCheck', { queries: [{ operation: 'url', url: 'https://github.com/lodash/lodash' }] });
  contractOf(probe, 'E10.url', result);
  const row = result.data ?? {};
  probe.check('E10.routesToRepository', row.repository?.owner === 'lodash' && row.repository?.name === 'lodash' && !row.errorCode, row.answer ?? row.error);
  if (!githubRead(probe, 'E10', row)) return;
  const listed = sorted([...new Set((row.advisories ?? []).map(a => a.id))]);
  probe.check('E10.repositoryAdvisories', listed === sorted(published), `${listed} vs ${sorted(published)}`);
}

async function e14AdvisoryRepo(mode, probe, record) {
  const result = await cli(mode, 'securityCheck', { queries: [{ operation: 'advisory', id: record.ghsa_id, repo: 'lodash/lodash' }] });
  contractOf(probe, 'advisoryRepo', result);
  const own = result.data?.advisories?.find(a => a.link?.includes('/lodash/lodash/security/advisories/'));
  if (!own) return probe.check('advisoryRepo.row', false, `no repository-sourced row: ${JSON.stringify(result.data?.sources)}`);
  probe.expect('advisoryRepo.matchesRecord', githubAdvisoryProblem(own, record), `publisher ${own.publisher}, ${own.credits?.length ?? 0} credits`);
  return probe.expect('advisoryRepo.cvss', cvssShapeProblem([own]), JSON.stringify(own.cvss));
}

export async function e14(mode) {
  const probe = startProbe(mode, 'E14', 'REST repos/lodash/lodash/security-advisories: published, multi-package; E10 url row lists them; advisory with repo states publisher, credits, cvss');
  const truth = await gh(mode, '/repos/lodash/lodash/security-advisories?per_page=100');
  if (truth.status !== 200) return probe.skip('provider', unreadReason(truth, 'repository advisories unread'));
  shared.repositoryAdvisories = truth.json;
  probe.check('provider.published', truth.json.length > 0 && truth.json.every(a => a.state === 'published'), `${truth.json.length} advisories`);
  probe.check('provider.multiPackage', truth.json.some(a => new Set((a.vulnerabilities ?? []).map(v => v.package?.name)).size > 1));
  await e10Url(mode, probe, truth.json.map(a => a.ghsa_id));
  return e14AdvisoryRepo(mode, probe, truth.json[0]);
}

export async function e15(mode) {
  const probe = startProbe(mode, 'E15', 'REST /advisories (global GHSA): epss and cvss {vector, score}; the advisory row matches the record');
  const list = await gh(mode, '/advisories?affects=lodash&ecosystem=npm&per_page=100');
  const none = await gh(mode, '/advisories?affects=left-pad&ecosystem=npm&per_page=100');
  const record = await gh(mode, '/advisories/GHSA-29mw-wpgm-hmr9');
  const unread = [list, none, record].find(answer => answer.status !== 200);
  if (unread) return probe.skip('provider', unreadReason(unread, 'global advisories unread'));
  shared.globalAdvisories = list.json;
  probe.check('provider.epss', list.json.some(a => typeof a.epss?.percentage === 'number'), `${list.json.length} rows`);
  probe.check('provider.leftPadNone', none.json.length === 0, `${none.json.length} rows`);
  const result = await cli(mode, 'securityCheck', { queries: [{ operation: 'advisory', id: 'GHSA-29mw-wpgm-hmr9' }] });
  contractOf(probe, 'advisory', result);
  probe.expect('cvss', cvssShapeProblem(result.data?.advisories));
  const github = result.data?.advisories?.find(a => a.link?.startsWith('https://github.com/advisories/'));
  if (!github) return probe.check('githubRow', false, JSON.stringify(result.data?.sources));
  return probe.expect('githubRow.matchesRecord', githubAdvisoryProblem(github, record.json), JSON.stringify({ cvss: github.cvss, epss: github.epss }));
}

function e9GithubFacts(probe, row) {
  const facts = row.sources.find(s => s.source === 'github').facts ?? {};
  if (shared.repositoryAdvisories) probe.expect('repositoryAdvisories.namePackage', subsetProblem(facts.repositoryAdvisories, advisoriesNaming(shared.repositoryAdvisories, 'lodash'), 'repositoryAdvisories'), JSON.stringify(facts.repositoryAdvisories));
  else probe.skip('repositoryAdvisories.namePackage', 'E14 provider truth unread');
  if (!shared.globalAdvisories) return probe.skip('globalAdvisories', 'E15 provider truth unread');
  probe.expect('globalAdvisories', subsetProblem(facts.globalAdvisories, shared.globalAdvisories.map(a => a.ghsa_id), 'globalAdvisories'), JSON.stringify(facts.globalAdvisories));
  const epssOf = id => shared.globalAdvisories.find(a => a.ghsa_id === id)?.epss;
  const scored = (row.advisories ?? []).filter(a => typeof epssOf(a.id)?.percentage === 'number');
  const wrong = scored.filter(a => a.epss?.percentage !== epssOf(a.id).percentage || a.epss?.percentile !== epssOf(a.id).percentile);
  return probe.check('epss', scored.length > 0 && !wrong.length, wrong.length ? wrong.map(a => `${a.id} ${JSON.stringify(a.epss)}`).join('; ') : `${scored.length} advisories carry GitHub's EPSS`);
}

export async function e9(mode) {
  const probe = startProbe(mode, 'E9', 'securityCheck package npm lodash@4.17.15: affected, upgradeTo, update fields, EPSS, GitHub advisory facts');
  const [ids] = await osvIds([osvQuery('npm', 'lodash', '4.17.15')]);
  const pkg = await registry('lodash');
  const result = await cli(mode, 'securityCheck', { queries: [{ operation: 'package', ecosystem: 'npm', packageName: 'lodash', version: '4.17.15' }] });
  contractOf(probe, 'package', result);
  const row = result.data ?? {};
  probe.expect('osvVerdict', osvVerdictProblem(row, result.status, ids), `${row.advisories?.length} of OSV ${ids.length}`);
  probe.check('upgradeTo', typeof row.upgradeTo === 'string', row.upgradeTo ?? row.upgradeNote);
  const latest = pkg['dist-tags'].latest;
  const update = { latestVersion: row.latestVersion, isLatest: row.isLatest, outdated: row.outdated, currentPublishedAt: row.currentPublishedAt, latestPublishedAt: row.latestPublishedAt };
  const want = { latestVersion: latest, isLatest: false, outdated: true, currentPublishedAt: pkg.time['4.17.15'].slice(0, 10), latestPublishedAt: pkg.time[latest].slice(0, 10) };
  probe.check('update', JSON.stringify(update) === JSON.stringify(want), `${JSON.stringify(update)} upgradeKind ${row.upgradeKind}`);
  probe.expect('cvss', cvssShapeProblem(row.advisories));
  if (githubRead(probe, 'package', row)) e9GithubFacts(probe, row);
  return row;
}

export async function e11(mode) {
  const probe = startProbe(mode, 'E11', 'securityCheck commit 6879efc2 (harfbuzz): in the affected range');
  const [ids] = await osvIds([{ commit: HARFBUZZ }]);
  const result = await cli(mode, 'securityCheck', { queries: [{ operation: 'commit', commit: HARFBUZZ }] });
  contractOf(probe, 'commit', result);
  probe.expect('osvVerdict', osvVerdictProblem(result.data, result.status, ids, { commit: true }), `OSV ${ids.length}`);
  probe.expect('cvss', cvssShapeProblem(result.data?.advisories));
  return result.data;
}

async function refCheck(mode, probe, { name, sha, facts }, [field, route, pick]) {
  const truth = await gh(mode, `/repos/lodash/lodash/commits/${sha}/${route}`);
  if (truth.status !== 200) return probe.skip(`${name}.${field}`, unreadReason(truth, 'REST truth unread'));
  const want = sorted(truth.json.map(pick));
  return probe.check(`${name}.${field}`, sorted(facts[field]) === want, `${sorted(facts[field])} vs ${want}`);
}

async function commitWithRepo(mode, probe, { name, sha, tag }) {
  const truth = await gh(mode, `/repos/lodash/lodash/commits/${sha}`);
  const result = await cli(mode, 'securityCheck', { queries: [{ operation: 'commit', commit: sha, repo: 'lodash/lodash' }] });
  contractOf(probe, `${name}.commit`, result);
  const row = result.data;
  if (!githubRead(probe, name, row)) return undefined;
  const facts = row.sources.find(s => s.source === 'github').facts ?? {};
  if ([404, 422].includes(truth.status)) return probe.check(`${name}.notInRepository`, facts.exists === false && (row.cautions ?? []).some(c => c.rule === 'commitNotInRepository'), JSON.stringify({ facts, cautions: row.cautions }));
  if (truth.status !== 200) return probe.skip(`${name}.facts`, unreadReason(truth, 'REST truth unread'));
  const signature = truth.json.commit.verification;
  probe.check(`${name}.signature`, facts.exists === true && facts.signature?.verified === signature.verified && facts.signature?.reason === signature.reason, JSON.stringify(facts.signature));
  if (tag) probe.check(`${name}.inTag`, (facts.inTag ?? []).includes(tag), JSON.stringify(facts.inTag));
  await refCheck(mode, probe, { name, sha, facts }, ['heads', 'branches-where-head', branch => branch.name]);
  return refCheck(mode, probe, { name, sha, facts }, ['pulls', 'pulls', pull => pull.number]);
}

export async function e12e13(mode) {
  const probe = startProbe(mode, 'E12-E13', 'securityCheck commit (lodash release commits): OSV proves nothing; with repo, GitHub exists, signature, heads, pulls, tags');
  const pkg = await registry('lodash');
  const latest = pkg['dist-tags'].latest;
  const gitHead = pkg.versions[latest].gitHead;
  const [ids] = await osvIds([{ commit: gitHead }]);
  const bare = await cli(mode, 'securityCheck', { queries: [{ operation: 'commit', commit: gitHead }] });
  contractOf(probe, 'E12.commit', bare);
  probe.expect('E12.osvVerdict', osvVerdictProblem(bare.data, bare.status, ids, { commit: true }), `OSV ${ids.length}`);
  await commitWithRepo(mode, probe, { name: 'registryRef', sha: gitHead });
  const tag = await gh(mode, `/repos/lodash/lodash/commits/${latest}`);
  if (tag.status === 200) await commitWithRepo(mode, probe, { name: 'releaseTag', sha: tag.json.sha, tag: latest });
  else probe.skip('releaseTag', unreadReason(tag, 'release tag unread'));
  await commitWithRepo(mode, probe, { name: 'madeUp', sha: gitHead.slice(0, -1) + (gitHead.endsWith('0') ? '1' : '0') });
}

export async function e16(mode) {
  const probe = startProbe(mode, 'E16', 'REST /rate_limit: the anonymous core limit is below the authenticated one');
  const limits = await gh(mode, '/rate_limit', { token: mode.token });
  probe.check('provider', limits.status === 200 && typeof limits.json?.resources?.core?.limit === 'number', limits.text.slice(0, 120));
  return limits.json?.resources?.core;
}

export async function e17(mode) {
  const probe = startProbe(mode, 'E17', 'GraphQL aliased repositories: one request, a missing alias is null with NOT_FOUND, the rest answer');
  const query = 'query { a: repository(owner:"lodash", name:"lodash") { stargazerCount isArchived pushedAt } b: repository(owner:"left-pad", name:"left-pad") { isArchived } c: repository(owner:"tmcw", name:"leftpad") { stargazerCount } d: repository(owner:"octocode-security-live", name:"no-such-repository") { stargazerCount } rateLimit { cost } }';
  const answer = await gh(mode, '/graphql', { method: 'POST', body: { query }, token: mode.token });
  // GitHub refuses anonymous GraphQL: 401, or 403 from its anonymous GraphQL quota of 0.
  if (!mode.token) return probe.check('anonymousRefused', [401, 403].includes(answer.status) && !answer.json?.data, `HTTP ${answer.status} ${answer.text.slice(0, 80)}`);
  const d = answer.json?.data ?? {};
  const notFound = (answer.json?.errors ?? []).some(e => e.type === 'NOT_FOUND' && e.path?.[0] === 'd');
  probe.check('partialSuccess', !!(d.a && d.b?.isArchived && d.c) && d.d === null && notFound, JSON.stringify(answer.json).slice(0, 200));
  return probe.check('cost', typeof d.rateLimit?.cost === 'number', JSON.stringify(d.rateLimit));
}

export async function e18(mode) {
  const probe = startProbe(mode, 'E18', 'OSV querybatch: positional ids-only results, a missing package is {}');
  const queries = [osvQuery('npm', 'lodash', '4.17.15'), osvQuery('npm', 'left-pad', '1.3.0'), osvQuery('npm', 'octocode-security-live-no-such-package', '1.0.0'), { commit: HARFBUZZ }];
  const answer = await provider(`${OSV}/v1/querybatch`, { method: 'POST', body: { queries } });
  const results = answer.json?.results ?? [];
  const counts = results.map(r => r.vulns?.length ?? 0);
  probe.check('positional', answer.status === 200 && results.length === queries.length, `HTTP ${answer.status}, ${results.length} results`);
  probe.check('idsOnly', results.flatMap(r => r.vulns ?? []).every(v => Object.keys(v).every(k => ['id', 'modified'].includes(k))), JSON.stringify(results[0]?.vulns?.[0]));
  probe.check('missingIsEmpty', JSON.stringify(results[2]) === '{}', JSON.stringify(results[2]));
  probe.check('verdicts', counts[0] > 0 && counts[1] === 0 && counts[3] > 0, counts.join(','));
}

export async function e19(mode) {
  const probe = startProbe(mode, 'E19', 'OSV GET /v1/vulns/GHSA-29mw-wpgm-hmr9: the advisory row states the record; cvss vector in vector, no score');
  const record = await provider(`${OSV}/v1/vulns/GHSA-29mw-wpgm-hmr9`);
  probe.check('provider', record.status === 200 && ['aliases', 'modified', 'affected', 'references'].every(k => k in (record.json ?? {})), Object.keys(record.json ?? {}).join(','));
  const result = await cli(mode, 'securityCheck', { queries: [{ operation: 'advisory', id: 'GHSA-29mw-wpgm-hmr9' }] });
  contractOf(probe, 'advisory', result);
  const osv = result.data?.advisories?.find(a => a.link?.startsWith('https://osv.dev/'));
  if (!osv) return probe.check('osvRow', false, JSON.stringify(result.data?.sources));
  probe.check('record', sorted(osv.aliases) === sorted(record.json.aliases) && osv.modified === record.json.modified, JSON.stringify({ aliases: osv.aliases, modified: osv.modified }));
  return probe.expect('cvssVectorNotScore', osvCvssProblem(osv, record.json), JSON.stringify(osv.cvss));
}

export async function e20(mode) {
  const probe = startProbe(mode, 'E20', 'OSV querybatch limit: 1000 answer, 1001 is HTTP 400 too many queries; the tool takes at most 5 rows');
  const batch = n => ({ queries: Array.from({ length: n }, () => osvQuery('npm', 'lodash', '4.17.15')) });
  const full = await provider(`${OSV}/v1/querybatch`, { method: 'POST', body: batch(1000) });
  probe.check('provider.1000', full.status === 200 && full.json.results.length === 1000, `HTTP ${full.status}`);
  const over = await provider(`${OSV}/v1/querybatch`, { method: 'POST', body: batch(1001) });
  probe.check('provider.1001', over.status === 400 && /too many queries/.test(over.text), `HTTP ${over.status} ${over.text.slice(0, 80)}`);
  const six = await cli(mode, 'securityCheck', { queries: Array.from({ length: 6 }, () => ({ operation: 'advisory', id: 'GHSA-29mw-wpgm-hmr9' })) });
  probe.check('tool.sixRows', six.exit === 2 && six.json?.errorCode === 'invalidInput', `exit ${six.exit} ${six.json?.errorCode}`);
}

export async function e21(mode) {
  const probe = startProbe(mode, 'E21', 'OSV querybatch django: page tokens walked to the end; the django@3.0 row is complete');
  const [all] = await osvIds([osvQuery('PyPI', 'django')]);
  probe.check('provider.walk', all.length > 0, `${all.length} ids for every django version`);
  const [ids] = await osvIds([osvQuery('PyPI', 'django', '3.0')]);
  const result = await cli(mode, 'securityCheck', { queries: [{ operation: 'package', ecosystem: 'PyPI', packageName: 'django', version: '3.0' }] });
  contractOf(probe, 'package', result);
  const row = result.data ?? {};
  probe.expect('osvVerdict', osvVerdictProblem(row, result.status, ids), `${row.advisories?.length} of OSV ${ids.length}`);
  probe.check('complete', !row.isPartial && !row.next && row.pagination?.hasMore !== true, JSON.stringify({ isPartial: row.isPartial, partialReasons: row.partialReasons }));
}
