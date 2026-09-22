import { describe, it, expect, vi } from 'vitest';
import {
  project,
  projectSelected,
  runScheme,
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
  it('carries presentation for all 13 tools and never an output schema', () => {
    expect(tools).toHaveLength(13);
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
    expect(instructions).toContain('Route each unresolved question');
    expect(instructions).toContain('clasify');
    // Hard cutover: the pre-rename public name never appears in instructions.
    expect(buildMcpInstructions([])).not.toContain('semanticAssess');
    expect(instructions).not.toContain('semanticAssess');
  });
});

describe('scheme command admission', () => {
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
      success: false,
      error: '--view expects full|query|variants, got: invalid',
    });
  });
});

describe('project', () => {
  it('full view returns the public tool object', () => {
    const full = project(toolNamed('localFetch'), 'full');
    expect(full.name).toBe('localFetch');
    expect(full.querySchema).toBeDefined();
    expect(full.description).toBeDefined();
    expect(full.outputSchema).toBeUndefined();
  });

  it('variants view exposes compact branch selectors before schema details', () => {
    const searchVariants = project(toolNamed('astSearch'), 'variants');
    expect(searchVariants.name).toBe('astSearch');
    expect(searchVariants.querySchema).toBeUndefined();
    expect(searchVariants.variants).toEqual(
      expect.arrayContaining([expect.objectContaining({ name: 'tree:syntax' })])
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
    expect(body).toContain('<reasoning>');
    expect(body).toContain('<path>');
    expect(body).toContain('[goal]');
    // A required field is never also shown as optional.
    expect(body).not.toContain('[reasoning]');
    expect(body).not.toContain('[path]');
  });

  it('labels each union branch by its discriminator const and drops it from fields', () => {
    const lines = usageLines(toolNamed('ghSearch'));
    const code = lines.find(line => line.startsWith('operation=code'));
    const repos = lines.find(line => line.startsWith('operation=repositories'));
    const tree = lines.find(line => line.startsWith('operation=tree'));
    expect(code).toBeDefined();
    expect(repos).toBeDefined();
    expect(tree).toBeDefined();
    // The discriminator is the label, not a field.
    expect(code).not.toContain('<operation>');
    // tree requires owner+repo; code does not.
    expect(tree).toContain('<owner>');
    expect(tree).toContain('<repo>');
    expect(code).not.toContain('<owner>');
  });

  it('resolves a $ref discriminator so every union branch is labeled', () => {
    // astSearch authors `operation` behind a $ref in its match branches.
    const lines = usageLines(toolNamed('astSearch'));
    expect(lines.slice(1).every(line => line.startsWith('operation='))).toBe(
      true
    );
    expect(lines.some(line => line.startsWith('operation=match'))).toBe(true);
  });

  it('degrades to a scheme hint when no query fields are exposed', () => {
    const lines = usageLines(toolNamed('clasify'));
    expect(lines[0]).toBe('octocode clasify \'{"queries":[ … ]}\'');
    expect(lines.some(line => line.includes('--view query'))).toBe(true);
  });
});

describe('projectSelected', () => {
  it('passes through when no selection is given', () => {
    const projected = projectSelected(
      toolNamed('ghSearch'),
      'query',
      undefined
    );
    expect(projected.name).toBe('ghSearch');
  });

  it('rejects selection outside query view', () => {
    expect(() =>
      projectSelected(toolNamed('ghSearch'), 'full', 'operation=code')
    ).toThrow('--select requires --view query');
  });

  it('rejects malformed selections', () => {
    expect(() =>
      projectSelected(toolNamed('ghSearch'), 'query', 'operation')
    ).toThrow('FIELD=VALUE');
  });

  it('isolates exactly one union branch and prunes unreachable defs', () => {
    const projected = projectSelected(
      toolNamed('ghSearch'),
      'query',
      'operation=code'
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
    const operation = properties.operation as JsonObject;
    const defs = schema.$defs as JsonObject;
    expect((defs.T_Operation as JsonObject).const).toBe('topology');
    expect(operation.$ref).toBe('#/$defs/T_Operation');
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

  it('rejects selections matching no branch', () => {
    expect(() =>
      projectSelected(toolNamed('ghSearch'), 'query', 'operation=nope')
    ).toThrow('matched 0');
  });
});
