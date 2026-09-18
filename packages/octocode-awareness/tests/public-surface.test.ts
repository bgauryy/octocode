import { expect, it } from 'vitest';

it('exports only the canonical routine runtime from the package root', async () => {
  const api = await import('../src/index.js');
  expect(Object.keys(api).sort()).toEqual([
    'AWARENESS_AGENT_INSTRUCTION_SECTIONS',
    'AWARENESS_CONCEPTS',
    'AWARENESS_MESSAGE_PARAMETER_GUIDANCE',
    'ROUTINE_AWARENESS_OPERATIONS',
    'createAwarenessClient',
    'getAwarenessAgentInstructions',
    'getAwarenessOperationDescriptor',
    'listAwarenessOperationDescriptors',
  ]);
});
