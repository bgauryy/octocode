// The one Octocode protocol, check A5: what `schema <tool> --select operation=X[,field=value]`
// serves is never larger than the full query view, and no `$defs` entry is a reference to
// another one. Wave 1 emptied both allowlists, so each check now asserts no deviation. The
// other schema checks (A1-A4, A6) run in core: src/__tests__/protocolAlignment.test.ts.
import { describe, expect, it } from 'vitest';
import { at, tools, type Json } from './cli-presentation-fixture.ts';

const size = (value: unknown): number => JSON.stringify(value).length;
const isJson = (value: unknown): value is Json =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

/** One served schema view: `plain`, or `selection` when a classification provider adds fields. */
interface Served {
  tool: string;
  form: 'plain' | 'selection';
  /** `schema <tool>`: the query view as the CLI serves it, without the MCP envelope. */
  full: Json;
  selections: Json;
}

const SERVED: Served[] = tools.flatMap(entry =>
  (['plain', 'selection'] as const).flatMap(form => {
    const source = form === 'plain' ? entry : entry.selection;
    if (!isJson(source)) return [];
    const { inputSchema: _inputSchema, ...full } = at(source, 'views', 'query');
    return [{ tool: String(entry.name), form, full, selections: at(source, 'selections') }];
  })
);

/** Whether a view defines a schema that refers to itself: a grammar the listed row states as a note. */
function keepsRecursiveGrammar(view: unknown): boolean {
  const schema = isJson(view) && isJson(view.querySchema) ? view.querySchema : {};
  const defs = isJson(schema.$defs) ? schema.$defs : {};
  const refs = (name: string): string[] =>
    [...JSON.stringify(defs[name] ?? {}).matchAll(/"#\/\$defs\/([^"]+)"/g)].map(match => match[1] ?? '');
  return Object.keys(defs).some(start => {
    const seen = new Set<string>();
    const queue = refs(start);
    for (let next = queue.pop(); next !== undefined; next = queue.pop()) {
      if (next === start) return true;
      if (!seen.has(next)) {
        seen.add(next);
        queue.push(...refs(next));
      }
    }
    return false;
  });
}

function largerThanFull(): string[] {
  return SERVED.flatMap(({ tool, form, full, selections }) =>
    Object.entries(selections)
      .filter(([, selected]) => size(selected) > size(full) && !keepsRecursiveGrammar(selected))
      .map(([name]) => `${tool} ${form} ${name}`)
  );
}

/** `$defs` entries that only point at another definition (a description does not make one a schema). */
function bareReferences(view: Json): string[] {
  const schema = isJson(view.querySchema) ? view.querySchema : {};
  const defs = isJson(schema.$defs) ? schema.$defs : {};
  return Object.entries(defs)
    .filter(([, def]) => isJson(def) && '$ref' in def && Object.keys(def).every(key => key === '$ref' || key === 'description'))
    .map(([name]) => name);
}

/** Views with such a definition; generated `$defs` names are not stable, so a view is listed once. */
function viewsWithBareReferences(): string[] {
  return SERVED.flatMap(({ tool, form, full, selections }) =>
    [['query', full] as const, ...Object.entries(selections)]
      .filter(([, view]) => isJson(view) && bareReferences(view).length > 0)
      .map(([name]) => `${tool} ${form} ${name}`)
  );
}

describe('one protocol: the selected-operation schema', () => {
  it('measures every tool, and every operation a tool selects by', () => {
    expect(SERVED.filter(served => served.form === 'plain').map(served => served.tool)).toEqual(tools.map(entry => entry.name));
    for (const entry of tools) {
      const operations = (Array.isArray(entry.variantNames) ? entry.variantNames : []).map(name => `operation=${name}`);
      const selections = Object.keys(at(entry, 'selections'));
      // `operation=X`, and `operation=X,field=value` where a second field tells that operation's forms apart.
      expect(selections.filter(key => !key.includes(',')).sort()).toEqual(operations.toSorted());
      expect(selections.filter(key => !operations.includes(key.split(',')[0] ?? ''))).toEqual([]);
    }
  });

  it('A5 is never larger than the full query schema', () => {
    expect(largerThanFull()).toEqual([]);
  });

  it('A5 hoists no $def that is a bare $ref', () => {
    expect(viewsWithBareReferences()).toEqual([]);
  });
});
