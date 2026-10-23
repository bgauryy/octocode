// Schema-only reasoning for the cdp-all sweep: placeholder params from the installed
// protocol and reply classification. No per-method knowledge.

// Schema-driven placeholder params: no per-method knowledge.
export function placeholders(protocol) {
  const types = new Map();
  for (const domain of protocol.domains)
    for (const type of domain.types ?? [])
      types.set(domain.domain + '.' + type.id, type);
  const value = (spec, domain, seen) => {
    if (spec.$ref) {
      const ref = spec.$ref.includes('.')
        ? spec.$ref
        : domain + '.' + spec.$ref;
      const type = types.get(ref);
      if (!type || seen.has(ref)) return {};
      return value(type, ref.split('.')[0], new Set([...seen, ref]));
    }
    if (spec.enum?.length) return spec.enum[0];
    switch (spec.type) {
      case 'string':
        return '';
      case 'integer':
      case 'number':
        return 0;
      case 'boolean':
        return false;
      case 'array':
        return [];
      case 'object':
        return required(spec.properties, domain, seen);
      default:
        return {};
    }
  };
  const required = (fields = [], domain, seen) =>
    Object.fromEntries(
      fields
        .filter(field => !field.optional)
        .map(field => [field.name, value(field, domain, seen)])
    );
  return (domain, command) =>
    required(command.parameters, domain.domain, new Set());
}

export function classify(error, replied) {
  if (replied) return { class: 'exception', error };
  const protocolError = error.match(/^CDP error \[(-?\d+)\]: ([^]*)$/);
  if (protocolError) {
    const code = Number(protocolError[1]);
    return {
      class: code === -32601 ? 'not-found' : 'chrome-error',
      code,
      error: protocolError[2],
    };
  }
  if (/CDP timeout|step deadline/.test(error))
    return { class: 'timeout', error };
  if (
    /WebSocket|Session (already )?closed|closed unexpectedly|detached from its target/i.test(
      error
    )
  )
    return { class: 'closed', error };
  return { class: 'engine', error };
}
