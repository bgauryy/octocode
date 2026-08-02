// Guard against a known native-extractor failure mode: on some Flow-typed `.js`
// files, oxc's default-JS parse mis-attributes control-flow statements (`if`,
// `let`, ...) as exported *function declarations*, so `extractGraphFacts`
// returns declarations whose `name` is a reserved keyword. Those can never be a
// real declaration name, so filtering them out drops the garbage without ever
// dropping a legitimate symbol. Consumers (localFindDeadCode's graph builder and
// lspGetSemantics' graph-facts documentSymbols fallback) share this filter so
// the guard stays in one place. (The underlying mis-parse is an engine-level
// Flow-parsing concern; this only stops the garbage from surfacing to agents.)
const JS_RESERVED_WORDS = new Set<string>([
  'break',
  'case',
  'catch',
  'class',
  'const',
  'continue',
  'debugger',
  'default',
  'delete',
  'do',
  'else',
  'enum',
  'export',
  'extends',
  'false',
  'finally',
  'for',
  'function',
  'if',
  'import',
  'in',
  'instanceof',
  'new',
  'null',
  'return',
  'super',
  'switch',
  'this',
  'throw',
  'true',
  'try',
  'typeof',
  'var',
  'void',
  'while',
  'with',
  'yield',
  'let',
  'static',
  'async',
  'await',
]);

/**
 * True when `name` is a usable JS identifier for a top-level declaration —
 * i.e. a real symbol name, not a reserved keyword the extractor mis-emitted.
 * Empty/undefined names are rejected too.
 */
export function isValidJsSymbolName(name: string | undefined | null): boolean {
  if (!name) return false;
  return !JS_RESERVED_WORDS.has(name);
}
