import { afterEach, describe, it, expect, vi } from 'vitest';
import { runSchema } from '../../../src/cli/commands/schema.js';
import {
  project,
  usageLines,
} from '../../../src/cli/commands/schema-projection.js';
import {
  getPublicToolCatalog,
  getNativeContractFingerprint,
  type SchemaJsonObject,
} from '@octocodeai/config/schema';
import {
  buildCliInstructions,
  buildMcpInstructions,
} from '@octocodeai/config/mcp';

const catalog = getPublicToolCatalog();
const tools = catalog.tools as unknown as readonly SchemaJsonObject[];
const toolNamed = (name: string): SchemaJsonObject => {
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

  it('serves the core prompt without a native instruction table', () => {
    const instructions = buildCliInstructions();
    expect(instructions).toBe(buildMcpInstructions());
    expect(instructions).toMatch(/^<clasify>Needs a provider key\. /m);
    expect(instructions).toMatch(/^<cli>CLI only: ghCloneRepo; /m);
    // Hard cutover: the pre-rename public name never appears in instructions.
    expect(instructions).not.toContain('semanticAssess');
  });
});

const stdoutTty = Object.getOwnPropertyDescriptor(process.stdout, 'isTTY');
function setTty(value: boolean): void {
  Object.defineProperty(process.stdout, 'isTTY', {
    value,
    configurable: true,
  });
}
afterEach(() => {
  if (stdoutTty) Object.defineProperty(process.stdout, 'isTTY', stdoutTty);
  else delete (process.stdout as { isTTY?: boolean }).isTTY;
  vi.restoreAllMocks();
});

describe('schema command admission', () => {
  it('rejects unknown flags with the JSON error envelope on a pipe', async () => {
    setTty(false);
    const output = vi.spyOn(console, 'log').mockImplementation(() => {});
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    await expect(
      runSchema({ command: 'schema', args: [], options: { bogus: true } })
    ).resolves.toBe(2);
    const printed = JSON.parse(output.mock.calls[0][0]);
    expect(printed.kind).toBe('octocode.toolError');
    expect(printed.error).toContain('Unknown option: --bogus');
    expect(error).not.toHaveBeenCalled();
  });

  it('rejects removed flags (--compact, --pretty, --json-errors)', async () => {
    setTty(true);
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    for (const flag of ['compact', 'pretty', 'json-errors']) {
      await expect(
        runSchema({ command: 'schema', args: [], options: { [flag]: true } })
      ).resolves.toBe(2);
    }
    expect(error).toHaveBeenCalledTimes(3);
  });

  it('rejects missing selector values', async () => {
    setTty(true);
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    await expect(
      runSchema({
        command: 'schema',
        args: ['localSearch'],
        options: { select: true },
      })
    ).resolves.toBe(2);
    expect(error).toHaveBeenCalledWith('--select requires a value.');
  });

  it('rejects extra tool names instead of silently ignoring them', async () => {
    setTty(true);
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    await expect(
      runSchema({
        command: 'schema',
        args: ['localFetch', 'localSearch'],
        options: {},
      })
    ).resolves.toBe(2);
    expect(error).toHaveBeenCalledWith(
      'schema accepts one tool name per call.'
    );
  });

  it('requires a tool name for --view and --select', async () => {
    setTty(true);
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    await expect(
      runSchema({ command: 'schema', args: [], options: { view: 'query' } })
    ).resolves.toBe(2);
    expect(error).toHaveBeenCalledWith(
      expect.stringContaining('need a tool name')
    );
  });

  it('rejects an invalid view as text on a terminal', async () => {
    setTty(true);
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    await expect(
      runSchema({
        command: 'schema',
        args: ['localSearch'],
        options: { view: 'invalid' },
      })
    ).resolves.toBe(2);
    expect(error).toHaveBeenCalledWith(
      '--view expects query|variants|full, got: invalid'
    );
  });

  it('rejects an invalid view as the JSON envelope on a pipe', async () => {
    setTty(false);
    const output = vi.spyOn(console, 'log').mockImplementation(() => {});
    await expect(
      runSchema({
        command: 'schema',
        args: ['localSearch'],
        options: { view: 'invalid' },
      })
    ).resolves.toBe(2);
    expect(JSON.parse(String(output.mock.calls[0]?.[0]))).toEqual({
      kind: 'octocode.toolError',
      version: 1,
      error: '--view expects query|variants|full, got: invalid',
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
    // Topology branches select their analysis through operation.
    const lines = usageLines(toolNamed('astTopology')).slice(1);
    expect(lines.length).toBeGreaterThan(1);
    expect(lines.every(line => line.startsWith('operation='))).toBe(true);
    expect(new Set(lines.map(line => line.split(' ')[0])).size).toBe(
      lines.length
    );
  });

  it('degrades to a schema hint when no query fields are exposed', () => {
    const lines = usageLines(toolNamed('clasify'));
    expect(lines[0]).toBe('octocode clasify \'{"queries":[ … ]}\'');
    expect(lines.some(line => line.includes('--view query'))).toBe(true);
  });
});

describe('compact query view', () => {
  const isObject = (value: unknown): value is SchemaJsonObject =>
    typeof value === 'object' && value !== null && !Array.isArray(value);
  const branchesOf = (schema: SchemaJsonObject): SchemaJsonObject[] =>
    ((schema.oneOf ?? schema.anyOf ?? [schema]) as unknown[]).filter(isObject);
  // Every `$ref` expanded in place (bounded for recursion); a reference to a
  // summarized definition expands to the view's summary on both sides.
  const expand = (
    root: SchemaJsonObject,
    value: unknown,
    summaries: SchemaJsonObject,
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
            (root.$defs as SchemaJsonObject)[name]) as SchemaJsonObject),
          ...siblings,
        };
      return {
        ...(expand(
          root,
          (root.$defs as SchemaJsonObject)[name],
          summaries,
          depth + 1
        ) as SchemaJsonObject),
        ...(expand(root, siblings, summaries, depth + 1) as SchemaJsonObject),
      };
    }
    return Object.fromEntries(
      Object.entries(value).map(([key, child]) => [
        key,
        expand(root, child, summaries, depth),
      ])
    );
  };

  // CLI compact views retain optional research briefs even when MCP omits them.
  const briefNotes = (tool: SchemaJsonObject): Record<string, string> => ({
    mainGoal:
      tool.name === 'clasify'
        ? 'Research question, sent to the judge. Name the artifact kind.'
        : 'Multi-call research only.',
    reasoning:
      tool.name === 'clasify' ? 'Sent to the judge.' : 'Research only.',
  });

  // Every brief field, nested ones included (a clasify resource's tool query).
  const shortenBriefs = (
    value: unknown,
    notes: Record<string, string>
  ): void => {
    if (Array.isArray(value))
      return value.forEach(v => shortenBriefs(v, notes));
    if (!isObject(value)) return;
    const properties = value.properties;
    if (isObject(properties))
      for (const [name, note] of Object.entries(notes)) {
        const field = properties[name];
        if (!isObject(field)) continue;
        delete field.pattern;
        field.description = note;
      }
    Object.values(value).forEach(v => shortenBriefs(v, notes));
  };

  it('keeps every field, requirement and small definition of each tool', () => {
    for (const tool of tools) {
      const notes = briefNotes(tool);
      expect(Object.keys(notes).sort(), String(tool.name)).toEqual([
        'mainGoal',
        'reasoning',
      ]);
      const full = tool.querySchema as SchemaJsonObject;
      const view = project({ ...tool }, 'query')
        .querySchema as SchemaJsonObject;
      expect(view.$schema, String(tool.name)).toBeUndefined();
      const summaries = Object.fromEntries(
        Object.entries((view.$defs ?? {}) as SchemaJsonObject).filter(
          ([, definition]) =>
            isObject(definition) &&
            String(definition.$comment ?? '').includes('--view full')
        )
      );
      for (const [name, summary] of Object.entries(summaries)) {
        const original = (full.$defs as SchemaJsonObject)[
          name
        ] as SchemaJsonObject;
        expect(JSON.stringify(original).length, name).toBeGreaterThan(300);
        expect((summary as SchemaJsonObject).description).toEqual(
          original.description
        );
      }
      // A root `$ref` (clasify's matrix) reads through to its definition.
      const rooted = (schema: SchemaJsonObject): SchemaJsonObject =>
        typeof schema.$ref === 'string'
          ? ((schema.$defs as SchemaJsonObject)[
              schema.$ref.slice('#/$defs/'.length)
            ] as SchemaJsonObject)
          : schema;
      const fullBranches = branchesOf(rooted(full));
      const viewBranches = branchesOf(rooted(view));
      expect(viewBranches).toHaveLength(fullBranches.length);
      fullBranches.forEach((original, index) => {
        const compact = viewBranches[index]!;
        expect(compact.required, String(tool.name)).toEqual(original.required);
        const hoisted =
          fullBranches.length > 1
            ? ((rooted(view).properties ?? {}) as SchemaJsonObject)
            : {};
        const shown = {
          ...hoisted,
          ...((compact.properties ?? {}) as SchemaJsonObject),
        };
        const fields = (original.properties ?? {}) as SchemaJsonObject;
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
        for (const [name, field] of Object.entries(fields)) {
          const expected = unguarded(
            expand(full, field, summaries)
          ) as SchemaJsonObject;
          shortenBriefs({ properties: { [name]: expected } }, notes);
          expect(
            expand(view, shown[name], summaries),
            `${String(tool.name)}.${name}`
          ).toEqual(expected);
        }
        if (original.additionalProperties === false && fullBranches.length > 1)
          expect(rooted(view).unevaluatedProperties).toBe(false);
      });
    }
  });

  it('halves the astRewrite read while full keeps the rule grammar', () => {
    const rewrite = toolNamed('astRewrite');
    // Regression guard: 9,587 B before compaction (plan target 4,500 B);
    // +~120 B since AS5 added the rule-file object `{rule, constraints?, …}`.
    expect(
      Buffer.byteLength(JSON.stringify(project(rewrite, 'query')))
    ).toBeLessThanOrEqual(4_900);
    expect(JSON.stringify(project(rewrite, 'full'))).toContain('"precedes"');
  });
});

describe('project with --select', () => {
  it('passes through when no selection is given', () => {
    const projected = project(toolNamed('ghSearchHistory'), 'query', undefined);
    expect(projected.name).toBe('ghSearchHistory');
  });

  it('rejects selection outside query view', () => {
    expect(() =>
      project(toolNamed('ghSearchHistory'), 'full', 'operation=commit')
    ).toThrow('--select requires --view query');
  });

  it('rejects malformed selections', () => {
    expect(() =>
      project(toolNamed('ghSearchHistory'), 'query', 'operation')
    ).toThrow('FIELD=VALUE');
  });

  it('isolates exactly one union branch and prunes unreachable defs', () => {
    const projected = project(
      toolNamed('ghSearchHistory'),
      'query',
      'operation=commit'
    );
    const schema = projected.querySchema as SchemaJsonObject;
    const union = (schema.oneOf ?? schema.anyOf) as unknown[];
    expect(union).toHaveLength(1);
    const serialized = JSON.stringify(schema);
    const defs = (schema.$defs ?? {}) as SchemaJsonObject;
    for (const name of Object.keys(defs)) {
      expect(serialized).toContain(`#/$defs/${name}`);
    }
  });

  it('selects a named nested astTopology variant in one step', () => {
    const projected = project(
      toolNamed('astTopology'),
      'query',
      'variant=dependencies'
    );
    const schema = projected.querySchema as SchemaJsonObject;
    const union = (schema.oneOf ?? schema.anyOf) as SchemaJsonObject[];
    expect(union).toHaveLength(1);
    const properties = union[0]!.properties as SchemaJsonObject;
    expect(properties).not.toHaveProperty('analysis');
    expect((properties.operation as SchemaJsonObject).const).toBe(
      'dependencies'
    );
  });

  it('keeps both valid match shapes when selecting the match variant', () => {
    const projected = project(toolNamed('astSearch'), 'query', 'variant=match');
    const schema = projected.querySchema as SchemaJsonObject;
    const union = (schema.oneOf ?? schema.anyOf) as SchemaJsonObject[];
    expect(union).toHaveLength(2);
  });

  it('keeps every branch sharing the selected const (operation=match)', () => {
    const projected = project(
      toolNamed('astSearch'),
      'query',
      'operation=match'
    );
    const schema = projected.querySchema as SchemaJsonObject;
    const union = (schema.oneOf ?? schema.anyOf) as SchemaJsonObject[];
    expect(union).toHaveLength(2);
  });

  it('accepts a catalog label such as operation=match(pattern)', () => {
    const projected = project(
      toolNamed('astSearch'),
      'query',
      'operation=match(pattern)'
    );
    const schema = projected.querySchema as SchemaJsonObject;
    const union = (schema.oneOf ?? schema.anyOf) as SchemaJsonObject[];
    expect(union).toHaveLength(1);
    expect(union[0]!.required).toContain('pattern');
  });

  it.each([
    ['definition', ['anchored']],
    ['references', ['anchored']],
    ['diagnostic', ['document']],
    ['workspaceSymbol', ['workspace:path', 'workspace:root']],
  ])('selects all LSP shapes accepting operation=%s', (operation, titles) => {
    const projected = project(
      toolNamed('lspSearch'),
      'query',
      `operation=${operation}`
    );
    const schema = projected.querySchema as SchemaJsonObject;
    expect(
      (schema.anyOf as SchemaJsonObject[]).map(branch => branch.title)
    ).toEqual(titles);
    expect(JSON.stringify(schema)).not.toContain('outputSchema');
  });

  it.each(['anchored', 'document', 'workspace:path', 'workspace:root'])(
    'selects the exact named LSP variant %s',
    variant => {
      const schema = project(
        toolNamed('lspSearch'),
        'query',
        `variant=${variant}`
      ).querySchema as SchemaJsonObject;
      expect(
        (schema.anyOf as SchemaJsonObject[]).map(branch => branch.title)
      ).toEqual([variant]);
    }
  );

  it('follows chained local refs but never treats a default as an enum', () => {
    const branch = (operation: SchemaJsonObject): SchemaJsonObject => ({
      type: 'object',
      properties: { operation },
    });
    const tool: SchemaJsonObject = {
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
    const schema = project(tool, 'query', 'operation=references')
      .querySchema as SchemaJsonObject;
    expect(schema.anyOf).toHaveLength(1);
    expect(schema.$defs).toEqual((tool.querySchema as SchemaJsonObject).$defs);
    expect(() => project(tool, 'query', 'operation=missing')).toThrow(
      'matched 0'
    );
  });

  it('preserves oneOf exclusions when an enum branch can overlap its sibling', () => {
    const sibling = {
      type: 'object',
      properties: { operation: { const: 'references' } },
      required: ['operation'],
    };
    const tool: SchemaJsonObject = {
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
    const schema = project(tool, 'query', 'operation=definition')
      .querySchema as SchemaJsonObject;
    expect(schema.oneOf).toHaveLength(1);
    expect(schema.allOf).toEqual([{ not: { anyOf: [sibling] } }]);
  });

  it('rejects selections matching no branch', () => {
    expect(() =>
      project(toolNamed('ghSearchHistory'), 'query', 'operation=nope')
    ).toThrow('matched 0');
  });
});
