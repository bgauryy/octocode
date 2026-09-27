// Queue state is session-scoped. Absence or malformed latest state is not idle.
export function grokInterviewIdle(events, sessionId) {
  if (typeof sessionId !== 'string' || !sessionId) return false;
  const state = events.findLast(event => event.method === '_x.ai/queue/changed'
    && event.params?.sessionId === sessionId)?.params;
  return Boolean(state && Array.isArray(state.entries) && state.entries.length === 0
    && state.runningPromptId == null && state.runningKind == null);
}

// Native hosts emit both plain strings and structured content blocks.
export function interviewEvidence(events, response) {
  const parts = [];
  let grokStream = '';
  const blocks = content => Array.isArray(content) ? content : [];
  for (const event of events) {
    if ((event.type === 'assistant' || event.type === 'message_end') && event.message?.role === 'assistant') {
      const content = event.message.content;
      if (typeof content === 'string') parts.push(content);
      else for (const block of blocks(content)) if (block?.type === 'text' && typeof block.text === 'string') parts.push(block.text);
    }
    if (event.method === 'item/completed' && event.params?.item?.type === 'agentMessage' && typeof event.params.item.text === 'string') parts.push(event.params.item.text);
    const update = event.params?.update;
    if (update?.sessionUpdate === 'agent_message_chunk' && update.content?.type === 'text' && typeof update.content.text === 'string') grokStream += update.content.text;
  }
  if (grokStream) parts.push(grokStream);
  for (const part of blocks(response?.parts)) if (part?.type === 'text' && typeof part.text === 'string') parts.push(part.text);
  const toolsUsed = events.some(event => event.type === 'tool_execution_start'
    || blocks(event.message?.content).some(block => ['tool_use', 'toolCall'].includes(block?.type))
    || (['item/started', 'item/completed'].includes(event.method) && ['mcpToolCall', 'commandExecution', 'fileChange'].includes(event.params?.item?.type))
    || ['tool_call', 'tool_call_update'].includes(event.params?.update?.sessionUpdate))
    || blocks(response?.parts).some(part => part?.type === 'tool');
  return {text: parts.join('\n').trim(), toolsUsed};
}
