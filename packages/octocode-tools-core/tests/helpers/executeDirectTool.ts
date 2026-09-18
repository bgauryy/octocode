import { executeDirectTool as executeDirectToolRuntime } from '../../src/tools/directToolCatalog.exec.js';

export {
  _overrideInitialize,
  _resetInitialize,
} from '../../src/tools/directToolCatalog.exec.js';

const TEST_REASONING =
  'Exercise this public tool query while testing behavior unrelated to invocation metadata.';

/** Add required public-call metadata to legacy behavior fixtures without weakening production schemas. */
export async function executeDirectTool(
  name: string,
  input: unknown,
  options?: Parameters<typeof executeDirectToolRuntime>[2]
): ReturnType<typeof executeDirectToolRuntime> {
  if (!input || typeof input !== 'object') {
    return executeDirectToolRuntime(name, input, options);
  }
  const record = input as Record<string, unknown>;
  if (!Array.isArray(record.queries)) {
    return executeDirectToolRuntime(name, input, options);
  }
  return executeDirectToolRuntime(
    name,
    {
      ...record,
      queries: record.queries.map(query =>
        query && typeof query === 'object'
          ? { reasoning: TEST_REASONING, debug: true, ...query }
          : query
      ),
    },
    options
  );
}
