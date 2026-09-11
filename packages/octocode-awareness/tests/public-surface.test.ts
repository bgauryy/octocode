import { expect, it } from 'vitest';

it('exports only the canonical routine runtime from the package root', async () => {
  const api = await import('../src/index.js');
  expect(Object.keys(api).sort()).toEqual([
    'AWARENESS_CONCEPTS',
    'ROUTINE_AWARENESS_OPERATIONS',
    'createAwarenessClient',
    'getAwarenessOperationDescriptor',
    'listAwarenessOperationDescriptors',
  ]);
});
