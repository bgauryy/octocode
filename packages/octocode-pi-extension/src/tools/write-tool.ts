/**
 * Octocode `write` — same-name override of Pi's built-in write.
 * Adds path-guard (home + ALLOWED_PATHS + cwd/tmp) and records read-state
 * so a subsequent `edit` stale-check can see the fresh bytes.
 */
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import type { TSchema, ToolCallResult, ToolDefinition, PiTheme } from '../types.js';
import { makeRenderer, truncateToWidth } from './render-helpers.js';
import { assertPathAllowed } from './path-guard.js';
import { recordFileReadState, withFileMutationQueue } from './edit-tool.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];

function resolveWritePath(filePath: string, cwd = process.cwd()): string {
  return path.isAbsolute(filePath) ? filePath : path.resolve(cwd, filePath);
}

function validateWriteParams(params: Record<string, unknown>): { path: string; content: string } {
  // Pi render path accepts file_path; fold it for compatibility.
  const rawPath = params['path'] ?? params['file_path'];
  if (typeof rawPath !== 'string' || rawPath.trim().length === 0) {
    throw new Error('Write tool input is invalid. path must be a non-empty string.');
  }
  if (typeof params['content'] !== 'string') {
    throw new Error('Write tool input is invalid. content must be a string.');
  }
  return { path: rawPath, content: params['content'] };
}

export function registerWriteTool(
  pi: { registerTool?(def: ToolDefinition): void },
  Type: TypeBoxBuilder,
): void {
  const parameters = Type.Object(
    {
      path: Type.String({ description: 'Path to the file to write (relative or absolute).' }),
      content: Type.String({ description: 'Content to write to the file.' }),
    },
    { additionalProperties: false },
  ) as TSchema;

  pi.registerTool?.({
    name: 'write',
    label: 'write (Octocode)',
    description:
      'Octocode custom write tool. Replaces Pi built-in write with the same create/overwrite semantics plus Octocode path-guard (working directory, home, OS temp, ALLOWED_PATHS) and post-write read-state recording for the edit stale-check. Prefer edit for surgical changes to existing files.',
    promptSnippet: 'Create or overwrite files with Octocode path-guard.',
    promptGuidelines: [
      'Octocode custom write replaces Pi built-in write; use write only for new files or intentional full rewrites.',
      'Prefer the edit tool for targeted replacements in existing files — write overwrites without an oldText match guard.',
      'Paths must stay inside the working directory, home directory, OS temp dir, or ALLOWED_PATHS.',
      'Do not use bash/cat redirection for ordinary file creates when write is available.',
    ],
    parameters,
    prepareArguments(args: unknown) {
      if (!args || typeof args !== 'object') return args;
      const input = args as Record<string, unknown>;
      if (typeof input['path'] !== 'string' && typeof input['file_path'] === 'string') {
        return { ...input, path: input['file_path'] };
      }
      return args;
    },
    async execute(
      _toolCallId: string,
      params: Record<string, unknown>,
      signal?: AbortSignal,
      _onUpdate?: unknown,
      ctx?: { cwd?: string },
    ): Promise<ToolCallResult> {
      const { path: requestPath, content } = validateWriteParams(params);
      const cwd = ctx?.cwd ?? process.cwd();
      const absolutePath = resolveWritePath(requestPath, cwd);
      assertPathAllowed(absolutePath, cwd, 'write');
      if (signal?.aborted) throw new Error('Operation aborted');

      await withFileMutationQueue(absolutePath, async () => {
        if (signal?.aborted) throw new Error('Operation aborted');
        await mkdir(path.dirname(absolutePath), { recursive: true });
        if (signal?.aborted) throw new Error('Operation aborted');
        await writeFile(absolutePath, content, 'utf8');
        if (signal?.aborted) throw new Error('Operation aborted');
        await recordFileReadState(absolutePath, cwd);
      });

      return {
        content: [
          {
            type: 'text',
            text: `Successfully wrote ${content.length} bytes to ${requestPath}`,
          },
        ],
        details: {
          path: requestPath,
          absolutePath,
          bytes: content.length,
        },
      };
    },
    renderCall(args: unknown, theme?: PiTheme) {
      const input = args && typeof args === 'object' ? (args as Record<string, unknown>) : {};
      const filePath =
        typeof input['path'] === 'string'
          ? input['path']
          : typeof input['file_path'] === 'string'
            ? input['file_path']
            : '(missing path)';
      const content = typeof input['content'] === 'string' ? input['content'] : '';
      const lines = content.length === 0 ? 0 : content.split('\n').length;
      const title = theme?.fg('toolTitle', theme.bold('write')) ?? 'write';
      const suffix =
        theme?.fg('dim', `${filePath} · ${lines} line${lines === 1 ? '' : 's'}`) ??
        `${filePath} · ${lines} line${lines === 1 ? '' : 's'}`;
      return makeRenderer((width) => [truncateToWidth(`${title} ${suffix}`, width)]);
    },
    renderResult(result: ToolCallResult, opts: { expanded?: boolean; isPartial?: boolean }, theme?: PiTheme) {
      if (opts.isPartial) {
        const prog = theme?.fg('warning', '… writing') ?? '… writing';
        return makeRenderer(() => [prog]);
      }
      if (!result.isError) {
        return makeRenderer(() => ['']);
      }
      const text = result.content.find((c) => c.type === 'text')?.text ?? 'write failed';
      const err = theme?.fg('error', text) ?? text;
      return makeRenderer((width) => [truncateToWidth(err, width)]);
    },
  });
}
