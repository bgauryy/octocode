// Agents type queries the way they think them: `callers of runQuery`, `who calls x`, `cycles a.ts`, `top pr`,
// `path from a to b`, a misspelled op or name, or the whole query in quotes. Each such input is read as one exact
// query, and the answer says how (`readAs`), so the canonical form stays the only form an agent learns. Nothing is
// read two ways: an input that fits several readings, or a misspelled name with several near matches, gets the
// candidates as re-runnable queries instead.

/** Ops a word stands for. Only words that cannot mean anything else. */
const SYNONYMS: Record<string, string> = {
  dependencies: 'deps',
  dependency: 'deps',
  uses: 'deps',
  dependants: 'dependents',
  importers: 'dependents',
  usedby: 'dependents',
  cycles: 'cycle',
  scc: 'cycle',
  paths: 'path',
  route: 'path',
  affected: 'impact',
  affects: 'impact',
  blast: 'impact',
  describe: 'about',
  explain: 'about',
  info: 'about',
  show: 'about',
  rank: 'top',
  ranking: 'top',
  caller: 'callers',
  find: 'search',
  grep: 'search',
  lookup: 'search',
  callee: 'callees',
};

/** Metric nicknames for `top`. */
const METRICS: Record<string, string> = {
  pr: 'pagerank',
  page_rank: 'pagerank',
  'page-rank': 'pagerank',
  between: 'betweenness',
  bc: 'betweenness',
  in: 'inweight',
  fanin: 'inweight',
  'fan-in': 'inweight',
  out: 'outweight',
  fanout: 'outweight',
  'fan-out': 'outweight',
  kcore: 'core',
  'k-core': 'core',
};

/** Words that only connect an op to its argument: `callers of x`, `impact for x`, `path from a to b`. */
const FILLER = new Set(['of', 'for', 'on', 'in', 'from', 'to']);
// Ops whose arguments are free text or keywords (a metric, a form): their words are never filler.
const FREE_TEXT = new Set(['search', 'trace', 'top', 'critical']);
// `path a b --avoid <dirs>` keeps its list as written.

// Damerau–Levenshtein distance (adjacent transpositions count once), capped: above `max` the exact value is moot.
export function editDistance(a: string, b: string, max = Infinity) {
  if (Math.abs(a.length - b.length) > max) return max + 1;
  // d[i][j]: distance between the first i characters of a and the first j of b.
  const d = Array.from({ length: a.length + 1 }, (_row, i) =>
    Array.from({ length: b.length + 1 }, (_cell, j) =>
      i === 0 ? j : j === 0 ? i : 0
    )
  );
  for (let i = 1; i <= a.length; i++) {
    for (let j = 1; j <= b.length; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      d[i][j] = Math.min(
        d[i - 1][j] + 1,
        d[i][j - 1] + 1,
        d[i - 1][j - 1] + cost
      );
      if (i > 1 && j > 1 && a[i - 1] === b[j - 2] && a[i - 2] === b[j - 1])
        d[i][j] = Math.min(d[i][j], d[i - 2][j - 2] + 1);
    }
  }
  return d[a.length][b.length];
}

/** How far a misspelling may be from the word it stands for: one edit for short words, two from eight letters. */
export const tolerance = (word: string) =>
  word.length < 4 ? 0 : word.length < 8 ? 1 : 2;

/** The names within tolerance of `word`, nearest first; one nearest name is the reading, several are candidates. */
export function nearest(word: string, names: Iterable<string>) {
  const max = tolerance(word);
  if (!max) return { best: [] as string[], distance: Infinity };
  const lower = word.toLowerCase();
  let best: string[] = [],
    distance = Infinity;
  for (const name of new Set(names)) {
    const d = editDistance(lower, name.toLowerCase(), max);
    if (d > max) continue;
    if (d < distance) [best, distance] = [[name], d];
    else if (d === distance) best.push(name);
  }
  return { best, distance };
}

export interface Reading {
  tokens: string[];
  /** Each change, in words: `"cycles" read as cycle`. */
  notes: string[];
  /** Several readings fit: re-runnable queries, nothing answered. */
  candidates?: string[];
}

/** One exact query for what an agent typed; `ops` are the canonical op names. */
export function readQuery(input: string[], ops: readonly string[]): Reading {
  let tokens = [...input];
  const notes: string[] = [];
  // The whole query in one quoted token (an MCP client quoting its argument).
  if (tokens.length === 1 && /\s/.test(tokens[0])) {
    tokens = tokens[0].trim().split(/\s+/);
    notes.push('the quoted query read as words');
  }
  if (!tokens.length) return { tokens, notes };
  let op = tokens[0].toLowerCase();
  // `who calls x`, `what calls x`, `who imports x`.
  if ((op === 'who' || op === 'what') && tokens.length > 2) {
    const verb = tokens[1].toLowerCase();
    const as =
      verb === 'calls'
        ? 'callers'
        : verb === 'imports' || verb === 'uses'
          ? 'dependents'
          : null;
    if (as) {
      notes.push(`"${tokens[0]} ${tokens[1]}" read as ${as}`);
      tokens = [as, ...tokens.slice(2)];
      op = as;
    }
  }
  if (!ops.includes(op)) {
    const synonym = SYNONYMS[op];
    const typo = synonym ? null : nearest(op, ops);
    const as = synonym ?? (typo?.best.length === 1 ? typo.best[0] : null);
    if (as) {
      notes.push(`"${tokens[0]}" read as ${as}`);
      tokens = [as, ...tokens.slice(1)];
      op = as;
    } else if (typo && typo.best.length > 1)
      return {
        tokens,
        notes,
        candidates: typo.best.map(o => [o, ...tokens.slice(1)].join(' ')),
      };
  }
  if (!FREE_TEXT.has(op)) {
    const args = tokens.slice(1);
    const kept = args.filter(
      (t, i) =>
        !FILLER.has(t.toLowerCase()) ||
        // A filler word that is the only argument, or ends the query, is an argument.
        args.filter(a => !FILLER.has(a.toLowerCase())).length === 0 ||
        i === args.length - 1
    );
    if (kept.length !== args.length) {
      notes.push(
        `dropped ${args
          .filter(t => !kept.includes(t))
          .map(t => `"${t}"`)
          .join(', ')}`
      );
      tokens = [tokens[0], ...kept];
    }
  }
  if (
    op === 'top' &&
    tokens[1] &&
    Object.hasOwn(METRICS, tokens[1].toLowerCase())
  ) {
    const metric = METRICS[tokens[1].toLowerCase()];
    notes.push(`"${tokens[1]}" read as ${metric}`);
    tokens = [tokens[0], metric, ...tokens.slice(2)];
  }
  if (op === 'critical' && tokens[1]) {
    const form = nearest(tokens[1], ['functions', 'points', 'path']);
    const as = ['function', 'point', 'paths'].includes(tokens[1].toLowerCase())
      ? { function: 'functions', point: 'points', paths: 'path' }[
          tokens[1].toLowerCase()
        ]
      : form.best.length === 1 && form.best[0] !== tokens[1].toLowerCase()
        ? form.best[0]
        : null;
    if (as) {
      notes.push(`"${tokens[1]}" read as ${as}`);
      tokens = [tokens[0], as, ...tokens.slice(2)];
    }
  }
  return { tokens, notes };
}
