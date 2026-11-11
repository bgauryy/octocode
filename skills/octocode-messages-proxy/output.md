# Delivery result

Name the transport, recipient and strongest observed state (discovered, queued, received, replied or blocked). Include the provider, model and reusable session ID when a relay was started. For a direct send give the vendor receipt or Octocode message ID and key. Quote the worker’s actual reply and its correlation ID when available. For A2A tasks include the task ID, context ID and server-reported state.

An empty inbox, elapsed timeout or accepted submission does not establish handling. Include any unresolved recipient, next required action, and any operator-carried hop. Return this in chat. If the user requests a saved receipt, use the named destination or `<workspace>/.octocode/octocode-messages-proxy/`.
