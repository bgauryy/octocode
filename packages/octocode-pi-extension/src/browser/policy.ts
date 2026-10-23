/** The page-request policy contract, shared by the policy (`network.ts`), its enforcement (`page-network.ts`) and the session (`session-state.ts`, `session.ts`). */

/** What the page's network traffic showed that the audit should record. */
export type NetworkEvent =
  | { kind: 'blocked'; url: string; resourceType: string; reason: string }
  | { kind: 'rebinding'; url: string; address: string }
  | { kind: 'websocket'; url: string; reason: string };

/** Who made a request: the tab it belongs to (a tab's own, its frames' and its dedicated workers'), and for a worker that lives per origin the origin it lives at. */
export interface RequestSource {
  tab?: string;
  origin?: string;
}

/** What a page may reach: the contract `PagePolicy` (`network.ts`) implements and `page-network.ts` enforces on every paused request. */
export interface PageNetworkPolicy {
  /** Resolves when the page may request `url`; rejects, with the reason, when it may not. */
  check(url: string, resourceType: string, source: RequestSource): Promise<void>;
  /** The main frame of tab `tab` committed `url`. */
  navigated(tab: string, url: string): void;
  /** Tab `popup` was opened by tab `opener`: it holds the approval the opener holds. */
  opened(opener: string, popup: string): void;
  /** Tab `tab` closed. */
  closed(tab: string): void;
  /** Whether a response from `address` (and `port`) for `url` shows Chrome connected to an address `check` did not allow. */
  rebinds(url: string, address: string, port?: number): boolean;
  /** Whether an approval of the private host `hostname:port` is in force for some tab: what the launched Chrome's filtering proxy lets through. */
  approves(hostname: string, port: number): boolean;
  report(event: NetworkEvent): void;
}
