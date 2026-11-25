#!/usr/bin/env node
// Resolve planned dependency versions from public registries and report deprecations, engine misfits and peer conflicts.
import { fail, errorMessage } from './lib/fail.mjs';
import { isMainModule } from './lib/entrypoint.mjs';
import { maxSatisfying, satisfies, parse, compare } from './lib/semver.mjs';

const HELP = `Usage: node <skill-folder>/scripts/check-versions.mjs <spec>... [--node <version>] [--registry <url>]

Resolves each planned dependency before it is written into a plan or installed. A spec is
  <name>              npm package at its latest dist-tag
  <name>@<range|tag>  npm package at the highest version matching the range, or that dist-tag
  crates:<name>       Rust crate: latest stable version from crates.io
  pypi:<name>         Python package: latest version from PyPI
npm checks treat the specs as one install set: each chosen version's peerDependencies are matched
against the versions chosen for the other specs, and engines.node against --node.

Options:
  --node <version>   Node.js version the project targets (default: the running Node.js)
  --registry <url>   npm registry base URL (default: https://registry.npmjs.org)
  --help             Show this help

Output: one JSON object on stdout.
  ok             true when conflicts, deprecated, engineMismatches and errors are all empty
  node           the Node.js version engines were checked against
  packages       [{ecosystem, name, requested, version, latest, deprecated, engines, peers}]
  conflicts      [{package, version, peer, range, chosen, fixPeer, fixPackage}]: a chosen version
                 outside a peer range; fixPeer is the highest peer version the range accepts,
                 fixPackage the highest version of the package whose peer range accepts the chosen
                 peer (null when none exists)
  missingPeers   [{package, peer, range}]: required peers not in the spec list (install them too)
  deprecated     [{name, version, message}]
  engineMismatches [{name, version, node, range}]
  unchecked      [{where, range}]: registry ranges this checker cannot parse; verify them by hand
  errors         [{spec, code, message}]: not-found, no-match or fetch-failed
Effects: HTTPS GET requests to the npm registry, crates.io and pypi.org; writes nothing.
Exit codes: 0 ok; 1 findings or lookup errors; 2 invalid input, as {"ok":false,"error":{code,message}}.`;

const DEFAULT_REGISTRY = 'https://registry.npmjs.org';
const USER_AGENT = 'octocode-generate-skill (https://github.com/bgauryy/octocode)';

/** Split argv into {specs, node, registry}; throws a usage error message. */
export function parseArgs(argv) {
  const options = { specs: [], node: process.versions.node, registry: DEFAULT_REGISTRY };
  for (let index = 0; index < argv.length; index++) {
    const arg = argv[index];
    if (arg === '--node' || arg === '--registry') {
      const value = argv[++index];
      if (!value) throw new Error(`${arg} needs a value`);
      if (arg === '--node' && !parse(value.replace(/^v/, ''))) throw new Error(`--node needs a full version such as 26.13.1, got ${value}`);
      options[arg.slice(2)] = arg === '--node' ? value.replace(/^v/, '') : value.replace(/\/+$/, '');
    } else if (arg.startsWith('--')) throw new Error(`unknown option ${arg}`);
    else options.specs.push(arg);
  }
  if (!options.specs.length) throw new Error('give at least one package spec');
  return options;
}

/** {ecosystem, name, requested} from one spec; scoped npm names keep their leading @. */
export function parseSpec(spec) {
  const prefixed = /^(crates|pypi):(.+)$/.exec(spec);
  if (prefixed) return { ecosystem: prefixed[1], name: prefixed[2], requested: 'latest' };
  const at = spec.indexOf('@', spec.startsWith('@') ? 1 : 0);
  return at > 0 ? { ecosystem: 'npm', name: spec.slice(0, at), requested: spec.slice(at + 1) || 'latest' } : { ecosystem: 'npm', name: spec, requested: 'latest' };
}

async function getJson(fetchImpl, url, headers = {}) {
  const response = await fetchImpl(url, { headers: { 'user-agent': USER_AGENT, ...headers } });
  if (response.status === 404) return { missing: true };
  if (!response.ok) throw new Error(`HTTP ${response.status} from ${url}`);
  return { body: await response.json() };
}

function pickNpmVersion(document, requested) {
  const tags = document['dist-tags'] ?? {};
  if (tags[requested]) return tags[requested];
  return maxSatisfying(Object.keys(document.versions ?? {}), requested);
}

async function resolveNpm(fetchImpl, registry, spec) {
  const url = `${registry}/${spec.name.replace('/', '%2f')}`;
  const { missing, body } = await getJson(fetchImpl, url, { accept: 'application/vnd.npm.install-v1+json' });
  if (missing) return { error: 'not-found', message: `${spec.name} is not on the npm registry` };
  const version = pickNpmVersion(body, spec.requested);
  if (!version) return { error: 'no-match', message: `no ${spec.name} version matches ${spec.requested}` };
  const manifest = body.versions[version] ?? {};
  return {
    package: { ...spec, version, latest: body['dist-tags']?.latest ?? null, deprecated: manifest.deprecated || null,
      engines: manifest.engines ?? null, peers: manifest.peerDependencies ?? {} },
    // Optional peers, and peers the package also installs as a dependency, need no separate install.
    optionalPeers: new Set([...Object.entries(manifest.peerDependenciesMeta ?? {}).filter(([, meta]) => meta?.optional).map(([name]) => name),
      ...Object.keys(manifest.dependencies ?? {})]),
    versions: body.versions,
  };
}

async function resolveCrate(fetchImpl, spec) {
  const { missing, body } = await getJson(fetchImpl, `https://crates.io/api/v1/crates/${encodeURIComponent(spec.name)}`);
  if (missing) return { error: 'not-found', message: `${spec.name} is not on crates.io` };
  const version = body.crate?.max_stable_version ?? body.crate?.max_version;
  return { package: { ...spec, version, latest: version, deprecated: null, engines: null, peers: {} } };
}

async function resolvePypi(fetchImpl, spec) {
  const { missing, body } = await getJson(fetchImpl, `https://pypi.org/pypi/${encodeURIComponent(spec.name)}/json`);
  if (missing) return { error: 'not-found', message: `${spec.name} is not on PyPI` };
  const version = body.info?.version;
  const yanked = (body.urls ?? []).length > 0 && body.urls.every(file => file.yanked);
  return { package: { ...spec, version, latest: version, deprecated: yanked ? 'yanked' : null,
    engines: body.info?.requires_python ? { python: body.info.requires_python } : null, peers: {} } };
}

// A candidate fix version declares a peer range that accepts the chosen version; an undeclared or unparsable range is no evidence.
function acceptsPeer(version, range) {
  if (range === undefined) return false;
  try { return satisfies(version, range); } catch { return false; }
}

/** Check one install set. fetchImpl is the global fetch or a test double. */
export async function checkVersions({ specs, node, registry = DEFAULT_REGISTRY }, fetchImpl = fetch) {
  const parsed = specs.map(parseSpec);
  const resolved = await Promise.all(parsed.map(async spec => {
    try {
      const result = spec.ecosystem === 'crates' ? await resolveCrate(fetchImpl, spec)
        : spec.ecosystem === 'pypi' ? await resolvePypi(fetchImpl, spec) : await resolveNpm(fetchImpl, registry, spec);
      return { spec, ...result };
    } catch (error) {
      return { spec, error: 'fetch-failed', message: errorMessage(error) };
    }
  }));
  const report = { ok: true, node, packages: [], conflicts: [], missingPeers: [], deprecated: [], engineMismatches: [], unchecked: [], errors: [] };
  // A range this matcher cannot parse is reported as unchecked, never as a pass or a conflict.
  const safeSatisfies = (version, range, where) => {
    try { return satisfies(version, range); } catch { report.unchecked.push({ where, range }); return true; }
  };
  const npm = new Map();
  for (const entry of resolved) {
    const { ecosystem, name, requested } = entry.spec;
    if (entry.error) { report.errors.push({ spec: requested === 'latest' ? name : `${name}@${requested}`, code: entry.error, message: entry.message }); continue; }
    report.packages.push(entry.package);
    if (ecosystem === 'npm') npm.set(name, entry);
    if (entry.package.deprecated) report.deprecated.push({ name, version: entry.package.version, message: entry.package.deprecated });
    const nodeRange = entry.package.engines?.node;
    if (ecosystem === 'npm' && nodeRange && !safeSatisfies(node, nodeRange, `${name} engines.node`)) {
      report.engineMismatches.push({ name, version: entry.package.version, node, range: nodeRange });
    }
  }
  for (const [name, entry] of npm) {
    for (const [peer, range] of Object.entries(entry.package.peers)) {
      const chosen = npm.get(peer);
      if (!chosen) {
        if (!entry.optionalPeers.has(peer)) report.missingPeers.push({ package: name, peer, range });
        continue;
      }
      if (safeSatisfies(chosen.package.version, range, `${name} peer ${peer}`)) continue;
      const fixPackage = Object.keys(entry.versions).filter(version => parse(version) && !parse(version).prerelease.length &&
        acceptsPeer(chosen.package.version, entry.versions[version].peerDependencies?.[peer])).toSorted(compare).at(-1) ?? null;
      report.conflicts.push({ package: name, version: entry.package.version, peer, range, chosen: chosen.package.version,
        fixPeer: maxSatisfying(Object.keys(chosen.versions), range), fixPackage });
    }
  }
  report.ok = !report.conflicts.length && !report.deprecated.length && !report.engineMismatches.length && !report.errors.length;
  return report;
}

if (isMainModule(import.meta.url)) {
  const argv = process.argv.slice(2);
  if (argv.includes('--help') || argv.includes('-h')) { console.log(HELP); process.exit(0); }
  let options;
  try { options = parseArgs(argv); } catch (error) { fail('usage', errorMessage(error), 2); }
  const report = await checkVersions(options);
  console.log(JSON.stringify(report, null, 2));
  process.exit(report.ok ? 0 : 1);
}
