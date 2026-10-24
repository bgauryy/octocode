import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import octocode from '../src/index.js';
import { fakeCtx, fakePi } from './fake-pi.js';
import { tmp } from './helpers.js';

/**
 * Pi validates tool arguments against the schema and refuses a call that breaks a bound, echoing every argument back:
 * the model pays for its input twice and retries. So the extension's schemas describe shape (types, enums, required
 * fields) and never size or range; the tools normalize at runtime (clamp numbers, clip display labels, spill long text).
 */
const BOUNDS = ['maxLength', 'minLength', 'maxItems', 'minItems', 'maximum', 'minimum', 'exclusiveMaximum', 'exclusiveMinimum', 'pattern', 'maxProperties', 'minProperties'];

function boundsIn(schema: unknown, at: string, found: string[]): string[] {
  if (Array.isArray(schema)) schema.forEach((item, index) => boundsIn(item, `${at}[${index}]`, found));
  else if (schema && typeof schema === 'object') {
    for (const [key, value] of Object.entries(schema)) {
      if (BOUNDS.includes(key)) found.push(`${at}.${key}`);
      boundsIn(value, `${at}.${key}`, found);
    }
  }
  return found;
}

let saved: NodeJS.ProcessEnv;
beforeEach(() => {
  saved = { ...process.env };
  process.env['OCTOCODE_HOME'] = tmp();
});
afterEach(() => {
  process.env = saved;
});

/** Every registered tool of a main session or a subagent, as the fake host sees them after startup. */
async function registeredTools(subagent: boolean): Promise<Array<{ name: string; description: string; parameters: unknown; promptGuidelines?: string[] }>> {
  if (subagent) process.env['OCTOCODE_SUBAGENT'] = '1';
  const fake = fakePi();
  Object.assign(fake.pi, { getSessionName: () => undefined });
  await octocode(fake.pi);
  const ctx = fakeCtx({ cwd: tmp(), hasUI: !subagent });
  Object.assign(ctx.ui, { onTerminalInput: () => () => undefined });
  await fake.fire('session_start', { type: 'session_start', reason: 'startup' }, ctx);
  return fake.pi.getAllTools();
}

/** Guidelines ride on every request: a few short lines per tool; the description owns the detail. */
const MAX_GUIDELINES = 2;
const MAX_GUIDELINE_CHARS = 250;

describe('tool schemas', () => {
  for (const subagent of [false, true]) {
    it(`carry no size or range bounds (${subagent ? 'subagent' : 'main session'})`, async () => {
      const tools = await registeredTools(subagent);
      expect(tools.length).toBeGreaterThan(5);
      const found = tools.flatMap((tool) => boundsIn(tool.parameters, tool.name, []));
      expect(found).toEqual([]);
    });
  }

  it('keep every tool prompt lean: a few short guidelines, never more in a subagent, no double spaces', async () => {
    const main = new Map((await registeredTools(false)).map((tool) => [tool.name, tool]));
    const child = await registeredTools(true);
    for (const tool of [...main.values(), ...child]) {
      const guidelines = tool.promptGuidelines ?? [];
      expect(guidelines.length, tool.name).toBeLessThanOrEqual(MAX_GUIDELINES);
      for (const line of guidelines) expect(line.length, `${tool.name}: ${line}`).toBeLessThanOrEqual(MAX_GUIDELINE_CHARS);
      expect([tool.description, ...guidelines].join(' '), tool.name).not.toMatch(/ {2}/);
    }
    for (const tool of child) expect((tool.promptGuidelines ?? []).length, tool.name).toBeLessThanOrEqual((main.get(tool.name)?.promptGuidelines ?? []).length);
  });
});
