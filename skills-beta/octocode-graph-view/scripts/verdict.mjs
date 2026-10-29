// Verdict rules: turn the signals of independent layers into one verdict per lead. Pure, so every rule has a test.
export const HIGH = 0.7;
export const LOW = 0.3;

// s = parsed-source signals (ast, textOk, usageLines, callLike, reexport, names); c = clasify {typeOnly, runtimeCall}.
export function edgeVerdict(s, c) {
  const usage = !s || (s.ast === 'n/a' && !s.textOk) ? null : s.reexport ? 'reexport' : s.usageLines === 0 ? 'unused' : s.callLike > 0 ? 'runtime' : 'type';
  const clasify = !c ? null : c.runtimeCall >= HIGH && c.typeOnly < 0.5 ? 'runtime' : (c.typeOnly >= HIGH && c.runtimeCall < 0.5) || (c.runtimeCall <= LOW && c.typeOnly > 0.5) ? 'type' : 'unsure';
  const votes = { usage, clasify };
  const done = (verdict, why) => ({ votes, verdict, why });
  if (s?.ast === 'absent' || (s && s.ast === 'n/a' && !s.textOk)) return done('unconfirmed', s.ast === 'n/a' ? 'no AST for this file type and the source line does not name the target module' : 'the parsed import statement at this line does not name the target module');
  if (usage === 'unused') return done('unused', `imported names (${s.names.join(', ')}) never appear after the import, in this file or in child modules that inherit it: stale import or an over-counted edge`);
  if (usage === 'reexport') return done('reexport', 'the file re-exports the module; usage is by its own importers');
  if (usage && clasify && clasify !== 'unsure' && usage !== clasify) return done('conflict', `usage counts say ${usage}-style use (${s.callLike} call-like of ${s.usageLines} lines), clasify says ${clasify}: read the file`);
  if (usage === 'runtime' && clasify === 'runtime') return done('corroborated', `runtime dependency: ${s.callLike} call-like uses and clasify agree`);
  if (usage === 'type' && clasify === 'type') return done('dismissed', 'type, constant or callback use only: a soft dependency');
  return done('unresolved', usage && !clasify ? `usage says ${usage}; no judging layer ran` : clasify && !usage ? `clasify says ${clasify}; no usage evidence` : 'layers are unsure: read the file');
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
