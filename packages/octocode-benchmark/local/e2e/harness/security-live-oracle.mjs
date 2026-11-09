// Pure oracles for the security-live suite: each compares one securityCheck row with what the
// provider itself answered in the same run (GitHub REST, OSV), so the live checks assert the
// relation between the tool and its source, never a copied star count or advisory total.

/** The trust thresholds the published contract states under `security.policy`. */
export function contractPolicy(contract) {
  const policy = contract.security?.policy;
  return Number.isFinite(policy?.lowTractionStars) && Number.isFinite(policy?.unmaintainedAfterDays)
    ? { lowTractionStars: policy.lowTractionStars, unmaintainedAfterDays: policy.unmaintainedAfterDays }
    : null;
}

/** Whole days from `from` to `to`, both read as UTC calendar days (`YYYY-MM-DD` or ISO). */
export function wholeDays(from, to) {
  const day = text => Date.parse(`${String(text).slice(0, 10)}T00:00:00Z`);
  return Math.round((day(to) - day(from)) / 86_400_000);
}

/**
 * The repository rules GitHub's own REST record implies under `policy` on `today`:
 * `requested` is the `owner/name` asked for, `truth` the `GET /repos/{owner}/{repo}` body.
 */
export function expectedRepositoryRules(truth, requested, policy, today) {
  const rules = new Set();
  if (truth.stargazers_count < policy.lowTractionStars) rules.add('lowTraction');
  if (wholeDays(truth.pushed_at, today) > policy.unmaintainedAfterDays) rules.add('unmaintained');
  if (truth.archived) rules.add('archived');
  if (truth.full_name.toLowerCase() !== requested.toLowerCase()) rules.add('repositoryMoved');
  return rules;
}

/** null when the row's caution rules (restricted to `considered`) equal `expected`. */
export function ruleSetProblem(cautions = [], expected, considered = expected) {
  const fired = new Set(cautions.map(caution => caution.rule).filter(rule => considered.has(rule)));
  const missing = [...expected].filter(rule => !fired.has(rule));
  const extra = [...fired].filter(rule => !expected.has(rule));
  return missing.length || extra.length ? `missing [${missing.join(', ')}], unexpected [${extra.join(', ')}]` : null;
}

/** Every caution carries the contract's required fields with a non-empty `why`. */
export function cautionShapeProblem(cautions = []) {
  const bad = cautions.filter(c => !['review', 'note'].includes(c.level) || !['reported', 'heuristic'].includes(c.basis) || !c.rule || !c.why);
  return bad.length ? `malformed cautions: ${JSON.stringify(bad).slice(0, 200)}` : null;
}

/** The repository block states the facts GitHub's REST record holds. */
export function repositoryFactsProblem(block, truth) {
  if (!block) return 'no repository block';
  const want = { owner: truth.owner.login, name: truth.name, archived: truth.archived, pushedAt: truth.pushed_at.slice(0, 10), defaultBranch: truth.default_branch, fork: truth.fork };
  const wrong = Object.entries(want).filter(([key, value]) => block[key] !== value).map(([key, value]) => `${key}: ${JSON.stringify(block[key])} != ${JSON.stringify(value)}`);
  // Stars move between two reads seconds apart; the side of the threshold is what a caution uses.
  if (typeof block.stars !== 'number' || Math.abs(block.stars - truth.stargazers_count) > Math.max(5, truth.stargazers_count * 0.001)) wrong.push(`stars: ${block.stars} vs ${truth.stargazers_count}`);
  return wrong.length ? wrong.join('; ') : null;
}

/**
 * How the row disclosed its GitHub source. `read`: GitHub answered. `disclosed`: it was asked
 * and refused (rate limit, no token), which must carry `errorCode` and `why`, and a failed
 * source leaves the row partial with `sourceFailed`. `absent` is a defect where GitHub applies.
 */
export function githubSource(row) {
  const entry = row?.sources?.find(source => source.source === 'github');
  if (!entry) return { state: 'absent', problem: 'no github entry in sources' };
  if (['found', 'noneKnown', 'partial'].includes(entry.status)) return { state: 'read', entry, problem: null };
  if (entry.status === 'notCovered') return { state: 'notCovered', entry, problem: null };
  const problems = [];
  if (!entry.errorCode) problems.push(`github ${entry.status} without errorCode`);
  if (!entry.why) problems.push(`github ${entry.status} without why`);
  if (entry.status === 'failed' && !(row.isPartial && row.partialReasons?.includes('sourceFailed'))) problems.push('github failed but the row is not isPartial with sourceFailed');
  return { state: 'disclosed', entry, problem: problems.join('; ') || null };
}

/**
 * OSV's verdict for a target against the row: `osvIds` are the ids OSV querybatch returned.
 * `commit`: an empty answer never claims `affected`. `allVersions`: no version was evaluated
 * (an ecosystem without a registry reader, asked with no version), so `affected` is absent.
 */
export function osvVerdictProblem(row = {}, status, osvIds, { commit = false, allVersions = false } = {}) {
  const ids = (row.advisories ?? []).map(advisory => advisory.id);
  return osvIds.length ? listedProblem(row, ids, osvIds, allVersions) : noneKnownProblem(row, status, ids, commit || allVersions);
}

function listedProblem(row, ids, osvIds, allVersions) {
  const claimed = allVersions ? !('affected' in row) : row.affected === true;
  if (!claimed) return `OSV lists ${osvIds.length} records but affected is ${row.affected}`;
  const unknown = ids.filter(id => !osvIds.includes(id));
  if (unknown.length) return `advisories OSV did not return: ${unknown.join(', ')}`;
  return ids.length ? null : 'affected with no advisories';
}

function noneKnownProblem(row, status, ids, affectedAbsent) {
  if (status !== 'empty' || ids.length) return `OSV lists none but status ${status} with ${ids.length} advisories`;
  const wrong = affectedAbsent ? 'affected' in row : row.affected === true;
  return wrong ? `OSV lists none but affected is ${row.affected}` : null;
}

/**
 * A row OSV could not answer at the version (an ecosystem whose versions OSV cannot order, such as
 * GitHub Actions, so OSV's version query lists nothing): the tool compares each record's ranges
 * itself. Its advisories must still be OSV's records of the package (`allIds`, asked with no
 * version), and `affected` must follow whether it listed any.
 */
export function unversionedVerdictProblem(row = {}, allIds) {
  const ids = (row.advisories ?? []).map(advisory => advisory.id);
  const unknown = ids.filter(id => !allIds.includes(id));
  if (unknown.length) return `advisories OSV does not keep for the package: ${unknown.join(', ')}`;
  return (row.affected === true) === ids.length > 0 ? null : `affected ${row.affected} with ${ids.length} advisories`;
}

const CVSS_VECTOR = /^(CVSS:\d\.\d\/)?[A-Z]{1,3}:[A-Z]/;

/** cvss entries carry the vector as published and, when the source states one, a numeric score. */
export function cvssShapeProblem(advisories = []) {
  const bad = [];
  for (const advisory of advisories) {
    for (const entry of advisory.cvss ?? []) {
      if (typeof entry.vector !== 'string' || !CVSS_VECTOR.test(entry.vector)) bad.push(`${advisory.id}: vector ${JSON.stringify(entry.vector)}`);
      if ('score' in entry && (typeof entry.score !== 'number' || entry.score < 0 || entry.score > 10)) bad.push(`${advisory.id}: score ${JSON.stringify(entry.score)}`);
    }
  }
  return bad.length ? bad.join('; ') : null;
}

/**
 * OSV publishes the CVSS vector in `severity[].score`: the OSV-sourced row must carry it as
 * `vector`, with no `score` (OSV gives none). `record` is `GET /v1/vulns/{id}`.
 */
export function osvCvssProblem(row, record) {
  const published = (record.severity ?? []).filter(s => String(s.type).startsWith('CVSS')).map(s => s.score).sort();
  const vectors = (row?.cvss ?? []).map(entry => entry.vector).sort();
  if (JSON.stringify(published) !== JSON.stringify(vectors)) return `vectors ${JSON.stringify(vectors)} != OSV severity[].score ${JSON.stringify(published)}`;
  const scored = (row?.cvss ?? []).filter(entry => 'score' in entry);
  return scored.length ? `OSV row carries a score OSV never sent: ${JSON.stringify(scored)}` : null;
}

function githubCvssProblems(row, record) {
  const severities = Object.values(record.cvss_severities ?? {}).filter(severity => severity?.vector_string);
  const published = severities.length || !record.cvss?.vector_string ? severities : [record.cvss];
  return published.flatMap(severity => {
    const entry = (row.cvss ?? []).find(c => c.vector === severity.vector_string);
    if (!entry) return [`vector ${severity.vector_string} missing`];
    return typeof severity.score === 'number' && entry.score !== severity.score ? [`score ${entry.score} != ${severity.score}`] : [];
  });
}

function githubEpssProblems(row, record) {
  if (typeof record.epss?.percentage !== 'number') return [];
  const want = { percentage: record.epss.percentage, percentile: record.epss.percentile };
  return row.epss?.percentage === want.percentage && row.epss?.percentile === want.percentile ? [] : [`epss ${JSON.stringify(row.epss)} != ${JSON.stringify(want)}`];
}

function githubAttributionProblems(row, record) {
  const problems = [];
  const credits = (record.credits ?? []).map(c => c.user?.login ?? c.login).filter(Boolean).sort();
  const listed = (row.credits ?? []).map(c => c.login).sort();
  if (credits.length && JSON.stringify(credits) !== JSON.stringify(listed)) problems.push(`credits ${listed.join(',')} != ${credits.join(',')}`);
  if (record.publisher?.login && row.publisher !== record.publisher.login) problems.push(`publisher ${row.publisher} != ${record.publisher.login}`);
  return problems;
}

/**
 * A GitHub advisory record's cvss, EPSS, credits and publisher against the row GitHub sourced.
 * `record` is `GET /advisories/{ghsa}` (credits under `user.login`) or one item of
 * `GET /repos/{o}/{r}/security-advisories` (credits under `login`, plus `publisher`).
 */
export function githubAdvisoryProblem(row = {}, record) {
  const problems = [...githubCvssProblems(row, record), ...githubEpssProblems(row, record), ...githubAttributionProblems(row, record)];
  return problems.length ? problems.join('; ') : null;
}

/** Repository advisories that name `packageName`, from `GET /repos/{o}/{r}/security-advisories`. */
export function advisoriesNaming(repositoryAdvisories, packageName) {
  return repositoryAdvisories.filter(advisory => (advisory.vulnerabilities ?? []).some(v => v.package?.name === packageName)).map(advisory => advisory.ghsa_id);
}

/** null when `ids` holds no value outside `allowed`. */
export function subsetProblem(ids = [], allowed, label) {
  const outside = ids.filter(id => !allowed.includes(id));
  return outside.length ? `${label} outside the provider's list: ${outside.join(', ')}` : null;
}
