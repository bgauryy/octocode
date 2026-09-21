import { describe, it, expect } from 'vitest';
import { project, projectSelected } from '../../../src/cli/commands/scheme.js';
import {
  getPublicToolCatalog,
  getNativeContractFingerprint,
} from '@octocodeai/octocode-core/schema';
import { buildMcpInstructions } from '@octocodeai/octocode-core/mcp';

type JsonObject = Record<string, unknown>;

const catalog = getPublicToolCatalog();
const tools = catalog.tools as readonly JsonObject[];
const toolNamed = (name: string): JsonObject => {
  const tool = tools.find(candidate => candidate.name === name);
  if (!tool) throw new Error(`missing tool ${name}`);
  return { ...tool };
};

describe('core public catalog', () => {
  it('carries presentation for all 12 tools and never an output schema', () => {
    expect(tools).toHaveLength(12);
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
      tools.map(tool => String(tool.name)),
    );
    expect(instructions).toContain('Choose a tool');
    expect(instructions).toContain('semanticAssess');
    expect(buildMcpInstructions([])).not.toContain('semanticAssess');
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

  it('query view is self-contained with envelope bounds', () => {
    const query = project(toolNamed('localSearch'), 'query');
    expect(Object.keys(query).sort()).toEqual(
      expect.arrayContaining(['name', 'querySchema']),
    );
    expect(query.name).toBe('localSearch');
    const envelope = query.queryEnvelope as
      | { queries?: Record<string, unknown> }
      | undefined;
    expect(envelope?.queries).toBeDefined();
  });
});

describe('projectSelected', () => {
  it('passes through when no selection is given', () => {
    const projected = projectSelected(toolNamed('ghSearch'), 'query', undefined);
    expect(projected.name).toBe('ghSearch');
  });

  it('rejects selection outside query view', () => {
    expect(() =>
      projectSelected(toolNamed('ghSearch'), 'full', 'operation=code'),
    ).toThrow('--select requires --view query');
  });

  it('rejects malformed selections', () => {
    expect(() =>
      projectSelected(toolNamed('ghSearch'), 'query', 'operation'),
    ).toThrow('FIELD=VALUE');
  });

  it('isolates exactly one union branch and prunes unreachable defs', () => {
    const projected = projectSelected(
      toolNamed('ghSearch'),
      'query',
      'operation=code',
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

  it('rejects selections matching no branch', () => {
    expect(() =>
      projectSelected(toolNamed('ghSearch'), 'query', 'operation=nope'),
    ).toThrow('matched 0');
  });
});
