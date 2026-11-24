// loads.ts mirrors the engine's edge-kind table (packages/octocode-native/crates/tool-graph/src/ast_graph/edges.rs
// `KINDS`) for saved graphs. Every import kind any grammar's linker emits must load the same way on both sides, so a
// kind the engine adds or reclassifies fails here instead of reading as `unknown` in every walk of one language.
import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { loadOfKind } from '../src/loads.ts';

const EDGES = new URL('../../octocode-native/crates/tool-graph/src/ast_graph/edges.rs', import.meta.url);
const LOADS = { Eager: 'eager', Deferred: 'deferred', TypeOnly: 'type', CompileTime: 'compile', Unknown: 'unknown', Data: 'data' };

function engineKinds() {
  const source = readFileSync(EDGES, 'utf8');
  const constants = Object.fromEntries([...source.matchAll(/const (\w+): &str = "([^"]+)";/g)].map((m) => [m[1], m[2]]));
  const table = source.slice(source.indexOf('const KINDS'), source.indexOf('];', source.indexOf('const KINDS')));
  return [...table.matchAll(/\((?:"([^"]+)"|(\w+)),\s*Load::(\w+)\)/g)].map(([, literal, name, load]) => ({
    kind: literal ?? constants[name],
    load,
  }));
}

test('every engine import kind loads as the engine says it does', () => {
  const kinds = engineKinds();
  assert.ok(kinds.length > 20, `read ${kinds.length} kinds from edges.rs`);
  for (const { kind, load } of kinds) {
    assert.ok(kind, `an unresolved constant in edges.rs KINDS (${load})`);
    // A module tree edge (`mod child;`) is containment, never an import row.
    if (load === 'ModuleTree') continue;
    assert.equal(loadOfKind(kind), LOADS[load], `${kind}: engine Load::${load}`);
  }
});
