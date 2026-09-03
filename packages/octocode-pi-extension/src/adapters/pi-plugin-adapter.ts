import {
  RuntimeFailure,
  type AgentEventType,
  type LifecycleBus,
  type LifecycleDecision,
  type LifecycleSubscription,
} from '@octocodeai/agent-core';

export interface PiPluginEventSubscription<T extends Record<string, unknown>> {
  readonly event: AgentEventType;
  readonly id: string;
  readonly priority?: number;
  readonly handler: LifecycleSubscription<T>['handler'];
}

/** Event-driven plugin subscriptions are removable even though Pi tool/command APIs are not. */
export class PiPluginEventAdapter {
  readonly #owned = new Map<string, Array<() => void>>();

  constructor(private readonly buses: ReadonlyMap<AgentEventType, LifecycleBus<Record<string, unknown>>>) {}

  subscribe(owner: string, subscription: PiPluginEventSubscription<Record<string, unknown>>): void {
    const bus = this.buses.get(subscription.event);
    if (!bus) throw new RuntimeFailure('unsupported-capability', `Pi does not expose canonical event ${subscription.event}`);
    const dispose = bus.subscribe({
      id: `plugin:${owner}:${subscription.id}`,
      source: 'plugin',
      ...(subscription.priority === undefined ? {} : { priority: subscription.priority }),
      handler: subscription.handler,
    });
    const owned = this.#owned.get(owner) ?? [];
    owned.push(dispose);
    this.#owned.set(owner, owned);
  }

  unload(owner: string): void {
    for (const dispose of [...(this.#owned.get(owner) ?? [])].reverse()) dispose();
    this.#owned.delete(owner);
  }

  assertTransactionalContribution(kind: 'hook' | 'tool' | 'command' | 'resource' | 'mcp' | 'setting' | 'prompt' | 'ui' | 'model'): void {
    if (kind === 'hook') return;
    throw new RuntimeFailure(
      'unsupported-capability',
      `Pi cannot transactionally unload ${kind} contributions; activate them only after a host reload boundary`,
    );
  }
}

export type PiPluginLifecycleDecision = LifecycleDecision<Record<string, unknown>>;
