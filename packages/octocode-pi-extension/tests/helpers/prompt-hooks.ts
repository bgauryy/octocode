/** Match Pi hook composition: observers returning nothing must not erase a prompt result. */
export function composeBeforeAgentStart(handlers: Map<string, Array<(event: unknown, ctx: unknown) => unknown | Promise<unknown>>>) {
  return async (event: unknown, ctx: unknown): Promise<Record<string, unknown> | undefined> => {
    let result: Record<string, unknown> | undefined;
    let current = event;
    for (const handler of handlers.get('before_agent_start') ?? []) {
      const value = await handler(current, ctx);
      if (value && typeof value === 'object') {
        result = { ...result, ...value };
        if (typeof (value as Record<string, unknown>)['systemPrompt'] === 'string') current = { ...(current as object), systemPrompt: (value as Record<string, unknown>)['systemPrompt'] };
      }
    }
    return result;
  };
}
