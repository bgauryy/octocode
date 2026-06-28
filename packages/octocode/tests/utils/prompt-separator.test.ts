import { describe, expect, it } from 'vitest';

import { separatorChoice } from '../../src/utils/prompt-separator.js';

describe('separatorChoice', () => {
  it('creates a typed prompt separator boundary value', () => {
    const separator = separatorChoice<{ name: string; value: 'back' }>();

    expect(separator).toBeDefined();
    expect(typeof separator).toBe('object');
  });
});
