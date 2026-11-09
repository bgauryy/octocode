import assert from 'node:assert/strict';
import test from 'node:test';
import { contract } from './contract.mjs';
import {
  advisoriesNaming,
  cautionShapeProblem,
  contractPolicy,
  cvssShapeProblem,
  expectedRepositoryRules,
  githubAdvisoryProblem,
  githubSource,
  osvCvssProblem,
  osvVerdictProblem,
  unversionedVerdictProblem,
  repositoryFactsProblem,
  ruleSetProblem,
  subsetProblem,
  wholeDays,
} from './security-live-oracle.mjs';

const policy = contractPolicy(contract);
const repo = (over = {}) => ({ full_name: 'tmcw/leftpad', name: 'leftpad', owner: { login: 'tmcw' }, stargazers_count: 5, pushed_at: '2017-05-03T12:00:00Z', archived: false, fork: false, default_branch: 'master', ...over });

test('the policy comes from the published contract', () => {
  assert.equal(typeof policy.lowTractionStars, 'number');
  assert.equal(typeof policy.unmaintainedAfterDays, 'number');
});

test('expected rules sit on each side of both thresholds', () => {
  const today = '2026-11-08';
  const pushedDaysAgo = days => new Date(Date.parse(`${today}T00:00:00Z`) - days * 86_400_000).toISOString();
  const healthy = repo({ stargazers_count: policy.lowTractionStars, pushed_at: pushedDaysAgo(policy.unmaintainedAfterDays) });
  assert.deepEqual([...expectedRepositoryRules(healthy, 'tmcw/leftpad', policy, today)], []);
  const stale = repo({ stargazers_count: policy.lowTractionStars - 1, pushed_at: pushedDaysAgo(policy.unmaintainedAfterDays + 1), archived: true });
  assert.deepEqual([...expectedRepositoryRules(stale, 'tmcw/leftpad', policy, today)].sort(), ['archived', 'lowTraction', 'unmaintained']);
  assert.ok(expectedRepositoryRules(healthy, 'stevemao/left-pad', policy, today).has('repositoryMoved'));
  assert.equal(wholeDays('2026-11-07T23:59:59Z', '2026-11-08'), 1);
});

test('rule sets compare only the rules considered', () => {
  const cautions = [{ rule: 'unmaintained' }, { rule: 'deprecated' }];
  assert.equal(ruleSetProblem(cautions, new Set(['unmaintained']), new Set(['unmaintained', 'lowTraction'])), null);
  assert.match(ruleSetProblem(cautions, new Set(['lowTraction']), new Set(['unmaintained', 'lowTraction'])), /missing \[lowTraction\], unexpected \[unmaintained\]/);
  assert.match(cautionShapeProblem([{ level: 'stop', basis: 'reported', rule: 'archived', why: 'x' }]), /malformed/);
});

test('repository facts follow the REST record, tolerating star drift only', () => {
  const block = { owner: 'tmcw', name: 'leftpad', archived: false, pushedAt: '2017-05-03', defaultBranch: 'master', fork: false, stars: 6 };
  assert.equal(repositoryFactsProblem(block, repo()), null);
  assert.match(repositoryFactsProblem({ ...block, pushedAt: '2026-10-29' }, repo()), /pushedAt/);
  assert.match(repositoryFactsProblem({ ...block, stars: 500 }, repo()), /stars/);
});

test('a refused GitHub source must say why and leave the row partial', () => {
  assert.equal(githubSource({ sources: [{ source: 'github', status: 'found' }] }).state, 'read');
  const refused = { source: 'github', status: 'failed', errorCode: 'rateLimited', why: 'resets 18:00' };
  assert.equal(githubSource({ sources: [refused], isPartial: true, partialReasons: ['sourceFailed'] }).problem, null);
  assert.match(githubSource({ sources: [refused] }).problem, /sourceFailed/);
  assert.match(githubSource({ sources: [{ source: 'github', status: 'unavailable' }] }).problem, /errorCode.*why/);
  assert.equal(githubSource({ sources: [{ source: 'osv', status: 'found' }] }).state, 'absent');
});

test('OSV verdicts: affected rows list only OSV ids; empty commits never claim affected', () => {
  assert.equal(osvVerdictProblem({ affected: true, advisories: [{ id: 'A' }] }, undefined, ['A', 'B']), null);
  assert.match(osvVerdictProblem({ affected: true, advisories: [{ id: 'C' }] }, undefined, ['A']), /did not return: C/);
  assert.match(osvVerdictProblem({ affected: false, advisories: [] }, 'empty', ['A']), /affected is false/);
  assert.equal(osvVerdictProblem({ affected: false, advisories: [] }, 'empty', []), null);
  assert.equal(osvVerdictProblem({ advisories: [] }, 'empty', [], { commit: true }), null);
  assert.match(osvVerdictProblem({ affected: false, advisories: [] }, 'empty', [], { commit: true }), /affected/);
  assert.equal(osvVerdictProblem({ advisories: [{ id: 'A' }] }, undefined, ['A'], { allVersions: true }), null);
  assert.match(osvVerdictProblem({ affected: true, advisories: [{ id: 'A' }] }, undefined, ['A'], { allVersions: true }), /affected is true/);
});

test('an unordered ecosystem row lists only OSV records of the package', () => {
  assert.equal(unversionedVerdictProblem({ affected: true, advisories: [{ id: 'A' }] }, ['A', 'B']), null);
  assert.match(unversionedVerdictProblem({ affected: true, advisories: [{ id: 'C' }] }, ['A']), /does not keep.*C/);
  assert.match(unversionedVerdictProblem({ affected: true, advisories: [] }, ['A']), /affected true with 0/);
});

test('cvss: the vector is a vector, the score a number, and OSV rows carry no score', () => {
  const vector = 'CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:L';
  assert.equal(cvssShapeProblem([{ id: 'G', cvss: [{ type: 'CVSS_V3', vector, score: 5.3 }, { type: 'CVSS_V2', vector: 'AV:N/AC:L/Au:N/C:P/I:N/A:N' }] }]), null);
  assert.match(cvssShapeProblem([{ id: 'G', cvss: [{ type: 'CVSS_V3', vector: '5.3', score: vector }] }]), /vector "5.3".*score "CVSS/);
  // OSV's Ubuntu severity is a priority label, not a CVSS vector.
  assert.match(cvssShapeProblem([{ id: 'U', cvss: [{ type: 'Ubuntu', vector: 'medium' }] }]), /U: vector "medium"/);
  const record = { severity: [{ type: 'CVSS_V3', score: vector }] };
  assert.equal(osvCvssProblem({ cvss: [{ type: 'CVSS_V3', vector }] }, record), null);
  assert.match(osvCvssProblem({ cvss: [{ type: 'CVSS_V3', vector, score: 5.3 }] }, record), /never sent/);
  assert.match(osvCvssProblem({ cvss: [] }, record), /OSV severity/);
});

test('GitHub records: cvss score, EPSS, credits and publisher match the row', () => {
  const vector = 'CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:L';
  const global = { cvss_severities: { cvss_v3: { score: 5.3, vector_string: vector }, cvss_v4: { score: null, vector_string: null } }, epss: { percentage: 0.07, percentile: 0.9 }, credits: [{ user: { login: 'b' } }, { user: { login: 'a' } }] };
  const row = { cvss: [{ type: 'CVSS_V3', vector, score: 5.3 }], epss: { percentage: 0.07, percentile: 0.9 }, credits: [{ login: 'a' }, { login: 'b' }] };
  assert.equal(githubAdvisoryProblem(row, global), null);
  assert.match(githubAdvisoryProblem({ ...row, epss: undefined }, global), /epss/);
  const repository = { cvss: { score: 5.3, vector_string: vector }, credits: [{ login: 'a' }], publisher: { login: 'p' } };
  assert.match(githubAdvisoryProblem({ ...row, credits: [{ login: 'a' }], publisher: 'q' }, repository), /publisher q != p/);
});

test('repository advisories are filtered by the package they name', () => {
  const list = [{ ghsa_id: 'A', vulnerabilities: [{ package: { name: 'lodash' } }] }, { ghsa_id: 'B', vulnerabilities: [{ package: { name: 'lodash-es' } }] }];
  assert.deepEqual(advisoriesNaming(list, 'lodash'), ['A']);
  assert.equal(subsetProblem(['A'], ['A'], 'ids'), null);
  assert.match(subsetProblem(['B'], ['A'], 'ids'), /outside.*B/);
});
