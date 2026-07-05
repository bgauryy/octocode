/**
 * Context session-management tools:
 * compact_context | clear_context
 *
 * IMPORTANT — session-control APIs (ctx.newSession, ctx.reload) are ONLY
 * available in ExtensionCommandContext (registerCommand handlers). They are
 * NOT exposed to tool execute() contexts and will always be undefined there.
 */
import type { PiContext, PiCommandContext, PiInstance, ToolDefinition } from '../types.js';
import type { registerUniqueTool } from './octocode-tools.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type RegisterFn = typeof registerUniqueTool;
type Notifier = (ctx: PiContext | undefined, msg: string, level?: string) => void;

const AUTO_COMPACT_THRESHOLD = 0.80;

export function registerContextTools(
  pi: PiInstance,
  Type: TypeBoxBuilder,
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
  notify: Notifier,
): void {
  let lastAutoCompactTokens: number | null = null;

  if (pi.on) {
    pi.on('turn_end', (_event, ctx) => {
      const usage = (ctx as unknown as PiContext).getContextUsage?.();
      if (!usage) return;
      const fill = usage.tokens / usage.contextWindow;
      const prevFill = lastAutoCompactTokens !== null
        ? lastAutoCompactTokens / usage.contextWindow
        : null;
      lastAutoCompactTokens = usage.tokens;
      if (fill < AUTO_COMPACT_THRESHOLD) return;
      if (prevFill !== null && prevFill >= AUTO_COMPACT_THRESHOLD) return;

      const pctStr = `${Math.round(fill * 100)}%`;
      if ((ctx as unknown as PiContext).hasUI) {
        (ctx as unknown as PiContext).ui?.notify?.(
          `Auto-compacting: context at ${pctStr} of context window.`,
          'info',
        );
      }
      (ctx as unknown as PiContext).compact?.({
        onComplete: () => {
          if ((ctx as unknown as PiContext).hasUI) {
            (ctx as unknown as PiContext).ui?.notify?.('Auto-compaction complete.', 'info');
          }
        },
        onError: (error: Error) => {
          if ((ctx as unknown as PiContext).hasUI) {
            (ctx as unknown as PiContext).ui?.notify?.(
              `Auto-compaction failed: ${error.message}`,
              'error',
            );
          }
        },
      });
    });
  }

  if (pi.registerCommand) {
    pi.registerCommand('_octocode-clear-context-impl', {
      description: '[internal] Start a new session — invoked by the clear_context tool.',
      handler: async (_args, ctx: PiCommandContext) => {
        if (!ctx.newSession) {
          notify(ctx, 'clear_context: ctx.newSession not available in this runtime.', 'error');
          return;
        }
        const result = await ctx.newSession();
        if (result?.cancelled) {
          notify(ctx, 'clear_context: session switch was cancelled.', 'warning');
        }
      },
    });
  }

  registerFn(pi, registeredToolNames, {
    name: 'compact_context',
    label: 'Compact Context',
    description:
      'Compact conversation history to free context window space. Call autonomously when context is ≥ 60 % full AND the next task is large, at a research→execution boundary, or when an unrelated task starts mid-session.',
    promptSnippet: 'Compact conversation history to free context window space',
    parameters: Type.Object({
      instructions: Type.Optional(
        Type.String({
          description:
            'Optional focus instructions for the compaction summary (e.g. "focus on recent file changes").',
        }),
      ),
    }),
    async execute(
      _toolCallId: string,
      params: Record<string, unknown>,
      _signal?: AbortSignal,
      _onUpdate?: unknown,
      ctx?: PiContext,
    ) {
      if (!ctx?.compact) {
        throw new Error('compact_context: ctx.compact is not available in this runtime. Use /compact manually.');
      }

      const continuation =
        'Compaction is complete. Continue from the compacted context. Re-orient if needed, then proceed with the user task.';

      ctx.compact({
        customInstructions: params['instructions'] as string | undefined,
        onComplete: () => {
          if (ctx.hasUI) {
            ctx.ui?.notify?.('Compaction completed. Continuing from the compacted context.', 'info');
          }
          pi.sendUserMessage(continuation, { deliverAs: 'followUp' });
        },
        onError: (error: Error) => {
          if (ctx.hasUI) {
            ctx.ui?.notify?.(`Compaction failed: ${error.message}`, 'error');
          }
        },
      });

      return {
        content: [
          {
            type: 'text' as const,
            text: 'Compaction triggered. The agent will continue after the summary is saved.',
          },
        ],
      };
    },
  } satisfies ToolDefinition);

  registerFn(pi, registeredToolNames, {
    name: 'clear_context',
    label: 'Clear Context',
    description:
      'Start a new session with no prior context. Call autonomously when the next task is unrelated to the current conversation.',
    promptSnippet: 'Start a new session with no prior context',
    parameters: Type.Object({}),
    async execute(
      _toolCallId: string,
      _params: Record<string, unknown>,
      _signal?: AbortSignal,
    ) {
      pi.sendUserMessage('/_octocode-clear-context-impl', { deliverAs: 'followUp' });
      return {
        content: [
          {
            type: 'text' as const,
            text: 'New session queued. The context will be cleared after this turn completes.',
          },
        ],
      };
    },
  } satisfies ToolDefinition);
}
