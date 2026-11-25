#!/usr/bin/env node
// Resolve planned dependency versions from public registries and report deprecations, engine misfits and peer conflicts.
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fail, errorMessage } from './lib/fail.mjs';
import { isMainModule } from './lib/entrypoint.mjs';
import { maxSatisfying, satisfies, parse, compare } from './lib/semver.mjs';
import { ECOSYSTEMS, resolveRegistry } from './lib/registries.mjs';

const HELP = `Usage: node <skill-folder>/scripts/check-versions.mjs <spec>... [--project <dir>] [--node <version>] [--registry <url>]

Resolves each planned dependency before it is written into a plan or installed. A spec is
  <name>[@<range|tag>]                npm package at the highest version matching the range, or that dist-tag
  <ecosystem>:<name>[@<requirement>]  another registry; without a requirement, its latest stable release
The requirement uses that ecosystem's own syntax:
  crates:serde@1.0          crates.io, Cargo requirements (bare = caret; comma = and)
  pypi:fastapi@>=0.115,<1   PyPI, PEP 440 specifiers (==, !=, ~=, >=, <, ==1.2.*); pypi:fastapi>=0.115 also works
  go:github.com/spf13/cobra@v1.10  Go module proxy; reads the chosen version's go.mod for Deprecated and go
  maven:org.springframework.boot:spring-boot@4.2.0  Maven Central, exact version or [1.0,2.0) interval
  nuget:Newtonsoft.Json@[13.0,14.0)  NuGet; bare version = at least it; reads deprecation and unlisted state
  rubygems:rails@~> 8.1     RubyGems, pessimistic ~> and comparators
  packagist:laravel/framework@^13.0  Packagist (Composer); reports abandoned packages
  pub:http@^1.2             pub.dev (Dart, Flutter); reports discontinued packages and retracted versions
  hex:phoenix@~> 1.8        Hex (Elixir); reports retired releases
  jsr:@std/path@^1          JSR (Deno)
Each package row has a pin: the exact form its tool adds (cargo add, uv add, go get, a Gradle or Maven
coordinate, dotnet package add, a Gemfile line, composer require, dart pub add, a mix.exs dependency,
deno add). Swift packages and GitHub Actions resolve by tag: use Octocode ghStructure refs.
npm checks treat the specs as one install set: each chosen version's peerDependencies are matched
against the versions chosen for the other specs, and engines.node against --node. Peers of transitive
dependencies are left to the package manager: its dry run (npm install --dry-run) is the final check.

Options:
  --project <dir>    Also check the dependencies and devDependencies that <dir>/package.json already declares,
                     at their declared ranges (a scaffolded project's pins are part of the install set); a spec
                     on the command line replaces the declared range. workspace:, file:, git and alias ranges
                     are listed under unchecked
  --node <version>   Node.js version the project targets (default: the running Node.js)
  --registry <url>   npm registry base URL (default: https://registry.npmjs.org)
  --help             Show this help

Output: one JSON object on stdout.
  ok             true when conflicts, deprecated, engineMismatches and errors are all empty
  node           the Node.js version engines were checked against
  packages       [{ecosystem, name, requested, version, latest, deprecated, engines, peers, pin, declared}]:
                 engines are runtime requirements (node, python, rust, go, ruby, php, dart); declared marks a
                 range read from --project
  install        exact npm specs (name@version) for the command-line specs: install these, not bare names
  conflicts      [{package, version, peer, range, chosen, fixPeer, fixPackage}]: a chosen version
                 outside a peer range; fixPeer is the highest peer version the range accepts,
                 fixPackage the highest version of the package whose peer range accepts the chosen
                 peer (null when none exists)
  missingPeers   [{package, peer, range}]: required peers not in the spec list. They leave ok true because npm,
                 pnpm and Bun install required peers themselves; with Yarn, add them to the install
  deprecated     [{name, version, message}]
  engineMismatches [{name, version, node, range}]
  unchecked      [{where, range}]: registry ranges this checker cannot parse; verify them by hand
  errors         [{spec, code, message}]: not-found, no-match or fetch-failed
Effects: HTTPS GET requests to the public registries named by the specs; writes nothing.
Exit codes: 0 ok; 1 findings or lookup errors; 2 invalid input, as {"ok":false,"error":{code,message}}.`;

const DEFAULT_REGISTRY = 'https://registry.npmjs.org';
const USER_AGENT = 'octocode-generate-skill (https://github.com/bgauryy/octocode)';

/** Split argv into {specs, node, registry}; throws a usage error message. */
export function parseArgs(argv) {
  const options = { specs: [], node: process.versions.node, registry: DEFAULT_REGISTRY, project: null };
  for (let index = 0; index < argv.length; index++) {
    const arg = argv[index];
    if (arg === '--project') {
      const value = argv[++index];
      if (!value) throw new Error('--project needs a value');
      options.project = value;
    } else if (arg === '--node' || arg === '--registry') {
      const value = argv[++index];
      if (!value) throw new Error(`${arg} needs a value`);
      if (arg === '--node' && !parse(value.replace(/^v/, ''))) throw new Error(`--node needs a full version such as 26.13.1, got ${value}`);
      options[arg.slice(2)] = arg === '--node' ? value.replace(/^v/, '') : value.replace(/\/+$/, '');
    } else if (arg.startsWith('--')) throw new Error(`unknown option ${arg}`);
    else options.specs.push(arg);
  }
  if (!options.specs.length && !options.project) throw new Error('give at least one package spec or --project');
  return options;
}

// npm ranges this checker resolves from the registry; protocols and aliases are installed some other way.
const REGISTRY_RANGE = /^(?!workspace:|file:|link:|portal:|npm:|git|github:|https?:|catalog:)\S/;

/** Declared dependency specs from dir/package.json that the command line does not already name. */
export function projectSpecs(dir, named) {
  const pkg = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8'));
  const declared = { ...pkg.dependencies, ...pkg.devDependencies };
  const specs = [], unchecked = [];
  for (const [name, range] of Object.entries(declared)) {
    if (named.has(name)) continue;
    if (REGISTRY_RANGE.test(String(range))) specs.push(`${name}@${range}`);
    else unchecked.push({ where: `${name} in package.json`, range });
  }
  return { specs, unchecked };
}

/** {ecosystem, name, requested} from one spec; scoped npm names keep their leading @. */
export function parseSpec(spec) {
  const prefixed = new RegExp(`^(${ECOSYSTEMS.join('|')}):(.+)$`).exec(spec);
  const [ecosystem, rest] = prefixed ? [prefixed[1], prefixed[2]] : ['npm', spec];
  const at = rest.indexOf('@', rest.startsWith('@') ? 1 : 0);
  if (at > 0) return { ecosystem, name: rest.slice(0, at), requested: rest.slice(at + 1).trim() || 'latest' };
  // Python requirements are often written name>=1.0 rather than name@>=1.0.
  const pep = ecosystem === 'pypi' ? /^([A-Za-z0-9._-]+)\s*([<>=!~].*)$/.exec(rest) : null;
  return pep ? { ecosystem, name: pep[1], requested: pep[2].trim() } : { ecosystem, name: rest, requested: 'latest' };
}

async function getJson(fetchImpl, url, headers = {}, as = 'json') {
  const response = await fetchImpl(url, { headers: { 'user-agent': USER_AGENT, ...headers } });
  if (response.status === 404 || response.status === 410) return { missing: true };
  if (!response.ok) throw new Error(`HTTP ${response.status} from ${url}`);
  return { body: as === 'text' ? await response.text() : await response.json() };
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
      engines: manifest.engines ?? null, peers: manifest.peerDependencies ?? {}, pin: `${spec.name}@${version}` },
    // Optional peers, and peers the package also installs as a dependency, need no separate install.
    optionalPeers: new Set([...Object.entries(manifest.peerDependenciesMeta ?? {}).filter(([, meta]) => meta?.optional).map(([name]) => name),
      ...Object.keys(manifest.dependencies ?? {})]),
    versions: body.versions,
  };
}

// A candidate fix version declares a peer range that accepts the chosen version; an undeclared or unparsable range is no evidence.
function acceptsPeer(version, range) {
  if (range === undefined) return false;
  try { return satisfies(version, range); } catch { return false; }
}

/** Check one install set. fetchImpl is the global fetch or a test double. */
export async function checkVersions({ specs, node, registry = DEFAULT_REGISTRY, project = null }, fetchImpl = fetch) {
  const named = new Set(specs.map(spec => parseSpec(spec).name));
  const fromProject = project ? projectSpecs(project, named) : { specs: [], unchecked: [] };
  const parsed = [...specs.map(parseSpec), ...fromProject.specs.map(spec => ({ ...parseSpec(spec), declared: true }))];
  const resolved = await Promise.all(parsed.map(async spec => {
    try {
      const get = (url, as = 'json') => getJson(fetchImpl, url, {}, as);
      const result = spec.ecosystem === 'npm' ? await resolveNpm(fetchImpl, registry, spec) : await resolveRegistry(get, spec);
      return { spec, ...result };
    } catch (error) {
      return { spec, error: 'fetch-failed', message: errorMessage(error) };
    }
  }));
  const report = { ok: true, node, packages: [], install: [], conflicts: [], missingPeers: [], deprecated: [], engineMismatches: [], unchecked: [...fromProject.unchecked], errors: [] };
  // A range this matcher cannot parse is reported as unchecked, never as a pass or a conflict.
  const safeSatisfies = (version, range, where) => {
    try { return satisfies(version, range); } catch { report.unchecked.push({ where, range }); return true; }
  };
  const npm = new Map();
  for (const entry of resolved) {
    const { ecosystem, name, requested } = entry.spec;
    if (entry.error) { report.errors.push({ spec: requested === 'latest' ? name : `${name}@${requested}`, code: entry.error, message: entry.message }); continue; }
    report.packages.push(entry.spec.declared ? { ...entry.package, declared: true } : entry.package);
    if (ecosystem === 'npm') npm.set(name, entry);
    if (ecosystem === 'npm' && !entry.spec.declared) report.install.push(`${name}@${entry.package.version}`);
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
