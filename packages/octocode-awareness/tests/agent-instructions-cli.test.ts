import { expect, it } from 'vitest';
import { getAwarenessAgentInstructions } from '../src/index.js';
import { executeAwarenessCli } from '../src/command-cli.js';

it('renders exactly the imported canonical instructions without identity or a store', async () => {
  expect(await executeAwarenessCli(['instructions'])).toEqual({ exitCode: 0, payload: null, text: getAwarenessAgentInstructions() });
  expect(await executeAwarenessCli(['instructions', '--section', 'feedback', '--section', 'observe'])).toEqual({
    exitCode: 0, payload: null, text: getAwarenessAgentInstructions({ sections: ['observe', 'feedback'] }),
  });
});

it('rejects misspelled sections and unsupported instruction flags', async () => {
  for (const args of [['--section', 'typo'], ['--section'], ['--unknown', 'true'], ['extra']]) {
    expect((await executeAwarenessCli(['instructions', ...args])).exitCode).toBe(1);
  }
});
