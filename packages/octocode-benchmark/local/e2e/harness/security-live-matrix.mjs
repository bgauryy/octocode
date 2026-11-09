// The B169 validation matrix (.octocode/security-check/validation-matrix*.md) live: each
// ecosystem row against OSV's own answer for what the tool sent, the refusals, and MCP parity.
import { startServer } from './mcp-client.mjs';
import { cvssShapeProblem, githubSource, osvVerdictProblem, unversionedVerdictProblem } from './security-live-oracle.mjs';
import { osvQuery } from './security-live-probes.mjs';
import { cli, currentProbe, osvIds, startProbe } from './security-live-calls.mjs';

/** [ecosystem, packageName, version, release]: the rows of validation-matrix-r2.md not already an E probe. */
export const MATRIX = [
  ['crates.io', 'time', '0.1.43'], ['Maven', 'org.apache.logging.log4j:log4j-core', '2.14.1'], ['NuGet', 'Newtonsoft.Json', '12.0.1'],
  ['Go', 'golang.org/x/net', '0.7.0'], ['Packagist', 'symfony/http-kernel', '4.4.0'], ['RubyGems', 'rails', '5.2.0'],
  ['Pub', 'http', '0.12.0'], ['Hex', 'plug', '1.0.0'], ['SwiftURL', 'github.com/vapor/vapor', '4.0.0'],
  ['GitHub Actions', 'tj-actions/changed-files', '40.0.0'], ['CRAN', 'jsonlite', '1.0'], ['Bioconductor', 'Biostrings'],
  ['Debian', 'openssl', '1.1.1n-0+deb11u1', '11'], ['Ubuntu', 'openssl', '3.0.2-0ubuntu1', '22.04:LTS'], ['Alpine', 'openssl', '1.1.1l-r0', 'v3.14'],
];

const codeOf = (row, json) => row?.errorCode ?? json?.errorCode;
/** {label, query, holds(row, json, status)}: validation-matrix.md cases 12, 21, 22, 31, 36 and the v2 release refusals. */
export const REFUSALS = [
  { label: 'notFound', query: { operation: 'package', ecosystem: 'npm', packageName: 'octocode-security-live-no-such-package' }, holds: row => row?.errorCode === 'notFound' && !('advisories' in row) },
  { label: 'mavenNoGroup', query: { operation: 'package', ecosystem: 'Maven', packageName: 'log4j-core' }, holds: (row, json) => codeOf(row, json) === 'invalidInput' },
  { label: 'unlistedVersion', query: { operation: 'package', ecosystem: 'npm', packageName: 'lodash', version: '99.0.0' }, holds: row => row?.versionEvaluated === false },
  { label: 'shortCommit', query: { operation: 'commit', commit: 'abc1234' }, holds: (row, json) => codeOf(row, json) === 'invalidInput' },
  { label: 'unknownAdvisory', query: { operation: 'advisory', id: 'CVE-1999-0001' }, holds: row => row?.errorCode === 'notFound' },
  { label: 'npmRelease', query: { operation: 'package', ecosystem: 'npm', packageName: 'lodash', version: '4.17.15', release: '11' }, holds: (row, json) => codeOf(row, json) === 'invalidInput' },
  { label: 'wrongReleaseForm', query: { operation: 'package', ecosystem: 'Alpine', packageName: 'openssl', version: '1.1.1l-r0', release: '3.14' }, holds: (row, _json, status) => status !== 'empty' && (row?.sources ?? []).some(s => s.source === 'osv' && s.status === 'notCovered' && s.why) },
];

/** The v2 trust rows of validation-matrix.md; each reads GitHub, so they run authenticated only. */
export const TRUST_MATRIX = ['application-stacks/runtime-component-operator', 'meh/dux', 'salesforce/generic-sidecar-injector', 'kubernetes/helm', 'octocode-security-live/no-such-repository'];


async function ecosystemRow(mode, probe, [ecosystem, packageName, version, release]) {
  const label = [ecosystem, packageName].join(':');
  const result = await cli(mode, 'securityCheck', { queries: [{ operation: 'package', ecosystem, packageName, ...(version && { version }), ...(release && { release }) }] });
  probe.expect(`${label}.contract`, result.problem);
  const row = result.data ?? {};
  const [ids] = await osvIds([osvQuery(ecosystem, row.packageName ?? packageName, row.version ?? version, release)]);
  const verdict = osvVerdictProblem(row, result.status, ids, { allVersions: !version });
  if (!verdict || row.registryConfirmed !== false || !version) probe.expect(`${label}.osvVerdict`, verdict, `${row.advisories?.length} of OSV ${ids.length}`);
  else {
    // OSV answered nothing usable at this version: compare with its records of the package.
    const [allIds] = await osvIds([osvQuery(ecosystem, packageName, undefined, release)]);
    probe.expect(`${label}.osvVerdict`, unversionedVerdictProblem(row, allIds), `ranges compared by the tool: ${row.advisories?.length} of OSV's ${allIds.length} records of the package (OSV at ${version}: ${ids.length})`);
  }
  probe.expect(`${label}.cvss`, cvssShapeProblem(row.advisories));
  const source = githubSource(row);
  if (source.state !== 'absent') probe.expect(`${label}.githubDisclosed`, source.problem, source.state);
}

export async function matrix(mode) {
  const probe = startProbe(mode, 'B169', 'validation matrix: every ecosystem row against OSV, and the refusals');
  for (const row of MATRIX) await ecosystemRow(mode, probe, row);
  for (const { label, query, holds } of REFUSALS) {
    const result = await cli(mode, 'securityCheck', { queries: [query] });
    probe.check(`${label}.refused`, holds(result.data, result.json, result.status), JSON.stringify(result.data ?? result.json).slice(0, 200));
  }
}

const stable = row => row && JSON.stringify({
  answer: row.answer, affected: row.affected, upgradeTo: row.upgradeTo,
  advisories: (row.advisories ?? []).map(a => [a.id, a.link, a.cves]), cautions: (row.cautions ?? []).map(c => c.rule), sources: (row.sources ?? []).map(s => [s.source, s.status]),
});

/** MCP with OCTOCODE_SECURITY_CHECKS=true answers as the CLI did: `cliRows` maps a name to {input, row}. */
export async function mcpParity(mode, cliRows, timeoutMs) {
  const probe = startProbe(mode, 'MCP', 'MCP with OCTOCODE_SECURITY_CHECKS=true answers as the CLI');
  const client = await startServer({ env: mode.env, replaceEnv: true, timeoutMs });
  try {
    probe.check('listed', client.tools.some(tool => tool.name === 'securityCheck'), client.tools.map(t => t.name).join(','));
    for (const [name, { input, row }] of Object.entries(cliRows)) {
      const args = { queries: [input], responseChars: 1_000_000 };
      currentProbe().calls.push({ surface: 'mcp', tool: 'securityCheck', input: args });
      const entry = await client.raw('securityCheck', args, name, { keepRaw: true });
      const served = entry.raw?.results?.[0]?.data;
      probe.check(`${name}.sameAsCli`, !entry.isError && stable(served) === stable(row), `${stable(served)?.slice(0, 160)} vs ${stable(row)?.slice(0, 160)}`);
    }
  } finally { client.close(); }
}
