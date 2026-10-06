import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock the interactive backend so no test renders a real inquirer prompt.
const { selectMock } = vi.hoisted(() => ({ selectMock: vi.fn() }));
vi.mock('@inquirer/prompts', () => ({ select: selectMock }));

import { select } from '../../src/utils/prompts.js';

describe('select', () => {
  const config = {
    message: 'Pick one',
    choices: [
      { name: 'Option A', value: 'a' },
      { name: 'Option B', value: 'b' },
    ],
  };

  beforeEach(() => {
    selectMock.mockReset();
  });

  it('forwards the config to the underlying select prompt', async () => {
    selectMock.mockResolvedValue('a');
    const result = await select(config);
    expect(selectMock).toHaveBeenCalledTimes(1);
    expect(selectMock).toHaveBeenCalledWith(config);
    expect(result).toBe('a');
  });

  it('propagates the underlying prompt rejection (e.g. user cancel)', async () => {
    const cancel = new Error('User force closed the prompt');
    selectMock.mockRejectedValue(cancel);
    await expect(select(config)).rejects.toBe(cancel);
  });
});
