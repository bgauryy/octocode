/** The delivery surface of a bound store and of the interface layer that runs delivery loops and hooks. */
import type { AttachInput, CommandResults, HostInboundData, RecordUsageInput, RetryDeliveryInput } from './catalog-types.generated.js';
import type { Offered } from './rows.js';
import type { StoreHost } from './store-types.js';

/** The delivery operations of a bound store. `Store` forwards each to the function of the same name in `dispatch.ts` or `dispatch-outcomes.ts`. */
export interface DeliveryApi {
  attach(session: string, input: AttachInput): CommandResults['attach'];
  attachment(session: string): CommandResults['attach'];
  present(session: string, managed?: boolean): Promise<void>;
  hostInbound(session: string): HostInboundData | null;
  claudeInbound(session: string, endpoint: string, ownChild?: boolean): ClaudeInbound;
  stage(session: string, mode: string, actionOnly?: boolean | null): Staged;
  stageBound(session: string, mode: string, binding?: CommandResults['attach'] | null, actionOnly?: boolean | null): Staged;
  hasDispatchable(session: string, actionOnly: boolean): boolean;
  finishDispatch(session: string, items: readonly Dispatched[], error?: string | null): void;
  releaseDispatch(session: string, items: readonly Dispatched[], reason: string): void;
  retryDelivery(session: string, input: RetryDeliveryInput): CommandResults['retry_delivery'];
  recordUsage(session: string, input: RecordUsageInput): CommandResults['record_usage'];
  ensureBindingChangeAllowed(session: string): void;
  reconcileCompletedOrphan(session: string): void;
}

export type DeliveryStore = StoreHost & DeliveryApi;

export type ClaudeInbound = NonNullable<CommandResults['attach']['inbound']>;
/** What a dispatch row is addressed by: the message and the token of the offer. */
export type Dispatched = Pick<Offered, 'id' | 'dispatchToken'>;

/** What the long-running and hook entry points need from their interface layer (cli.ts supplies it). */
export interface DeliveryRuntime {
  /** One compact JSON line on stdout, null members stripped. */
  output(value: unknown): void;
  /** `text` and a newline on stdout. */
  print(text: string): void;
  open(path: string, workspace: string, options?: { readOnly?: boolean; create?: boolean }): DeliveryStore;
}

/** The staged rows; an empty batch may carry the `reason` it is empty for. */
export type Staged = Offered[] & { reason?: string };
