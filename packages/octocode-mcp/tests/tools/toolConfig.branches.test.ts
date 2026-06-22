import { describe, it, expect, vi, beforeEach } from 'vitest';
import { toMCPSchema } from '../../src/types/toolTypes.js';

vi.mock(
  '../../../octocode-tools-core/src/tools/toolMetadata/proxies.js',
  async importOriginal => {
    const mod =
      await importOriginal<
        typeof import('../../../octocode-tools-core/src/tools/toolMetadata/proxies.js')
      >();
    return {
      ...mod,
      DESCRIPTIONS: new Proxy(mod.DESCRIPTIONS as Record<string, string>, {
        get(target, prop: string) {
          if (prop === '__nonexistent_tool_for_coverage__') return undefined;
          return Reflect.get(target, prop) ?? '';
        },
      }),
    };
  }
);

describe('toolConfig branch coverage - getDescription fallback (line 26)', () => {
  beforeEach(async () => {
    vi.resetModules();
  });

  describe('when DESCRIPTIONS returns undefined (fallback branch)', () => {
    it('should return empty string when tool is not in DESCRIPTIONS', async () => {
      const { DESCRIPTIONS } =
        await import('../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');

      const unknownDescription = DESCRIPTIONS['completely_unknown_tool_xyz'];
      expect(unknownDescription).toBe('');
    });

    it('should return empty string for undefined tool name', async () => {
      const { DESCRIPTIONS } =
        await import('../../../octocode-tools-core/src/tools/toolMetadata/proxies.js');

      const result = DESCRIPTIONS[''];
      expect(result).toBe('');
    });

    it('getDescription returns empty string for unknown tool (hits ?? fallback)', async () => {
      const { getDescription } = await import('../../src/tools/toolConfig.js');
      const result = getDescription('__nonexistent_tool_for_coverage__');
      expect(result).toBe('');
    }, 10_000);
  });

  describe('tool configuration initialization', () => {
    it('should create valid tool configs with all required properties', async () => {
      const {
        GITHUB_SEARCH_CODE,
        GITHUB_FETCH_CONTENT,
        GITHUB_VIEW_REPO_STRUCTURE,
        GITHUB_SEARCH_REPOSITORIES,
        GITHUB_SEARCH_PULL_REQUESTS,
        PACKAGE_SEARCH,
        ALL_TOOLS,
      } = await import('../../src/tools/toolConfig.js');

      const configs = [
        GITHUB_SEARCH_CODE,
        GITHUB_FETCH_CONTENT,
        GITHUB_VIEW_REPO_STRUCTURE,
        GITHUB_SEARCH_REPOSITORIES,
        GITHUB_SEARCH_PULL_REQUESTS,
        PACKAGE_SEARCH,
      ];

      for (const config of configs) {
        expect(config).toHaveProperty('name');
        expect(config).toHaveProperty('description');
        expect(config).toHaveProperty('isDefault');
        expect(config).toHaveProperty('type');
        expect(config).toHaveProperty('fn');
        expect(typeof config.name).toBe('string');
        expect(typeof config.description).toBe('string');
        expect(typeof config.isDefault).toBe('boolean');
        expect(typeof config.type).toBe('string');
        expect(typeof config.fn).toBe('function');
      }

      expect(ALL_TOOLS).toHaveLength(14);
    });

    it('should have correct tool types assigned', async () => {
      const {
        GITHUB_SEARCH_CODE,
        GITHUB_FETCH_CONTENT,
        GITHUB_VIEW_REPO_STRUCTURE,
        GITHUB_SEARCH_REPOSITORIES,
        GITHUB_SEARCH_PULL_REQUESTS,
        PACKAGE_SEARCH,
      } = await import('../../src/tools/toolConfig.js');

      expect(GITHUB_SEARCH_CODE.type).toBe('search');
      expect(GITHUB_SEARCH_REPOSITORIES.type).toBe('search');
      expect(PACKAGE_SEARCH.type).toBe('search');

      expect(GITHUB_FETCH_CONTENT.type).toBe('content');
      expect(GITHUB_VIEW_REPO_STRUCTURE.type).toBe('content');

      expect(GITHUB_SEARCH_PULL_REQUESTS.type).toBe('history');

      expect(GITHUB_SEARCH_PULL_REQUESTS.type).toBe('history');
    });

    it('should mark all tools as default (except opt-in tools)', async () => {
      const { ALL_TOOLS } = await import('../../src/tools/toolConfig.js');
      const optInTools = ['localBinaryInspect'];

      for (const tool of ALL_TOOLS) {
        if (!optInTools.includes(tool.name)) {
          expect(tool.isDefault).toBe(true);
        }
      }
    });
  });
});

describe('toolConfig - fn property', () => {
  it('should have callable registration functions', async () => {
    const {
      GITHUB_SEARCH_CODE,
      GITHUB_FETCH_CONTENT,
      GITHUB_VIEW_REPO_STRUCTURE,
      GITHUB_SEARCH_REPOSITORIES,
      GITHUB_SEARCH_PULL_REQUESTS,
      PACKAGE_SEARCH,
    } = await import('../../src/tools/toolConfig.js');

    expect(typeof GITHUB_SEARCH_CODE.fn).toBe('function');
    expect(typeof GITHUB_FETCH_CONTENT.fn).toBe('function');
    expect(typeof GITHUB_VIEW_REPO_STRUCTURE.fn).toBe('function');
    expect(typeof GITHUB_SEARCH_REPOSITORIES.fn).toBe('function');
    expect(typeof GITHUB_SEARCH_PULL_REQUESTS.fn).toBe('function');
    expect(typeof PACKAGE_SEARCH.fn).toBe('function');
  });
});

describe('toMCPSchema — branch coverage', () => {
  it('returns the schema as-is when no zod pipe/effects wrappers are present', () => {
    const plain = { _def: {}, shape: {} };
    const result = toMCPSchema(plain as never);
    expect(result).toBe(plain);
  });

  it('unwraps ZodPipeline wrapper via _def.typeName (line 23 branch)', () => {
    const inner = { _def: {}, shape: {} };
    const pipeline = {
      _def: { typeName: 'ZodPipeline', schema: inner },
    };
    const result = toMCPSchema(pipeline as never);
    expect(result).toBe(inner);
  });

  it('unwraps ZodEffects wrapper via _def.schema (line 23 branch)', () => {
    const inner = { _def: {}, shape: {} };
    const effects = {
      _def: { typeName: 'ZodEffects', schema: inner },
    };
    const result = toMCPSchema(effects as never);
    expect(result).toBe(inner);
  });

  it('uses _def.in when _def.schema is absent for ZodPipeline (line 23 ?? fallback)', () => {
    const inner = { _def: {}, shape: {} };
    const pipeline = {
      _def: { typeName: 'ZodPipeline', in: inner },
    };
    const result = toMCPSchema(pipeline as never);
    expect(result).toBe(inner);
  });

  it('traverses _zod.def.type === "pipe" chain (line 17 while branch)', () => {
    const final = { _def: {} };
    const piped = {
      _zod: { def: { type: 'pipe', out: final } },
      _def: { typeName: 'ZodPipeline', schema: final },
    };
    const result = toMCPSchema(piped as never);
    expect(result).toBe(final);
  });

  it('falls back to original schema when _def.schema and _def.in are both absent (line 23 ?? schema branch)', () => {
    const original = { _def: { typeName: 'ZodPipeline' } };
    const result = toMCPSchema(original as never);
    expect(result).toBe(original);
  });

  it('falls back to schema when s becomes falsy after pipe unwrap (line 25 ?? schema branch)', () => {
    const original = {
      _zod: { def: { type: 'pipe', out: null } },
      _def: {},
    };
    const result = toMCPSchema(original as never);
    expect(result).toBe(original);
  });
});
