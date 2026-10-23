import type { AddressInfo } from 'node:net';
import { fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { parseSessionEntries, type ExtensionContext, type ExtensionToolContext, type SessionEntry, type SessionMessageEntry, type SessionProjection } from '@earendil-works/pi-coding-agent';

type AgentMessage = SessionMessageEntry['message'];
/** A tool result as the session holds it. */
export type ToolResultMessage = Extract<AgentMessage, { role: 'toolResult' }>;
/** The JSON arguments of a tool call. */
export type ToolArguments = Parameters<typeof fauxToolCall>[1];

/**
 * A test double for `T` that implements only the members a test exercises. `Partial<T>` checks every member given
 * against `T`, so a misspelled or mistyped member fails typecheck; a member left out is absent at run time, so a call
 * the test did not expect throws instead of passing silently.
 */
export function stub<T extends object>(members: Partial<T>): T {
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the one place a partial double stands in for a full interface; the members given are still checked against T.
  return members as T;
}

/** The address a server listening on a TCP port is bound to; throws for a pipe or a server not listening. */
export function addressOf(server: { address(): string | AddressInfo | null }): AddressInfo {
  const address = server.address();
  if (address === null || typeof address === 'string') throw new Error(`expected a TCP address, got ${String(address)}`);
  return address;
}

/** The text of a text content block; throws for an image block. */
export function textOf(block: { type: string; text?: string }): string {
  if (block.type !== 'text' || typeof block.text !== 'string') throw new Error(`expected a text block, got ${block.type}`);
  return block.text;
}

/** An empty session message from the user or the assistant, for code that reads only a branch's entry ids and roles. */
export function messageEntry(id: string, role: 'user' | 'assistant'): SessionMessageEntry {
  const message = role === 'user' ? { role, content: '', timestamp: 0 } : fauxAssistantMessage('');
  return { type: 'message', id, parentId: null, timestamp: '', message };
}

/** `ctx` as the context a tool's `execute` gets, with no tools to call: a nested call rejects. */
export function toolContext<T extends ExtensionContext>(ctx: T): T & ExtensionToolContext {
  return Object.assign(ctx, { tools: [], executeTool: () => Promise.reject(new Error('this test runs no nested tools')) });
}

/** The assistant message that calls tool `name` with `args` under the call id `id`. */
export function toolCallMessage(id: string, name: string, args: ToolArguments): AgentMessage {
  return fauxAssistantMessage(fauxToolCall(name, args, { id }));
}

/** A successful text result of the tool call `id`, at `timestamp`; `extra` overrides any field. */
export function toolResultMessage(id: string, toolName: string, text: string, extra: Partial<ToolResultMessage> = {}): ToolResultMessage {
  return { role: 'toolResult', toolCallId: id, toolName, content: [{ type: 'text', text }], isError: false, timestamp: 0, ...extra };
}

/** A session in `cwd` whose context shows `messages`, each from the entry `id`, as a reload projects it. */
export function projectedSession(cwd: string, messages: ReadonlyArray<{ id: string; message: AgentMessage }>): Pick<ExtensionContext, 'cwd' | 'sessionManager'> {
  const entries = messages.map(({ id, message }) => ({ sourceEntry: stub<SessionEntry>({ id }), messages: [message] }));
  return { cwd, sessionManager: stub<ExtensionContext['sessionManager']>({ getBranch: () => [], buildSessionProjection: () => stub<SessionProjection>({ entries }) }) };
}

/**
 * A double for a method generic in its result, such as CDP `send` and `evaluate`, whose caller names the type of what
 * the browser returns: `answer` gives that value for each call.
 */
export function answering<A extends unknown[]>(answer: (...args: A) => unknown): <T>(...args: A) => Promise<T> {
  return async <T>(...args: A): Promise<T> => {
    const value = await answer(...args);
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the real method trusts the browser for T in the same way; the double states the browser's answer.
    return value as T;
  };
}

/**
 * `entries` as Pi reads them from a session file: JSON lines parsed without checking their shape. For code that must
 * cope with whatever a session file holds, so a test can hand it entries no typed session would build.
 */
export function storedEntries(entries: readonly unknown[]): SessionEntry[] {
  return parseSessionEntries(entries.map((entry) => JSON.stringify(entry)).join('\n')).filter((entry): entry is SessionEntry => entry.type !== 'session');
}
