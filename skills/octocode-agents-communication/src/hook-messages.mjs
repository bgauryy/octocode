/**
 * Messages from a `hook {"format":"json"}` batch. Items carry only IDs; bodies are in
 * the rendered context, whose envelopes may name a sender instead of repeating its ID,
 * so each named envelope gets the last ID seen for that name.
 */
export function hookMessages(batch) {
  if (!batch.context || !batch.items?.length) return [];
  const ids = new Map();
  return JSON.parse(batch.context.slice(batch.context.indexOf('\n') + 1)).map(message => {
    if (message.sender) ids.set(message.from, message.sender);
    return { ...message, sender: message.sender ?? ids.get(message.from) };
  });
}
