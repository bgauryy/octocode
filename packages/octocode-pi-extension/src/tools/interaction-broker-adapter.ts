import type { PiContext } from '../types.js';
import {
  drainInteractionContinuations,
  listPendingInteractions,
  submitHostInteractionAnswer,
  type HostInteractionAnswerV1,
  type InteractionContinuationDrainResult,
  type InteractionContinuationV1,
} from './interaction-broker.js';

export interface RegisteredInteractionBrokerAdapter {
  listPending(ctx: PiContext): ReturnType<typeof listPendingInteractions>;
  submitAnswer(ctx: PiContext, answer: HostInteractionAnswerV1): ReturnType<typeof submitHostInteractionAnswer>;
  drain(ctx: PiContext): Promise<InteractionContinuationDrainResult>;
}

/** Host registration boundary; the parent runtime owns the concrete RPC route. */
export interface InteractionBrokerAdapterRegistry {
  registerInteractionBrokerAdapter(adapter: RegisteredInteractionBrokerAdapter): void;
}

export interface InteractionContinuationHost {
  deliver(continuation: InteractionContinuationV1, prompt: string, ctx: PiContext): void | Promise<void>;
}

/**
 * Register one production adapter shared by RPC submission and session-resume
 * hooks. The host should call drain on startup/resume and after submitAnswer.
 */
export function registerInteractionBrokerAdapter(
  registry: InteractionBrokerAdapterRegistry,
  host: InteractionContinuationHost,
): RegisteredInteractionBrokerAdapter {
  const adapter: RegisteredInteractionBrokerAdapter = {
    listPending: (ctx) => listPendingInteractions(ctx),
    submitAnswer: (ctx, answer) => submitHostInteractionAnswer(ctx, answer),
    drain: (ctx) => drainInteractionContinuations(
      ctx,
      (continuation) => host.deliver(continuation, formatInteractionContinuationPrompt(continuation), ctx),
    ),
  };
  registry.registerInteractionBrokerAdapter(adapter);
  return adapter;
}

export function formatInteractionContinuationPrompt(continuation: InteractionContinuationV1): string {
  const answer = continuation.answer;
  const value = answer.cancelled
    ? 'The user cancelled this question.'
    : answer.optionIds?.length
      ? `The user selected option(s): ${answer.optionIds.join(', ')}.`
      : answer.text
        ? `The user answered: ${answer.text}`
        : 'The user submitted an empty structured answer.';
  return [
    `[interaction-continuation id=${continuation.continuationId} interaction=${continuation.interactionId} correlation=${continuation.correlationId}]`,
    value,
    'Resume the suspended turn from this answer. Do not ask the same question again unless validation fails.',
  ].join('\n');
}
