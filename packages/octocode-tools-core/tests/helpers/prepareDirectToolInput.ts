import { prepareDirectToolInput as prepareCanonicalInput } from '@octocodeai/octocode-core/schema';

const TEST_REASONING =
  'Prepare this public tool query while testing behavior unrelated to invocation metadata.';

/** Add required public-call metadata to legacy behavior fixtures without weakening production schemas. */
export function prepareDirectToolInput(
  toolName: Parameters<typeof prepareCanonicalInput>[0],
  query: Parameters<typeof prepareCanonicalInput>[1],
  options?: Parameters<typeof prepareCanonicalInput>[2]
): ReturnType<typeof prepareCanonicalInput> {
  const withMetadata = (candidate: unknown) =>
    candidate && typeof candidate === 'object'
      ? { reasoning: TEST_REASONING, debug: true, ...candidate }
      : candidate;
  const preparedQuery = Array.isArray(query)
    ? query.map(withMetadata)
    : withMetadata(query);
  return prepareCanonicalInput(toolName, preparedQuery, options);
}
