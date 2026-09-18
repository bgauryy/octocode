import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ADAPTER_PARITY_CASES } from '../fixtures/adapterParityFixture.js';
import { DIRECT_TOOL_SPECIFICATIONS } from '@octocodeai/octocode-core/schema';
const runtime = vi.hoisted(() => ({
  initialize: vi.fn(),
  providers: vi.fn(),
  maintenance: vi.fn(),
}));
vi.mock('../../src/serverConfig.js', () => ({
  initialize: runtime.initialize,
}));
vi.mock('../../src/providers/factory.js', () => ({
  initializeProviders: runtime.providers,
}));
vi.mock('../../src/cacheMaintenance.js', () => ({
  runCacheMaintenanceIfDue: runtime.maintenance,
}));
import { executeDirectTool } from '../helpers/executeDirectTool.js';
import { prepareDirectToolInput } from '../helpers/prepareDirectToolInput.js';

beforeEach(() => vi.clearAllMocks());
describe('all canonical tools validate the complete envelope before runtime work', () => {
  it.each(ADAPTER_PARITY_CASES)(
    '$name preparation rejects unknown fields by default instead of changing the query',
    ({ name, query }) => {
      expect(() =>
        prepareDirectToolInput(name, [
          query,
          { ...query, unexpectedAuditField: true },
        ])
      ).toThrow(/unexpectedAuditField/);
    }
  );
  it.each(ADAPTER_PARITY_CASES)(
    '$name rejects a later malformed query without initialization',
    async ({ name, query }) => {
      const input = {
        queries: [query, { ...query, unexpectedAuditField: true }],
      };
      const result = await executeDirectTool(name, input);
      expect(result.isError).toBe(true);
      expect(JSON.stringify(result)).toContain('unexpectedAuditField');
      expect(runtime.initialize).not.toHaveBeenCalled();
      expect(runtime.providers).not.toHaveBeenCalled();
      expect(runtime.maintenance).not.toHaveBeenCalled();
    }
  );
  it.each(ADAPTER_PARITY_CASES)(
    '$name rejects empty and over-limit batches before initialization',
    async ({ name, query }) => {
      for (const queries of [[], Array.from({ length: 6 }, () => query)]) {
        expect((await executeDirectTool(name, { queries })).isError).toBe(true);
      }
      expect(runtime.initialize).not.toHaveBeenCalled();
      expect(runtime.providers).not.toHaveBeenCalled();
      expect(runtime.maintenance).not.toHaveBeenCalled();
    }
  );
  it.each(ADAPTER_PARITY_CASES)(
    '$name accepts the full supported batch in its executable schema',
    ({ name, query }) => {
      const schema = DIRECT_TOOL_SPECIFICATIONS.find(
        tool => tool.name === name
      )!.inputSchema;
      expect(
        schema.safeParse({ queries: Array.from({ length: 5 }, () => query) })
          .success
      ).toBe(true);
      expect(
        schema.safeParse({ queries: [query], unexpectedAuditField: true })
          .success
      ).toBe(false);
    }
  );
});
