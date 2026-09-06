import { describe, expect, it } from 'vitest';
import {
  APPROVAL_CLASSES,
  DEFAULT_OCTOCODE_PROMPT_MODE,
  PERMISSION_LEVELS,
  PROMPT_MODES,
} from '../src/protocols.js';

describe('shared protocol enums', () => {
  it('publishes stable prompt and permission choices', () => {
    expect(PROMPT_MODES).toEqual(['append', 'octocode-first']);
    expect(DEFAULT_OCTOCODE_PROMPT_MODE).toBe('octocode-first');
    expect(PERMISSION_LEVELS).toEqual(['strict', 'default', 'relaxed']);
    expect(APPROVAL_CLASSES).toContain('fs-delete');
  });
});

