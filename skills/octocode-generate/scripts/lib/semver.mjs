// SemVer 2 parsing, ordering and npm range matching (node-semver range syntax, strict mode, no includePrerelease).
const VERSION = /^v?(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$/;
const PARTIAL = /^v?(\d+|[xX*])(?:\.(\d+|[xX*]))?(?:\.(\d+|[xX*]))?(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$/;

/** {major, minor, patch, prerelease[]} or null for an invalid version. */
export function parse(version) {
  const match = VERSION.exec(String(version).trim());
  if (!match) return null;
  return { major: Number(match[1]), minor: Number(match[2]), patch: Number(match[3]), prerelease: match[4] ? match[4].split('.') : [] };
}

function comparePrerelease(a, b) {
  if (!a.length || !b.length) return Math.sign(b.length - a.length);
  for (let index = 0; index < Math.max(a.length, b.length); index++) {
    const x = a[index], y = b[index];
    if (x === undefined) return -1;
    if (y === undefined) return 1;
    const xNumeric = /^\d+$/.test(x), yNumeric = /^\d+$/.test(y);
    if (xNumeric && yNumeric && Number(x) !== Number(y)) return Math.sign(Number(x) - Number(y));
    if (xNumeric !== yNumeric) return xNumeric ? -1 : 1;
    if (!xNumeric && x !== y) return x < y ? -1 : 1;
  }
  return 0;
}

const asVersion = value => typeof value === 'string' ? parse(value) : value;

/** -1, 0 or 1; throws on an invalid version. */
export function compare(left, right) {
  const a = asVersion(left), b = asVersion(right);
  if (!a || !b) throw new Error(`invalid version: ${!a ? left : right}`);
  return Math.sign(a.major - b.major) || Math.sign(a.minor - b.minor) || Math.sign(a.patch - b.patch) ||
    comparePrerelease(a.prerelease, b.prerelease);
}

const wild = part => part === undefined || /^[xX*]$/.test(part);
const at = (major, minor, patch, prerelease = []) => ({ major, minor, patch, prerelease });
const floor = (major, minor, patch) => at(major, minor, patch, ['0']);

// One partial version with its operator → [op, version] comparators; [] means any version.
function desugar(op, text) {
  const match = PARTIAL.exec(text);
  if (!match) throw new Error(`invalid range part: ${op}${text}`);
  const [, ma, mi, pa, pre] = match;
  const major = wild(ma) ? null : Number(ma);
  const minor = major === null || wild(mi) ? null : Number(mi);
  const patch = minor === null || wild(pa) ? null : Number(pa);
  const exact = patch !== null ? at(major, minor, patch, pre ? pre.split('.') : []) : null;
  if (major === null) return op === '<' || op === '>' ? [['<', floor(0, 0, 0)]] : [];
  const low = at(major, minor ?? 0, patch ?? 0, exact?.prerelease ?? []);
  const nextMinor = floor(major, (minor ?? 0) + 1, 0), nextMajor = floor(major + 1, 0, 0);
  const partialUpper = minor === null ? nextMajor : nextMinor;
  switch (op) {
    case '^': {
      if (major > 0 || minor === null) return [['>=', low], ['<', nextMajor]];
      if (minor > 0 || patch === null) return [['>=', low], ['<', nextMinor]];
      return [['>=', low], ['<', floor(0, 0, patch + 1)]];
    }
    case '~': return [['>=', low], ['<', partialUpper]];
    case '>': return exact ? [['>', exact]] : [['>=', minor === null ? at(major + 1, 0, 0) : at(major, minor + 1, 0)]];
    case '>=': return [['>=', low]];
    case '<': return [['<', exact ?? floor(major, minor ?? 0, 0)]];
    case '<=': return exact ? [['<=', exact]] : [['<', partialUpper]];
    default: return exact ? [['=', exact]] : [['>=', low], ['<', partialUpper]];
  }
}

function hyphen(lowText, highText) {
  const high = PARTIAL.exec(highText);
  if (!high) throw new Error(`invalid range part: ${highText}`);
  const upper = wild(high[2]) || wild(high[3]) ? desugar('<=', highText) : [['<=', parse(highText)]];
  return [...desugar('>=', lowText), ...upper];
}

/** A range as OR-sets of AND-comparators; throws on invalid syntax. */
export function parseRange(range) {
  return String(range).split('||').map(set => {
    const text = set.trim().replace(/(<=|>=|<|>|=|~>?|\^)\s+/g, '$1');
    if (text === '' || text === '*' || text === 'x' || text === 'X') return [];
    const span = /^(\S+)\s+-\s+(\S+)$/.exec(text);
    if (span) return hyphen(span[1], span[2]);
    return text.split(/\s+/).flatMap(token => {
      const [, op = '', version] = /^(<=|>=|<|>|=|~>?|\^)?(.*)$/.exec(token);
      return desugar(op.startsWith('~') ? '~' : op, version);
    });
  });
}

const passes = (version, [op, bound]) => {
  const order = compare(version, bound);
  return op === '=' ? order === 0 : op === '>' ? order > 0 : op === '>=' ? order >= 0 : op === '<' ? order < 0 : order <= 0;
};

/** Whether version satisfies range. A prerelease matches only a set naming a prerelease of the same major.minor.patch. */
export function satisfies(version, range) {
  const target = asVersion(version);
  if (!target) return false;
  return parseRange(range).some(set => set.every(comparator => passes(target, comparator)) &&
    (!target.prerelease.length || set.some(([, bound]) => bound.prerelease.length &&
      bound.major === target.major && bound.minor === target.minor && bound.patch === target.patch)));
}

/** The highest version in versions that satisfies range, or null. */
export function maxSatisfying(versions, range) {
  return versions.filter(version => parse(version) && satisfies(version, range)).toSorted(compare).at(-1) ?? null;
}
