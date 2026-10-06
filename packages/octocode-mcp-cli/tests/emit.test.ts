import { mkdtemp, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import { emitTypeScript, renderTypeScript } from '../src/emit.js';
import { defineCli, defineCommand } from '../src/spec.js';

const evil = '"); throw new Error("pwned"); //';

describe('emitTypeScript', () => {
  it('writes JSON string literals and leaves the user module unchanged', async () => {
    const inputSchema = {
      type: 'object',
      properties: { note: { type: 'string', description: evil } },
      required: ['note'],
    };
    const spec = defineCli({
      name: 'issues',
      instructions: evil,
      commands: [
        defineCommand({
          name: 'search',
          description: evil,
          inputSchema,
          mcpName: 'issues.search',
          run: () => 'secret-handler',
        }),
        defineCommand({
          name: 'authored',
          description: evil,
          schema: z.object({
            query: z.string().describe(evil),
            limit: z.number().int(),
            ratio: z.number(),
            force: z.boolean().default(false),
            state: z.enum(['open', 'closed']),
            labels: z.array(z.string()),
            counts: z.array(z.number().int()),
            scores: z.array(z.number()),
            bits: z.array(z.boolean()),
            payload: z.unknown(),
            note: z.string().optional(),
            code: z.union([z.literal(1), z.literal(2)]),
            titled: z.string().describe('Title').default('x'),
          }),
          run: () => 'secret-handler',
        }),
      ],
    });
    const dir = await mkdtemp(join(tmpdir(), 'octocode-mcp-cli-'));
    const userPath = resolve('examples/user-cli.ts');
    const before = await readFile(userPath, 'utf8');
    const target = join(dir, 'nested', 'generated.ts');
    const written = await emitTypeScript(spec, target);
    await emitTypeScript(spec, target);
    expect(await readFile(userPath, 'utf8')).toBe(before);
    expect(await readFile(target, 'utf8')).toBe(written);
    expect(written).toBe(renderTypeScript(spec));
    expect(written).toContain(JSON.stringify(evil));
    expect(written).not.toContain('secret-handler');
    expect(written).toContain('z.number().int()');
    expect(written).toContain('z.number()');
    expect(written).toContain('z.boolean()');
    expect(written).toContain('z.enum(["open", "closed"])');
    expect(written).toContain('z.array(z.string())');
    expect(written).toContain('z.array(z.number().int())');
    expect(written).toContain('z.array(z.number())');
    expect(written).toContain('z.array(z.boolean())');
    expect(written).toContain('z.unknown()');
    expect(written).toContain('.default(false)');
    expect(written).toContain('.default("x")');
    expect(written).toContain('.optional()');
    expect(written).toContain('rawInputSchemas["issues.search"]');
    expect(written).toContain('schema: z.object({');
    expect(written.indexOf('schema: z.object({')).toBeLessThan(written.indexOf('inputSchema: rawInputSchemas["issues.search"]'));

    const js = written.replace(/^import .*$/gm, '').replace(/^export const /gm, 'const ');
    const evaluated = new Function(
      'defineCommand',
      'z',
      `${js}\nreturn { instructions, rawInputSchemas, mcpCommands, authoredCommands };`,
    )(defineCommand, z) as {
      instructions: string;
      mcpCommands: Array<{ description: string; source: string; schema?: unknown; flags: Array<{ description?: string }>; run: () => Promise<unknown> }>;
      authoredCommands: Array<{ description: string; flags: Array<{ property: string; kind: string }>; run: () => Promise<unknown> }>;
    };
    expect(evaluated.instructions).toBe(evil);
    expect(evaluated.mcpCommands[0]?.description).toBe(evil);
    expect(evaluated.mcpCommands[0]?.flags[0]?.description).toBe(evil);
    expect(evaluated.mcpCommands[0]).toMatchObject({ source: 'mcp' });
    expect(evaluated.mcpCommands[0]?.schema).toBeTypeOf('object');
    await expect(evaluated.mcpCommands[0]?.run()).rejects.toThrow('no handler');
    expect(evaluated.authoredCommands[0]?.description).toBe(evil);
    expect(evaluated.authoredCommands[0]?.flags.find(flag => flag.property === 'limit')?.kind).toBe('integer');
    await expect(evaluated.authoredCommands[0]?.run()).rejects.toThrow('no handler');
  });
});
