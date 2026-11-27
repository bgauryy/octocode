// Public registries other than npm: fetch versions with their yank, deprecation and runtime facts, pick one per request.
import { compare, parse, satisfies } from './semver.mjs';

export const ECOSYSTEMS = ['crates', 'pypi', 'go', 'maven', 'nuget', 'rubygems', 'packagist', 'pub', 'hex', 'jsr'];

// Maven's Final, RELEASE and GA qualifiers mark releases, not pre-releases.
const RELEASE_QUALIFIER = /[.-](final|release|ga)$/i;

/** A release's numeric segments, any count (Rails 8.1.4.1, NuGet 4.0.0.1); null for a pre-release or other text. */
export function segments(version) {
  const text = String(version).trim().replace(/^v(?=\d)/, '').replace(RELEASE_QUALIFIER, '');
  return /^\d+(\.\d+)*$/.test(text) ? text.split('.').map(Number) : null;
}

/** A version as SemVer text for range matching: a release keeps its first three segments; null when it is not SemVer-shaped. */
export function normalize(version) {
  const parts = segments(version);
  if (parts) return [...parts, 0, 0].slice(0, 3).join('.');
  const text = String(version).trim().replace(/^v(?=\d)/, '');
  return parse(text) ? text : null;
}

// Releases order by every segment, so 8.1.4.1 follows 8.1.4; SemVer text orders by SemVer precedence.
function order(a, b) {
  const left = segments(a), right = segments(b);
  if (left && right) {
    for (let index = 0; index < Math.max(left.length, right.length); index++) {
      const difference = (left[index] ?? 0) - (right[index] ?? 0);
      if (difference) return Math.sign(difference);
    }
    return 0;
  }
  return compare(normalize(a), normalize(b));
}

const stable = version => { const semver = normalize(version); return Boolean(semver) && !parse(semver).prerelease.length; };

// lib/semver.mjs reads partial versions itself (>=1.2, 1.2.x); only a v prefix needs removing.
const lift = text => text.replace(/(^|[^\w.])v(?=\d)/g, '$1');
// "~> 1.8" allows 1.x from 1.8; "~> 1.8.2" allows 1.8.x from 1.8.2 (RubyGems and Hex).
const pessimistic = version => {
  const parts = version.split('.').map(Number);
  const upper = parts.length < 2 ? [parts[0] + 1] : [...parts.slice(0, -2), parts.at(-2) + 1];
  return `>=${version} <${upper.join('.')}`;
};
// [1.0,2.0) style intervals (NuGet, Maven); a bare version means at least it (NuGet) or exactly it (Maven).
const interval = (text, bare) => {
  const match = /^([[(])\s*([^,\s]*)\s*(?:,\s*([^\])\s]*))?\s*([\])])$/.exec(text.trim());
  if (!match) return bare(text.trim());
  const [, open, low, high, close] = match;
  if (high === undefined) return `=${low}`;
  return [low && `${open === '[' ? '>=' : '>'}${low}`, high && `${close === ']' ? '<=' : '<'}${high}`].filter(Boolean).join(' ') || '*';
};

/** An ecosystem's version requirement as an npm-style range for lib/semver.mjs, plus PEP 440 exclusions. */
export function toRange(ecosystem, requirement) {
  const text = requirement.trim();
  const exclude = [];
  let range;
  if (ecosystem === 'crates') range = text.split(',').map(part => part.trim()).map(part => /^\d/.test(part) ? `^${part}` : part).join(' ');
  else if (ecosystem === 'pypi') {
    range = text.split(',').map(part => part.trim()).filter(Boolean).map(part => {
      const [, op, version] = /^(===|==|!=|~=|<=|>=|<|>)?\s*(.+)$/.exec(part);
      if (op === '!=') { exclude.push(version); return ''; }
      if (op === '~=') return pessimistic(version);
      if ((op === '==' || op === '===' || !op) && version.endsWith('.*')) return version.replace(/\.\*$/, '.x');
      return `${op === '==' || op === '===' ? '=' : op ?? '='}${version}`;
    }).filter(Boolean).join(' ');
  } else if (ecosystem === 'rubygems' || ecosystem === 'hex') {
    range = text.split(/\s+or\s+|\s*\|\|\s*/).map(set => set.split(/\s*,\s*|\s+and\s+/).map(part => {
      const match = /^~>\s*(.+)$/.exec(part.trim());
      return match ? pessimistic(match[1]) : part.trim().replace(/^==\s*/, '=');
    }).join(' ')).join(' || ');
  } else if (ecosystem === 'packagist') {
    range = text.replace(/@\w+/g, '').split(/\s*\|\|?\s*/).map(set => set.split(/\s*,\s*|\s+/).filter(Boolean).map(part => {
      const match = /^~\s*v?(\d+)\.(\d+)$/.exec(part);
      return match ? `>=${match[1]}.${match[2]}.0 <${Number(match[1]) + 1}.0.0` : part.replace(/^v/, '');
    }).join(' ')).join(' || ');
  } else if (ecosystem === 'nuget') range = interval(text, version => `>=${version}`);
  else if (ecosystem === 'maven') range = interval(text, version => `=${version}`);
  else range = text === 'any' ? '*' : text;
  return { range: lift(range), exclude: exclude.map(normalize) };
}

/** The candidate for a request: latest, an exact version, or the highest stable match of a range. */
export function pick(ecosystem, candidates, latest, requested) {
  if (requested === 'latest') {
    const entry = candidates.find(candidate => candidate.version === latest);
    return entry?.yanked ? { ...entry, deprecated: 'yanked' } : entry ?? null;
  }
  const usable = candidates.filter(entry => !entry.yanked);
  const exact = usable.find(entry => entry.version === requested || entry.version === `v${requested}`);
  if (exact) return exact;
  const { range, exclude } = toRange(ecosystem, requested);
  return usable.filter(entry => stable(entry.version) && !exclude.includes(normalize(entry.version)) && satisfies(normalize(entry.version), range))
    .toSorted((a, b) => order(a.version, b.version)).at(-1) ?? null;
}

const newestStable = versions => versions.filter(stable).toSorted(order).at(-1) ?? null;
// The Go proxy escapes each capital letter as ! plus its lowercase.
const goPath = module => module.replace(/[A-Z]/g, letter => `!${letter.toLowerCase()}`);
// Composer p2 files list each version as a diff of the previous one; "__unset" removes a key.
function expandComposer(entries) {
  let previous = {};
  return entries.map(entry => {
    const expanded = { ...previous, ...entry };
    for (const [key, value] of Object.entries(entry)) if (value === '__unset') delete expanded[key];
    previous = expanded;
    return expanded;
  });
}

// Each fetcher returns {candidates: [{version, yanked, deprecated, engines}], latest, detail?(entry)} or {missing:true}.
const FETCHERS = {
  async crates(get, name) {
    const { missing, body } = await get(`https://crates.io/api/v1/crates/${encodeURIComponent(name)}`);
    if (missing) return { missing };
    return { latest: body.crate?.max_stable_version ?? body.crate?.max_version,
      candidates: (body.versions ?? []).map(entry => ({ version: entry.num, yanked: entry.yanked, deprecated: null, engines: entry.rust_version ? { rust: entry.rust_version } : null })) };
  },
  async pypi(get, name) {
    const { missing, body } = await get(`https://pypi.org/pypi/${encodeURIComponent(name)}/json`);
    if (missing) return { missing };
    return { latest: body.info?.version,
      candidates: Object.entries(body.releases ?? {}).filter(([, files]) => files.length).map(([version, files]) => ({ version, yanked: files.every(file => file.yanked),
        deprecated: null, engines: files[0].requires_python ? { python: files[0].requires_python } : null })) };
  },
  async go(get, name) {
    const base = `https://proxy.golang.org/${goPath(name)}/@v`;
    const list = await get(`${base}/list`, 'text');
    if (list.missing) return { missing: true };
    const latest = await get(`https://proxy.golang.org/${goPath(name)}/@latest`);
    const versions = String(list.body).split('\n').map(line => line.trim()).filter(Boolean);
    const latestVersion = latest.body?.Version ?? newestStable(versions);
    return { latest: latestVersion, candidates: [...new Set([...versions, latestVersion].filter(Boolean))].map(version => ({ version, yanked: false, deprecated: null, engines: null })),
      async detail(entry) {
        const mod = await get(`${base}/${entry.version}.mod`, 'text');
        const text = String(mod.body ?? '');
        const deprecated = /^\s*\/\/\s*Deprecated:\s*(.+)$/m.exec(text.split(/^module\s/m)[0] + (text.match(/^module\s.*$/m)?.[0] ?? ''))?.[1] ?? null;
        const goVersion = /^go\s+(\S+)/m.exec(text)?.[1];
        return { deprecated, engines: goVersion ? { go: goVersion } : null };
      } };
  },
  async maven(get, name) {
    const [group, artifact] = name.split(':');
    if (!group || !artifact) throw new Error('a maven spec is maven:<group>:<artifact>');
    const { missing, body } = await get(`https://repo1.maven.org/maven2/${group.replaceAll('.', '/')}/${artifact}/maven-metadata.xml`, 'text');
    if (missing) return { missing };
    const versions = [...String(body).matchAll(/<version>([^<]+)<\/version>/g)].map(match => match[1]);
    return { latest: /<release>([^<]+)<\/release>/.exec(body)?.[1] ?? newestStable(versions),
      candidates: versions.map(version => ({ version, yanked: false, deprecated: null, engines: null })) };
  },
  async nuget(get, name) {
    const id = name.toLowerCase();
    const { missing, body } = await get(`https://api.nuget.org/v3-flatcontainer/${encodeURIComponent(id)}/index.json`);
    if (missing) return { missing };
    const versions = body.versions ?? [];
    return { latest: newestStable(versions), candidates: versions.map(version => ({ version, yanked: false, deprecated: null, engines: null })),
      async detail(entry) {
        const index = await get(`https://api.nuget.org/v3/registration5-gz-semver2/${encodeURIComponent(id)}/index.json`);
        for (const page of index.body?.items ?? []) {
          const items = page.items ?? (await get(page['@id'])).body?.items ?? [];
          const found = items.find(item => item.catalogEntry?.version?.toLowerCase() === entry.version.toLowerCase())?.catalogEntry;
          if (!found) continue;
          const deprecation = found.deprecation ? `deprecated: ${found.deprecation.message ?? found.deprecation.reasons?.join(', ') ?? 'see nuget.org'}` : null;
          return { deprecated: deprecation ?? (found.listed === false ? 'unlisted' : null), engines: null };
        }
        return {};
      } };
  },
  async rubygems(get, name) {
    const { missing, body } = await get(`https://rubygems.org/api/v1/versions/${encodeURIComponent(name)}.json`);
    if (missing) return { missing };
    const seen = new Map();
    for (const entry of body ?? []) if (!seen.has(entry.number) || entry.platform === 'ruby') seen.set(entry.number, entry);
    const candidates = [...seen.values()].map(entry => ({ version: entry.number, yanked: false, deprecated: null, engines: entry.ruby_version ? { ruby: entry.ruby_version } : null }));
    return { latest: [...seen.values()].find(entry => !entry.prerelease)?.number ?? null, candidates };
  },
  async packagist(get, name) {
    const { missing, body } = await get(`https://repo.packagist.org/p2/${name.toLowerCase()}.json`);
    if (missing) return { missing };
    const entries = expandComposer(body.packages?.[name.toLowerCase()] ?? body.packages?.[name] ?? []);
    const candidates = entries.map(entry => ({ version: entry.version, yanked: false,
      deprecated: entry.abandoned ? `abandoned${typeof entry.abandoned === 'string' ? `; use ${entry.abandoned}` : ''}` : null,
      engines: entry.require?.php ? { php: entry.require.php } : null }));
    return { latest: newestStable(entries.map(entry => entry.version)) ?? candidates[0]?.version, candidates };
  },
  async pub(get, name) {
    const { missing, body } = await get(`https://pub.dev/api/packages/${encodeURIComponent(name)}`);
    if (missing) return { missing };
    const options = await get(`https://pub.dev/api/packages/${encodeURIComponent(name)}/options`);
    const discontinued = options.body?.isDiscontinued ? `discontinued${options.body.replacedBy ? `; use ${options.body.replacedBy}` : ''}` : null;
    return { latest: body.latest?.version, candidates: (body.versions ?? []).map(entry => ({ version: entry.version, yanked: Boolean(entry.retracted), deprecated: discontinued,
      engines: entry.pubspec?.environment?.sdk ? { dart: entry.pubspec.environment.sdk } : null })) };
  },
  async hex(get, name) {
    const { missing, body } = await get(`https://hex.pm/api/packages/${encodeURIComponent(name)}`);
    if (missing) return { missing };
    const retired = body.retirements ?? {};
    return { latest: body.latest_stable_version ?? body.latest_version, candidates: (body.releases ?? []).map(entry => ({ version: entry.version, yanked: false,
      deprecated: retired[entry.version] ? `retired: ${[retired[entry.version].reason, retired[entry.version].message].filter(Boolean).join(', ')}` : null, engines: null })) };
  },
  async jsr(get, name) {
    if (!/^@[\w-]+\/[\w-]+$/.test(name)) throw new Error('a jsr spec is jsr:@<scope>/<name>');
    const { missing, body } = await get(`https://jsr.io/${name}/meta.json`);
    if (missing) return { missing };
    return { latest: body.latest, candidates: Object.entries(body.versions ?? {}).map(([version, meta]) => ({ version, yanked: Boolean(meta.yanked), deprecated: null, engines: null })) };
  },
};

/** The native pin for a resolved version, written where that ecosystem adds dependencies. */
export function pinFor(ecosystem, name, version) {
  return { crates: `${name}@${version}`, pypi: `${name}==${version}`, go: `${name}@${version}`, maven: `${name}:${version}`, nuget: `${name}@${version}`,
    rubygems: `gem "${name}", "${version}"`, packagist: `${name}:${version.replace(/^v(?=\d)/, "")}`, pub: `${name}:${version}`, hex: `{:${name}, "${version}"}`, jsr: `jsr:${name}@${version}` }[ecosystem];
}

/** Resolve one non-npm spec: {package} or {error, message}. get(url, 'json'|'text') returns {missing} or {body}. */
export async function resolveRegistry(get, spec) {
  const fetched = await FETCHERS[spec.ecosystem](get, spec.name);
  if (fetched.missing) return { error: 'not-found', message: `${spec.name} is not on the ${spec.ecosystem} registry` };
  const chosen = pick(spec.ecosystem, fetched.candidates, fetched.latest, spec.requested);
  if (!chosen) return { error: 'no-match', message: `no ${spec.name} version matches ${spec.requested}` };
  const detail = fetched.detail ? await fetched.detail(chosen) : {};
  return { package: { ...spec, version: chosen.version, latest: fetched.latest ?? null, deprecated: detail.deprecated ?? chosen.deprecated ?? null,
    engines: detail.engines ?? chosen.engines ?? null, peers: {}, pin: pinFor(spec.ecosystem, spec.name, chosen.version) } };
}
