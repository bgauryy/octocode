import assert from 'node:assert/strict';

// Verify the immutable revision actually requested, including its publisher/hash.
export function verifyContributionReads(requests, recordsByAgent) {
  return requests.map(request => {
    const names = new Set(request.body.match(/[a-z0-9][a-z0-9._-]*\.md\b/g) ?? []);
    const published = new Map((recordsByAgent.get(request.sender) ?? [])
      .filter(record => record.name === 'share_document' && record.value?.document?.author === request.sender)
      .map(record => [record.value.document.name, record.value.document]));
    const candidates = [...published.values()].filter(document => names.has(document.name));
    assert.equal(candidates.length, 1, `Request ${request.id} must reference exactly one document published by its sender`);
    const document = candidates[0];
    assert.ok(document.sha256, `Request ${request.id} needs a publication hash`);
    const read = (recordsByAgent.get(request.recipient) ?? []).some(record => {
      const value = record.value;
      return record.name === 'read_document' && value?.document?.name === document.name
        && value.document.author === request.sender && value.document.sha256 === document.sha256
        && typeof value.content === 'string' && value.content.includes('COPPER');
    });
    assert.ok(read, `${request.recipient} must read ${document.name} for request ${request.id}`);
    return { request: request.id, author: request.sender, reader: request.recipient, name: document.name, sha256: document.sha256 };
  });
}

export function nativeResults(agent) {
  const records = [];
  const decode = result => {
    for (const block of result?.content ?? []) { try { return JSON.parse(block.text); } catch {} }
  };
  if (agent.vendor === 'codex') for (const event of agent.rpc.events) {
    const item = event.params?.item;
    if (event.method === 'item/completed' && item?.type === 'mcpToolCall' && item.status === 'completed' && !item.error && !item.result?.isError) records.push({name: item.tool, value: decode(item.result)});
  }
  if (agent.vendor === 'grok') for (const event of agent.rpc.events) {
    const update = event.params?.update, result = update?.rawOutput;
    if (update?.status === 'completed' && result?.type === 'MCP' && result.server_name === 'communication') {
      try { records.push({name: result.tool_name, value: JSON.parse(result.output.OkayOutput)}); } catch {}
    }
  }
  if (agent.vendor === 'claude') {
    const blocks = agent.process.events.flatMap(event => event.message?.content ?? []);
    const names = new Map(blocks.filter(block => block.type === 'tool_use').map(block => [block.id, block.name.split('__').at(-1)]));
    for (const block of blocks) if (block.type === 'tool_result' && !block.is_error && names.has(block.tool_use_id)) records.push({name: names.get(block.tool_use_id), value: decode(block)});
  }
  return records;
}
