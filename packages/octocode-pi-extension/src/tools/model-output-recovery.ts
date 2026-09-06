import type { NotifyFn, PiContext, PiInstance } from '../types.js';

export const OUTPUT_LIMIT_RECOVERY_PROMPT =
  'The previous assistant response hit the model maximum output limit. Assume any incomplete tool call did not run. Retry only the unfinished next action now. Keep pre-tool explanation to one sentence, use the smallest unique edit anchor, split large mutations into separate calls, and return a concise result.';

interface AssistantMessage {
  role?: string;
  stopReason?: string;
}

interface AgentEndEvent {
  messages?: unknown[];
  willRetry?: boolean;
}

function lastAssistantMessage(messages: unknown[] | undefined): AssistantMessage | undefined {
  if (!Array.isArray(messages)) return undefined;
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (message && typeof message === 'object' && (message as AssistantMessage).role === 'assistant') {
      return message as AssistantMessage;
    }
  }
  return undefined;
}

/**
 * Recover once when a provider truncates model output before a tool call finishes.
 * Pi owns context-overflow retries (`willRetry:true`); this only handles the
 * separate maximum-output-token failure and never executes partial tool input.
 */
export function registerModelOutputRecovery(pi: PiInstance, notify: NotifyFn): void {
  const handledMessages = new WeakSet<object>();
  let recoveryAttempts = 0;

  const rearm = (): void => {
    recoveryAttempts = 0;
  };

  pi.on('agent_end', async (rawEvent: AgentEndEvent, ctx: PiContext) => {
    const message = lastAssistantMessage(rawEvent.messages);
    if (!message) return;

    const identity = message as object;
    if (handledMessages.has(identity)) return;
    handledMessages.add(identity);

    if (message.stopReason !== 'length') {
      rearm();
      return;
    }
    if (rawEvent.willRetry === true) return;

    if (recoveryAttempts >= 1) {
      notify(
        ctx,
        'Model output limit reached again. Automatic retry stopped; continue in smaller steps or raise the model maximum output tokens.',
        'warning',
      );
      return;
    }

    recoveryAttempts += 1;
    notify(ctx, 'Model output limit reached; retrying once with a smaller response and tool call.', 'warning');
    queueMicrotask(() => {
      try {
        pi.sendUserMessage(OUTPUT_LIMIT_RECOVERY_PROMPT, { deliverAs: 'followUp' });
      } catch (error) {
        notify(ctx, `Could not queue output-limit recovery: ${(error as Error)?.message ?? String(error)}`, 'error');
      }
    });
  });

  pi.on('input', async (event: { source?: string }) => {
    if (event.source === 'interactive' || event.source === 'rpc') rearm();
  });
  pi.on('session_start', async () => rearm());
  pi.on('session_shutdown', async () => rearm());
}
