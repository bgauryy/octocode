// Verdict rules: turn the signals of independent layers into one verdict per lead. Pure, so every rule has a test.
export const HIGH = 0.7;
export const LOW = 0.3;

// The call each layer makes on an import: usage counts from the parsed source, clasify from its two questions.
function usageVote(s) {
  if (!s || (s.ast === 'n/a' && !s.textOk)) return null;
  if (s.reexport) return 'reexport';
  if (s.usageLines === 0) return 'unused';
  return s.callLike > 0 ? 'runtime' : 'type';
}
function clasifyVote(c) {
  if (!c) return null;
  if (c.runtimeCall >= HIGH && c.typeOnly < 0.5) return 'runtime';
  if ((c.typeOnly >= HIGH && c.runtimeCall < 0.5) || (c.runtimeCall <= LOW && c.typeOnly > 0.5)) return 'type';
  return 'unsure';
}

// Edge rules in priority order: the first whose `when` holds decides. Each sees the parsed-source signals `s` and the
// two votes; `why` explains the verdict in the brief. Order matters: a missing import beats every vote, and a
// disagreement beats a partial agreement.
const EDGE_RULES = [
  {
    verdict: 'unconfirmed',
    when: ({ s }) => s?.ast === 'absent' || (s?.ast === 'n/a' && !s.textOk),
    why: ({ s }) => (s.ast === 'n/a' ? 'no AST for this file type and the source line does not name the target module' : 'the parsed import statement at this line does not name the target module'),
  },
  {
    verdict: 'unused',
    when: ({ usage }) => usage === 'unused',
    why: ({ s }) => `imported names (${s.names.join(', ')}) never appear after the import, in this file or in child modules that inherit it: stale import or an over-counted edge`,
  },
  { verdict: 'reexport', when: ({ usage }) => usage === 'reexport', why: () => 'the file re-exports the module; usage is by its own importers' },
  {
    verdict: 'conflict',
    when: ({ usage, clasify }) => usage && clasify && clasify !== 'unsure' && usage !== clasify,
    why: ({ s, usage, clasify }) => `usage counts say ${usage}-style use (${s.callLike} call-like of ${s.usageLines} lines), clasify says ${clasify}: read the file`,
  },
  {
    verdict: 'corroborated',
    when: ({ usage, clasify }) => usage === 'runtime' && clasify === 'runtime',
    why: ({ s }) => `runtime dependency: ${s.callLike} call-like uses and clasify agree`,
  },
  { verdict: 'dismissed', when: ({ usage, clasify }) => usage === 'type' && clasify === 'type', why: () => 'type, constant or callback use only: a soft dependency' },
  {
    // The parsed AST places the path in a type position and no call-like use follows: two syntax layers agree.
    verdict: 'dismissed',
    when: ({ s, usage, clasify }) => s?.typePosition && usage === 'type' && clasify !== 'runtime',
    why: ({ s }) => `the parsed AST places ${s.names.join(', ')} in a type position and usage finds no call-like use: a type-only, soft dependency`,
  },
];

const unresolvedWhy = ({ usage, clasify }) => {
  if (usage && !clasify) return `usage says ${usage}; no judging layer ran`;
  if (clasify && !usage) return `clasify says ${clasify}; no usage evidence`;
  return 'layers are unsure: read the file';
};

// s = parsed-source signals (ast, textOk, usageLines, callLike, reexport, names); c = clasify {typeOnly, runtimeCall}.
export function edgeVerdict(s, c) {
  const votes = { usage: usageVote(s), clasify: clasifyVote(c) };
  const facts = { s, ...votes };
  const rule = EDGE_RULES.find((r) => r.when(facts));
  return rule ? { votes, verdict: rule.verdict, why: rule.why(facts) } : { votes, verdict: 'unresolved', why: unresolvedWhy(facts) };
}

// c = clasify answers keyed by question; structure = {lines, owners}; detector = the graph rule that raised the lead.
export function fileVerdict(c, structure, detector) {
  const k = c && Object.keys(c)[0];
  // A generic question scores near 0.65 for almost any file; only its extremes carry information.
  const [hi, lo] = k === 'unrelated' ? [0.85, 0.15] : [HIGH, LOW];
  if (c && c[k] >= hi) return { verdict: 'corroborated', why: `graph (${detector}) and clasify agree that this file mixes duties` };
  if (c && c[k] <= lo) return { verdict: 'not-supported', why: `clasify says this file does not mix duties, against the graph (${detector}); it stays a hub` };
  if (c && k === 'unrelated' && structure.owners === 1 && structure.lines < 400) return { verdict: 'not-supported', why: `a single-owner file of ${structure.lines} lines and clasify is not convinced: the ${detector} finding measures connectivity, not mixed duties` };
  return { verdict: 'unresolved', why: c ? 'clasify is unsure: read the file' : 'no judging layer ran' };
}

// p = P(yes) for the follow-up "do all top-level items serve the one purpose the file's name suggests?", asked only
// when the first file question was unsure. Its extremes settle the lead; the middle stays unresolved (null).
export function cohesionVerdict(p, detector) {
  if (p == null) return null;
  if (p >= HIGH) return { verdict: 'not-supported', why: `a named follow-up finds every item serves the one purpose the file's name suggests: the ${detector} finding measures connectivity, not mixed duties` };
  if (p <= LOW) return { verdict: 'corroborated', why: `graph (${detector}) and a named clasify follow-up agree that the file's items do not serve one purpose` };
  return null;
}
