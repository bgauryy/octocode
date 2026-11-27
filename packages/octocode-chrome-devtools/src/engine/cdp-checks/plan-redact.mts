// A saved plan is evidence, not a replay script: text typed into a field can be a password or token,
// so the persisted copy keeps the step, the target and the value's length, never the characters.
const TYPED_ACTIONS = new Set(['fill', 'type']);

type Step = Record<string, unknown>;

export function redactPlanForEvidence<T>(plan: T): T {
  const copy = structuredClone(plan) as { steps?: Step[] };
  for (const step of Array.isArray(copy?.steps) ? copy.steps : [])
    if (
      step?.op === 'act' &&
      typeof step.action === 'string' &&
      TYPED_ACTIONS.has(step.action) &&
      typeof step.value === 'string'
    )
      step.value = `[redacted ${step.value.length} chars]`;
  return copy as T;
}

/** The strings a plan types into fields: what no persisted or printed output may repeat. */
export function typedValues(steps: readonly Step[]): string[] {
  return [
    ...new Set(
      steps.flatMap(step =>
        typeof step?.action === 'string' &&
        TYPED_ACTIONS.has(step.action) &&
        typeof step.value === 'string' &&
        step.value
          ? [step.value]
          : []
      )
    ),
  ].toSorted((a, b) => b.length - a.length);
}

/** Replaces every occurrence of a typed value inside any string of `value`, at any depth. */
export function scrubTyped<T>(value: T, secrets: readonly string[]): T {
  if (!secrets.length) return value;
  const walk = (node: unknown): unknown => {
    if (typeof node === 'string')
      return secrets.reduce(
        (text, secret) =>
          text.replaceAll(secret, `[redacted ${secret.length} chars]`),
        node
      );
    if (Array.isArray(node)) return node.map(walk);
    if (node && typeof node === 'object')
      return Object.fromEntries(
        Object.entries(node).map(([key, item]) => [key, walk(item)])
      );
    return node;
  };
  return walk(value) as T;
}
