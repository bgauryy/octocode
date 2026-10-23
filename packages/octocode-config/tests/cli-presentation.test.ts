import { describe, expect, it } from 'vitest';
import { INSTRUCTION_SECTIONS } from '@octocodeai/octocode-core/mcp';
import {
  DIRECT_TOOL_DEFINITIONS,
  CLASIFY_SOURCE_TOOL_NAMES,
  getDirectToolDefinitionsWithAddons,
  getPublicToolCatalogWithAddons,
  getPublicToolCatalog,
  operationSchemas,
  publishedInputSchema,
  publishedJson,
} from '@octocodeai/octocode-core/schema';
import { type Json, presentation, tool, tools } from './cli-presentation-fixture.ts';

const definition = (name: string) =>
  getDirectToolDefinitionsWithAddons().find(entry => entry.name === name)!;

describe('CLI presentation embedded by the native binary', () => {
  it('embeds core instruction sections for the native renderer', () => {
    // Native renders these for its enabled tools (crates/cli schema.rs tests);
    // MCP renders the same sections in core.
    expect(presentation.instructionSections).toEqual(INSTRUCTION_SECTIONS);
  });

  it('describes every catalog tool under the enforcement fingerprint', () => {
    expect(presentation.fingerprint).toBe(getPublicToolCatalog().fingerprint);
    expect(tools.map(entry => entry.name)).toEqual(
      DIRECT_TOOL_DEFINITIONS.map(entry => entry.name)
    );
  });

  it('serves the row MCP lists as the query view', () => {
    const views = tool('lspSearch').views as Json;
    expect((views.query as Json).querySchema).toEqual(
      publishedInputSchema(definition('lspSearch').inputSchema, 'row')
    );
    expect((views.full as Json).usage).toEqual([
      `octocode lspSearch '{"queries":[ … ]}'`,
    ]);
  });

  it('emits core clasify views for every exposed set of source readers', () => {
    const sourceViews = tool('clasify').sourceViews as Record<string, Json>;
    expect(Object.keys(sourceViews).sort()).toEqual(
      [
        '',
        'localFetch',
        'ghGetFileContent',
        'localFetch,ghGetFileContent',
      ].sort()
    );
    for (const [key, entry] of Object.entries(sourceViews)) {
      const availableTools = key ? key.split(',') : [];
      const views = entry.views as Json;
      const expected = getPublicToolCatalogWithAddons({
        availableTools,
      }).tools.find(t => t.name === 'clasify')!.querySchema as Json;
      const { $schema: _schema, ...canonical } = expected;
      expect((views.full as Json).querySchema, key).toEqual(canonical);
      expect((views.query as Json).querySchema, key).toEqual(
        publishedInputSchema(
          getDirectToolDefinitionsWithAddons({ availableTools }).find(
            t => t.name === 'clasify'
          )!.inputSchema,
          'row'
        )
      );
    }
    // State only: no reader, so no source field at all.
    const stateOnly = sourceViews['']!.views as Json;
    for (const view of ['query', 'full'])
      expect(
        JSON.stringify((stateOnly[view] as Json).querySchema),
        view
      ).not.toContain('"source"');
    expect(tool('clasify').help).not.toContain('"source":');
    expect(presentation.clasifySourceTools).toEqual([
      ...CLASIFY_SOURCE_TOOL_NAMES,
    ]);
  });

  it('lists the envelope fields MCP publishes beside queries', () => {
    for (const definition of DIRECT_TOOL_DEFINITIONS) {
      const { queries: _queries, ...published } = (publishedInputSchema(
        definition.inputSchema
      ).properties ?? {}) as Json;
      const views = tool(definition.name).views as Record<string, Json>;
      for (const name of ['query', 'full'])
        expect(
          views[name]!.envelopeFields ?? {},
          `${definition.name} ${name}`
        ).toEqual(published);
    }
    expect(
      ((tool('localSearch').views as Json).query as Json).envelopeFields
    ).toHaveProperty('responseLength');
  });

  it('lists the continuation fields the query view hides, one line each', () => {
    const fields = (name: string) =>
      ((tool(name).views as Json).query as Json).continuationFields as Record<
        string,
        string
      >;
    expect(Object.keys(fields('localSearch')).sort()).toEqual([
      'extras',
      'includeHidden',
      'matchPage',
      'page',
      'snapshot',
    ]);
    expect(Object.keys(fields('localFetch')).sort()).toEqual([
      'extras',
      'offset',
      'snapshot',
    ]);
    for (const entry of tools) {
      const query = (entry.views as Json).query as Json;
      const shown = Object.keys(
        ((query.querySchema as Json).properties ?? {}) as Json
      );
      for (const [field, line] of Object.entries(fields(String(entry.name)))) {
        expect(
          shown,
          `${entry.name}.${field} is hidden from the row`
        ).not.toContain(field);
        expect(line, `${entry.name}.${field}`).not.toMatch(/\n/);
        expect(line.length, `${entry.name}.${field}`).toBeLessThanOrEqual(160);
      }
    }
  });

  it('generates optional selection fields only in the enabled read-tool presentation', () => {
    const supported = [
      'localFetch',
      'localSearch',
      'ghGetFileContent',
      'ghSearchCode',
    ];
    const branches = (schema: Json): Json[] =>
      ((schema.oneOf ?? schema.anyOf) as Json[] | undefined) ?? [schema];
    const hasField = (schema: Json, field: string): boolean =>
      Object.hasOwn((schema.properties ?? {}) as Json, field);
    for (const name of DIRECT_TOOL_DEFINITIONS.map(entry => entry.name).filter(
      name => name !== 'clasify'
    )) {
      const plain = tool(name);
      expect(JSON.stringify({ ...plain, selection: undefined })).not.toMatch(
        /"clasifyContext"|"goals"|"selection"/
      );
      const ranked = plain.selection as Json | undefined;
      if (!supported.includes(name)) {
        expect(ranked, `${name} has no selection projection`).toBeUndefined();
        continue;
      }
      expect(ranked, `${name} has a selection projection`).toBeDefined();
      for (const view of ['query', 'full']) {
        const schema = ((ranked!.views as Json)[view] as Json)
          .querySchema as Json;
        for (const branch of branches(schema)) {
          const operation = ((branch.properties ?? {}) as Json).operation as
            Json | undefined;
          expect(
            hasField(branch, 'clasifyContext'),
            `${name} ${view} ${String(operation?.const ?? 'query')} clasifyContext`
          ).toBe(true);
          expect(
            hasField(branch, 'selection'),
            `${name} ${view} ${String(operation?.const ?? 'query')} selection`
          ).toBe(view === 'full');
          expect((branch.required ?? []) as string[]).not.toContain(
            'clasifyContext'
          );
        }
      }
      expect(
        ((ranked!.views as Json).query as Json).continuationFields
      ).toHaveProperty('selection');
    }
    expect(
      tools
        .filter(entry => entry.selection)
        .map(entry => entry.name)
        .sort()
    ).toEqual([...supported].sort());
    const history = tool('ghSearchHistory');
    const selections = history.selections as Record<string, Json>;
    for (const operation of ['pullRequest', 'issue', 'commit']) {
      const selection = selections[`operation=${operation}`];
      expect(selection, operation).toBeDefined();
      const schema = selection!.querySchema as Json;
      expect(hasField(schema, 'clasifyContext'), operation).toBe(false);
      expect(hasField(schema, 'selection'), operation).toBe(false);
    }
  });

  it('lists in each operation view only the fields that operation takes', () => {
    const selected = (name: string, operation: string) =>
      (
        (tool(name).selections as Record<string, Json>)[
          `operation=${operation}`
        ]!.querySchema as Json
      ).properties as Json;
    const hover = selected('lspSearch', 'hover');
    for (const field of [
      'graphDepth',
      'groupByFile',
      'includeDeclaration',
      'contextLines',
    ])
      expect(hover, `hover.${field}`).not.toHaveProperty(field);
    expect((hover.operation as Json).enum).toEqual(['hover']);
    expect(selected('lspSearch', 'callers')).toHaveProperty('graphDepth');
    expect(selected('lspSearch', 'callers')).not.toHaveProperty('groupByFile');
    const languages = selected('ghStructure', 'languages');
    for (const field of ['path', 'ref', 'page', 'maxDepth'])
      expect(languages, `languages.${field}`).not.toHaveProperty(field);
    const hidden = ((tool('lspSearch').views as Json).query as Json)
      .operationFields as Record<string, string[]>;
    expect(hidden.hover).not.toContain('graphDepth');
  });

  it('names the fields the query view hides and the call that shows them', () => {
    const query = (name: string) => (tool(name).views as Json).query as Json;
    const lsp = query('lspSearch');
    const byOperation = lsp.operationFields as Record<string, string[]>;
    expect(byOperation.definition).toContain('rustContext');
    expect(byOperation.references).toEqual(
      expect.arrayContaining(['groupByFile', 'rustContext'])
    );
    expect(lsp.select).toBe(
      "octocode schema lspSearch --view query --select operation=<operation> shows that operation's fields"
    );
    for (const entry of tools) {
      const view = query(String(entry.name));
      const shown = new Set([
        ...Object.keys(((view.querySchema as Json).properties ?? {}) as Json),
        ...Object.keys(view.continuationFields as Json),
      ]);
      const named = [
        ...Object.values(
          (view.operationFields ?? {}) as Record<string, string[]>
        ).flat(),
        ...((view.moreFields ?? []) as string[]),
      ];
      for (const field of named)
        expect(shown.has(field), `${entry.name}.${field}`).toBe(false);
      // Every accepted field is reachable from the query view: shown, a
      // continuation field, or named here.
      const selections = Object.values(
        entry.selections as Record<string, Json>
      );
      const accepted = selections.length
        ? selections.flatMap(selection =>
            Object.keys(
              ((selection.querySchema as Json).properties ?? {}) as Json
            )
          )
        : Object.keys(
            (((entry.views as Json).full as Json).querySchema as Json)
              .properties ?? {}
          );
      for (const field of accepted.filter(
        field => field !== 'mainGoal' && field !== 'reasoning'
      ))
        expect(
          shown.has(field) || named.includes(field),
          `${entry.name}.${field} is undiscoverable`
        ).toBe(true);
    }
  });

  it('pre-renders one selection per operation, keyed operation=VALUE', () => {
    for (const entry of tools) {
      const operations = Object.keys(
        operationSchemas(definition(String(entry.name)).inputSchema)
      );
      expect(
        Object.keys(entry.selections as Json).sort(),
        String(entry.name)
      ).toEqual(operations.map(operation => `operation=${operation}`).sort());
      expect(entry.variantNames).toEqual(operations);
    }
    const symbols = (tool('astSearch').selections as Json)[
      'operation=symbols'
    ] as Json;
    expect(
      ((symbols.querySchema as Json).properties as Json).symbolName
    ).toBeDefined();
  });

  it('emits operation views directly from the core Zod projection', () => {
    const availableTools = DIRECT_TOOL_DEFINITIONS.map(entry => entry.name);
    for (const selection of [false, true]) {
      for (const definition of getDirectToolDefinitionsWithAddons({
        availableTools,
        selection,
      })) {
        const entry = tool(definition.name);
        const rendered = (
          selection && entry.selection ? entry.selection : entry
        ) as Json;
        const views = rendered.selections as Record<string, Json>;
        for (const [operation, schema] of Object.entries(
          operationSchemas(definition.inputSchema)
        )) {
          expect(
            views[`operation=${operation}`]!.querySchema,
            `${definition.name}.${operation}, selection=${selection}`
          ).toEqual(publishedJson(schema));
        }
      }
    }
  });

  it('serves the canonical row from the enforcement contract, not a second emission', () => {
    const full = (tool('lspSearch').views as Json).full as Json;
    const { $schema: _schema, ...querySchema } =
      getPublicToolCatalogWithAddons().tools.find(
        entry => entry.name === 'lspSearch'
      )!.querySchema as Json;
    expect(full.querySchema).toEqual(querySchema);
    expect(full.querySchema).not.toHaveProperty('$schema');
  });
});
