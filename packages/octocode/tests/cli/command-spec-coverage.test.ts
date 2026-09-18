import { describe, expect, it } from 'vitest';

import {
  REGISTERED_COMMAND_NAMES,
  loadCommand,
} from '../../src/cli/commands/index.js';
import {
  COMMAND_SPECS,
  findCommandSpec,
} from '../../src/cli/commands/specs.js';

describe('Node command contract', () => {
  it('has one help spec for every Node-owned command and no native-command specs', () => {
    expect(COMMAND_SPECS.map(spec => spec.name)).toEqual(
      REGISTERED_COMMAND_NAMES
    );
  });

  it('keeps runtime options documented with matching value semantics', async () => {
    for (const name of REGISTERED_COMMAND_NAMES) {
      const command = await loadCommand(name);
      const spec = findCommandSpec(name);
      expect(command).toBeDefined();
      expect(spec).toBeDefined();
      const documented = new Map(
        spec!.options?.map(option => [option.name, option])
      );
      for (const option of command!.options ?? []) {
        expect(documented.has(option.name), `${name} --${option.name}`).toBe(
          true
        );
        expect(Boolean(documented.get(option.name)?.hasValue)).toBe(
          Boolean(option.hasValue)
        );
      }
    }
  });

  it('keeps the skill help contract complete and unique', () => {
    const spec = findCommandSpec('skill')!;
    expect(spec.usage).toMatch(/^skill /);
    expect(spec.description).toBeTruthy();
    expect(spec.scheme?.length).toBeGreaterThan(0);
    expect(spec.examples?.length).toBeGreaterThan(0);
    const names = spec.options?.map(option => option.name) ?? [];
    expect(new Set(names).size).toBe(names.length);
  });
});
