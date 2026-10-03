import { describe, it, expect, vi } from 'vitest';
import {
  project,
  projectSelected,
  runScheme,
  useCompactJson,
} from '../../../src/cli/commands/scheme.js';
import {
  usageLines,
  type JsonObject,
} from '../../../src/cli/commands/scheme-projection.js';
import {
  getPublicToolCatalog,
  getNativeContractFingerprint,
} from '@octocodeai/config/schema';
import { buildMcpInstructions } from '@octocodeai/config/mcp';

const catalog = getPublicToolCatalog();
const tools = catalog.tools as unknown as readonly JsonObject[];
const toolNamed = (name: string): JsonObject => {
  const tool = tools.find(candidate => candidate.name === name);
  if (!tool) throw new Error(`missing tool ${name}`);
  return { ...tool };
};

describe('core public catalog', () => {
  it('carries presentation for all 16 tools and never an output schema', () => {
    expect(tools).toHaveLength(16);
    for (const tool of tools) {
      expect(tool.outputSchema, String(tool.name)).toBeUndefined();
      expect(typeof tool.description, String(tool.name)).toBe('string');
      expect(typeof tool.querySchema, String(tool.name)).toBe('object');
    }
  });

  it('exposes the enforcement fingerprint used by the fail-closed gate', () => {
    expect(catalog.fingerprint).toMatch(/^[a-f0-9]{64}$/);
    expect(getNativeContractFingerprint()).toBe(catalog.fingerprint);
  });

  it('builds availability-scoped instructions without a native instruction table', () => {
    const instructions = buildMcpInstructions(
      tools.map(tool => String(tool.name))
    );
    expect(buildMcpInstructions([])).not.toMatch(/\bclasify\b/i);
    expect(instructions).toContain('clasify');
    // Hard cutover: the pre-rename public name never appears in instructions.
    expect(buildMcpInstructions([])).not.toContain('semanticAssess');
    expect(instructions).not.toContain('semanticAssess');
  });
});

describe('scheme output format', () => {
  it('is compact when piped, indented on a terminal, and flag-overridable', () => {
    expect(useCompactJson({}, false)).toBe(true);
    expect(useCompactJson({}, true)).toBe(false);
    expect(useCompactJson({ compact: true }, true)).toBe(true);
    expect(useCompactJson({ pretty: true }, false)).toBe(false);
  });
});

describe('scheme command admission', () => {
  it('rejects unknown flags with the global JSON error contract', async () => {
    const output = vi.spyOn(console, 'log').mockImplementation(() => {});
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    await expect(
      runScheme({
        command: 'scheme',
        args: [],
        options: { bogus: true, 'json-errors': true },
      })
    ).resolves.toBe(2);
    expect(JSON.parse(output.mock.calls[0][0])).toEqual({
      kind: 'octocode.toolError',
      version: 1,
      error: 'Unknown option: --bogus',
    });
    expect(error).not.toHaveBeenCalled();
    output.mockRestore();
    error.mockRestore();
  });

  it('rejects missing selector values', async () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    await expect(
      runScheme({
        command: 'scheme',
        args: ['localSearch'],
        options: { select: true },
      })
    ).resolves.toBe(2);
    expect(error).toHaveBeenCalledWith('--select requires a value.');
    error.mockRestore();
  });
  it('rejects extra tool names instead of silently ignoring them', async () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    await expect(
      runScheme({
        command: 'scheme',
        args: ['localFetch', 'localSearch'],
        options: {},
      })
    ).resolves.toBe(2);
    expect(error).toHaveBeenCalledWith(
      'scheme accepts one tool name per call.'
    );
  });

  it.each(['help', 'h'])('prints usage for --%s', async option => {
    const output = vi.spyOn(console, 'log').mockImplementation(() => {});
    await expect(
      runScheme({
        command: 'scheme',
        args: [],
        options: { [option]: true },
      })
    ).resolves.toBe(0);
    expect(output).toHaveBeenCalledWith(
      expect.stringContaining('octocode scheme')
    );
  });

  it('rejects an invalid view on the text error channel', async () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    await expect(
      runScheme({
        command: 'scheme',
        args: [],
        options: { view: 'invalid' },
      })
    ).resolves.toBe(2);
    expect(error).toHaveBeenCalledWith(
      '--view expects full|query|variants, got: invalid'
    );
  });

  it('rejects an invalid view on the JSON error channel', async () => {
    const output = vi.spyOn(console, 'log').mockImplementation(() => {});
    await expect(
      runScheme({
        command: 'scheme',
        args: [],
        options: { view: 'invalid', 'json-errors': true },
      })
    ).resolves.toBe(2);
    expect(JSON.parse(String(output.mock.calls[0]?.[0]))).toEqual({
      kind: 'octocode.toolError',
      version: 1,
      error: '--view expects full|query|variants, got: invalid',
    });
  });
});

describe('project', () => {
  it('full view returns the public tool object', () => {
    const full = project(toolNamed('localFetch'), 'full');
    expect(full.name).toBe('localFetch');
    expect(full.querySchema).toBeUndefined();
    expect(full.inputSchema).toEqual(toolNamed('localFetch').inputSchema);
    expect(full.description).toBeDefined();
    expect(full.outputSchema).toBeUndefined();
  });

  it.each(['clasify', 'localSearch'])(
    'full view keeps one complete input schema for %s',
    name => {
      const tool = toolNamed(name);
      const full = project(tool, 'full');
      expect(full.inputSchema).toEqual(tool.inputSchema);
      expect(full).not.toHaveProperty('querySchema');
      expect(full).not.toHaveProperty('outputSchema');
      expect(JSON.stringify(full).length).toBeLessThan(
        JSON.stringify(tool).length
      );
    }
  );

  it('variants view exposes compact branch selectors before schema details', () => {
    const searchVariants = project(toolNamed('astSearch'), 'variants');
    expect(searchVariants.name).toBe('astSearch');
    expect(searchVariants.querySchema).toBeUndefined();
    expect(searchVariants.variants).toEqual(
      expect.arrayContaining([expect.objectContaining({ name: 'syntaxTree' })])
    );

    const topologyVariants = project(toolNamed('astTopology'), 'variants');
    expect(topologyVariants.name).toBe('astTopology');
    expect(topologyVariants.querySchema).toBeUndefined();
    expect(topologyVariants.variants).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ name: 'dependencies' }),
      ])
    );
  });

  it('query view is self-contained with envelope bounds', () => {
    const query = project(toolNamed('localSearch'), 'query');
    expect(Object.keys(query).sort()).toEqual(
      expect.arrayContaining(['name', 'querySchema'])
    );
    expect(query.name).toBe('localSearch');
    const envelope = query.queryEnvelope as
      { queries?: Record<string, unknown> } | undefined;
    expect(envelope?.queries).toBeDefined();
  });
});

describe('usageLines', () => {
  it('marks mandatory params <angle> and optional [square] for a simple tool', () => {
    const lines = usageLines(toolNamed('localFetch'));
    expect(lines[0]).toBe('octocode localFetch \'{"queries":[ … ]}\'');
    const body = lines[1]!;
    expect(body).toContain('<path>');
    // Briefs are optional; the legacy `goal` alias is not advertised.
    expect(body).toContain('[mainGoal]');
    expect(body).toContain('[reasoning]');
    expect(body).not.toContain('<mainGoal>');
    expect(body).not.toContain('<reasoning>');
    expect(body).not.toMatch(/[<[]goal[>\]]/);
    // A required field is never also shown as optional.
    expect(body).not.toContain('[path]');
  });

  it('labels each union branch by its discriminator const and drops it from fields', () => {
    const lines = usageLines(toolNamed('ghSearchHistory'));
    const commit = lines.find(line => line.startsWith('operation=commit'));
    const issue = lines.find(line => line.startsWith('operation=issue'));
    expect(commit).toBeDefined();
    expect(issue).toBeDefined();
    // The discriminator is the label, not a field.
    expect(commit).not.toContain('<operation>');
  });

  it('lists required fields for a single-shape tool', () => {
    // ghStructure requires both fields; ghSearchCode requires an owner only.
    const [, tree] = usageLines(toolNamed('ghStructure'));
    const [, code] = usageLines(toolNamed('ghSearchCode'));
    expect(tree).toContain('<owner>');
    expect(tree).toContain('<repo>');
    expect(code).toContain('<owner>');
    expect(code).not.toContain('<repo>');
  });

  it('resolves a $ref discriminator so every union branch is labeled', () => {
    // astSearch authors `operation` behind a $ref in its match branches.
    const lines = usageLines(toolNamed('astSearch'));
    expect(lines.slice(1).every(line => line.startsWith('operation='))).toBe(
      true
    );
    expect(lines.some(line => line.startsWith('operation=match'))).toBe(true);
  });

  it('labels branches by the const that varies, not one shared by every branch', () => {
    // Topology branches select their operation through analysis.
    const lines = usageLines(toolNamed('astTopology')).slice(1);
    expect(lines.length).toBeGreaterThan(1);
    expect(lines.every(line => line.startsWith('analysis='))).toBe(true);
    expect(new Set(lines.map(line => line.split(' ')[0])).size).toBe(
      lines.length
    );
  });

  it('degrades to a scheme hint when no query fields are exposed', () => {
    const lines = usageLines(toolNamed('clasify'));
    expect(lines[0]).toBe('octocode clasify \'{"queries":[ … ]}\'');
    expect(lines.some(line => line.includes('--view query'))).toBe(true);
  });
});

describe('compact query view', () => {
  const isObject = (value: unknown): value is JsonObject =>
    typeof value === 'object' && value !== null && !Array.isArray(value);
  const branchesOf = (schema: JsonObject): JsonObject[] =>
    ((schema.oneOf ?? schema.anyOf ?? [schema]) as unknown[]).filter(isObject);
  // Every `$ref` expanded in place (bounded for recursion); a reference to a
  // summarized definition expands to the view's summary on both sides.
  const expand = (
    root: JsonObject,
    value: unknown,
    summaries: JsonObject,
    depth = 0
  ): unknown => {
    if (Array.isArray(value))
      return value.map(item => expand(root, item, summaries, depth));
    if (!isObject(value)) return value;
    const ref = typeof value.$ref === 'string' ? value.$ref : undefined;
    if (ref?.startsWith('#/$defs/')) {
      const name = ref.slice('#/$defs/'.length);
      const { $ref: _ref, ...siblings } = value;
      if (name in summaries || depth > 6)
        return {
          ...((summaries[name] ??
            (root.$defs as JsonObject)[name]) as JsonObject),
          ...siblings,
        };
      return {
        ...(expand(
          root,
          (root.$defs as JsonObject)[name],
          summaries,
          depth + 1
        ) as JsonObject),
        ...(expand(root, siblings, summaries, depth + 1) as JsonObject),
      };
    }
    return Object.fromEntries(
      Object.entries(value).map(([key, child]) => [
        key,
        expand(root, child, summaries, depth),
      ])
    );
  };

  it('keeps every field, requirement and small definition of each tool', () => {
    for (const tool of tools) {
      const full = tool.querySchema as JsonObject;
      const view = project({ ...tool }, 'query').querySchema as JsonObject;
      expect(view.$schema, String(tool.name)).toBeUndefined();
      const summaries = Object.fromEntries(
        Object.entries((view.$defs ?? {}) as JsonObject).filter(
          ([, definition]) =>
            isObject(definition) &&
            String(definition.$comment ?? '').includes('--view full')
        )
      );
      for (const [name, summary] of Object.entries(summaries)) {
        const original = (full.$defs as JsonObject)[name] as JsonObject;
        expect(JSON.stringify(original).length, name).toBeGreaterThan(300);
        expect((summary as JsonObject).description).toEqual(
          original.description
        );
      }
      // A root `$ref` (clasify's matrix) reads through to its definition.
      const rooted = (schema: JsonObject): JsonObject =>
        typeof schema.$ref === 'string'
          ? ((schema.$defs as JsonObject)[
              schema.$ref.slice('#/$defs/'.length)
            ] as JsonObject)
          : schema;
      const fullBranches = branchesOf(rooted(full));
      const viewBranches = branchesOf(rooted(view));
      expect(viewBranches).toHaveLength(fullBranches.length);
      fullBranches.forEach((original, index) => {
        const compact = viewBranches[index]!;
        expect(compact.required, String(tool.name)).toEqual(original.required);
        const hoisted =
          fullBranches.length > 1
            ? ((rooted(view).properties ?? {}) as JsonObject)
            : {};
        const shown = {
          ...hoisted,
          ...((compact.properties ?? {}) as JsonObject),
        };
        const fields = (original.properties ?? {}) as JsonObject;
        expect(Object.keys(shown).sort(), String(tool.name)).toEqual(
          Object.keys(fields).sort()
        );
        // The view drops safe-integer sentinel maximums (enforcement guards).
        const unguarded = (value: unknown): unknown =>
          JSON.parse(
            JSON.stringify(value, (key, child) =>
              key === 'maximum' && typeof child === 'number' && child >= 1e9
                ? undefined
                : child
            )
          );
        for (const [name, field] of Object.entries(fields))
          expect(
            expand(view, shown[name], summaries),
            `${String(tool.name)}.${name}`
          ).toEqual(unguarded(expand(full, field, summaries)));
        if (original.additionalProperties === false && fullBranches.length > 1)
          expect(rooted(view).unevaluatedProperties).toBe(false);
      });
    }
  });

  it('halves the astRewrite read while full keeps the rule grammar', () => {
    const rewrite = toolNamed('astRewrite');
    // Regression guard: 9,587 B before compaction (plan target 4,500 B).
    expect(
      Buffer.byteLength(JSON.stringify(project(rewrite, 'query')))
    ).toBeLessThanOrEqual(4_700);
    expect(JSON.stringify(project(rewrite, 'full'))).toContain('"precedes"');
  });
});

describe('projectSelected', () => {
  it('passes through when no selection is given', () => {
    const projected = projectSelected(
      toolNamed('ghSearchHistory'),
      'query',
      undefined
    );
    expect(projected.name).toBe('ghSearchHistory');
  });

  it('rejects selection outside query view', () => {
    expect(() =>
      projectSelected(toolNamed('ghSearchHistory'), 'full', 'operation=commit')
    ).toThrow('--select requires --view query');
  });

  it('rejects malformed selections', () => {
    expect(() =>
      projectSelected(toolNamed('ghSearchHistory'), 'query', 'operation')
    ).toThrow('FIELD=VALUE');
  });

  it('isolates exactly one union branch and prunes unreachable defs', () => {
    const projected = projectSelected(
      toolNamed('ghSearchHistory'),
      'query',
      'operation=commit'
    );
    const schema = projected.querySchema as JsonObject;
    const union = (schema.oneOf ?? schema.anyOf) as unknown[];
    expect(union).toHaveLength(1);
    const serialized = JSON.stringify(schema);
    const defs = (schema.$defs ?? {}) as JsonObject;
    for (const name of Object.keys(defs)) {
      expect(serialized).toContain(`#/$defs/${name}`);
    }
  });

  it('selects a named nested astTopology variant in one step', () => {
    const projected = projectSelected(
      toolNamed('astTopology'),
      'query',
      'variant=dependencies'
    );
    const schema = projected.querySchema as JsonObject;
    const union = (schema.oneOf ?? schema.anyOf) as JsonObject[];
    expect(union).toHaveLength(1);
    const properties = union[0]!.properties as JsonObject;
    expect(properties).not.toHaveProperty('operation');
    expect((properties.analysis as JsonObject).const).toBe('dependencies');
  });

  it('keeps both valid match shapes when selecting the match variant', () => {
    const projected = projectSelected(
      toolNamed('astSearch'),
      'query',
      'variant=match'
    );
    const schema = projected.querySchema as JsonObject;
    const union = (schema.oneOf ?? schema.anyOf) as JsonObject[];
    expect(union).toHaveLength(2);
  });

  it('keeps every branch sharing the selected const (operation=match)', () => {
    const projected = projectSelected(
      toolNamed('astSearch'),
      'query',
      'operation=match'
    );
    const schema = projected.querySchema as JsonObject;
    const union = (schema.oneOf ?? schema.anyOf) as JsonObject[];
    expect(union).toHaveLength(2);
  });

  it('accepts a catalog label such as operation=match(pattern)', () => {
    const projected = projectSelected(
      toolNamed('astSearch'),
      'query',
      'operation=match(pattern)'
    );
    const schema = projected.querySchema as JsonObject;
    const union = (schema.oneOf ?? schema.anyOf) as JsonObject[];
    expect(union).toHaveLength(1);
    expect(union[0]!.required).toContain('pattern');
  });

  it.each([
    ['definition', ['anchored', 'position']],
    ['references', ['anchored', 'position']],
    ['diagnostic', ['document']],
    ['workspaceSymbol', ['workspace:uri', 'workspace:root']],
  ])('selects all LSP shapes accepting operation=%s', (operation, titles) => {
    const projected = projectSelected(
      toolNamed('lspSearch'),
      'query',
      `operation=${operation}`
    );
    const schema = projected.querySchema as JsonObject;
    expect((schema.anyOf as JsonObject[]).map(branch => branch.title)).toEqual(
      titles
    );
    expect(JSON.stringify(schema)).not.toContain('outputSchema');
  });

  it.each([
    'anchored',
    'position',
    'document',
    'workspace:uri',
    'workspace:root',
  ])('selects the exact named LSP variant %s', variant => {
    const schema = projectSelected(
      toolNamed('lspSearch'),
      'query',
      `variant=${variant}`
    ).querySchema as JsonObject;
    expect((schema.anyOf as JsonObject[]).map(branch => branch.title)).toEqual([
      variant,
    ]);
  });

  it('follows chained local refs but never treats a default as an enum', () => {
    const branch = (operation: JsonObject): JsonObject => ({
      type: 'object',
      properties: { operation },
    });
    const tool: JsonObject = {
      name: 'fixture',
      querySchema: {
        anyOf: [
          branch({ $ref: '#/$defs/alias' }),
          branch({ default: 'definition', type: 'string' }),
        ],
        $defs: {
          alias: { $ref: '#/$defs/operations' },
          operations: { enum: ['definition', 'references'] },
        },
      },
    };
    const schema = projectSelected(tool, 'query', 'operation=references')
      .querySchema as JsonObject;
    expect(schema.anyOf).toHaveLength(1);
    expect(schema.$defs).toEqual((tool.querySchema as JsonObject).$defs);
    expect(() => projectSelected(tool, 'query', 'operation=missing')).toThrow(
      'matched 0'
    );
  });

  it('preserves oneOf exclusions when an enum branch can overlap its sibling', () => {
    const sibling = {
      type: 'object',
      properties: { operation: { const: 'references' } },
      required: ['operation'],
    };
    const tool: JsonObject = {
      name: 'fixture',
      querySchema: {
        oneOf: [
          {
            type: 'object',
            properties: { operation: { enum: ['definition', 'references'] } },
            required: ['operation'],
          },
          sibling,
        ],
      },
    };
    const schema = projectSelected(tool, 'query', 'operation=definition')
      .querySchema as JsonObject;
    expect(schema.oneOf).toHaveLength(1);
    expect(schema.allOf).toEqual([{ not: { anyOf: [sibling] } }]);
  });

  it('rejects selections matching no branch', () => {
    expect(() =>
      projectSelected(toolNamed('ghSearchHistory'), 'query', 'operation=nope')
    ).toThrow('matched 0');
  });
});
