// Where a raw CDP call runs (the page session or the browser connection) and which domains it needs
// enabled. Pure routing over injected senders, so it is tested without a browser.
//
// Chrome answers a method sent on the wrong connection with a recognisable non-executing error, so the
// router tries the page session first and retries once on the browser connection. Domains are enabled
// from Chrome's own words ("DOM agent needs to be enabled first"), not from a hand-written list.
import type { Payload } from './cdp-connection.mjs';
import { policyRefusal } from './cdp-policy.mjs';
import {
  docsUrl,
  enableInfo,
  nearestDomains,
  type Schema,
} from './cdp-schema.mjs';

export type Scope = 'auto' | 'page' | 'browser';
export type ProtocolError = { code: number; message: string };
export type Sender = (method: string, params: Payload) => Promise<Payload>;
/** One connection a call can run on; `enabled` is the set of domains this router enabled on it. */
export type Lane = {
  name: 'page' | 'browser';
  send: Sender;
  enabled: Set<string>;
};

export const protocolErrorOf = (error: unknown): ProtocolError | undefined => {
  const value =
    error instanceof Error && 'protocolError' in error
      ? error.protocolError
      : undefined;
  if (typeof value !== 'object' || value === null) return undefined;
  const { code, message } = value as Record<string, unknown>;
  return typeof code === 'number' && typeof message === 'string'
    ? { code, message }
    : undefined;
};

export type ScopeVerdict =
  /** -32601 naming the method sent: wrong connection, or unsupported by this Chrome. */
  | 'notFound'
  /** `Not allowed` or `only supported on the Browser target`: wrong connection. */
  | 'wrongTarget'
  /** Anything else: a failure of the call itself, forwarded as is. */
  | 'other';

/**
 * Judges a protocol error of `method`. The quoted name must EQUAL the method sent: an error that names
 * another method comes from a call Chrome forwarded internally and is a failure, not a routing signal.
 */
export function scopeVerdict(
  method: string,
  error: ProtocolError | undefined
): ScopeVerdict {
  if (!error) return 'other';
  if (error.code === -32601) {
    const quoted = /^'([A-Z]\w+\.\w+)' wasn't found$/.exec(error.message)?.[1];
    return quoted === method ? 'notFound' : 'other';
  }
  if (
    error.code === -32000 &&
    (error.message === 'Not allowed' ||
      /is only supported on the (B|b)rowser target$/.test(error.message))
  )
    return 'wrongTarget';
  return 'other';
}

const NEEDS_ENABLE: readonly RegExp[] = [
  /\b([A-Za-z]+) (?:agent|domain) (?:was not|is not|not|needs to be|has not been) enabled/i,
  /\b([A-Za-z]+) (?:must|should) be enabled/i,
  /^([A-Za-z]+) not enabled/i,
];
/** Wordings that name no domain: the method's own domain is the one to enable. */
const NEEDS_OWN_ENABLE: readonly RegExp[] = [
  /Virtual Authenticator Environment has not been enabled/i,
  /^Agent (?:is not|needs to be) enabled/i,
  /^Enable agent first/i,
];

const LIFECYCLE_NAMES = new Set(['enable', 'disable']);
/** Any wording of "this domain is not enabled": the domain is then the method's own. */
const NOT_ENABLED: readonly RegExp[] = [
  /(?:has not been|hasn't been|was not|is not|isn't|not) enabled/i,
  /must be enabled/i,
  /needs to be enabled/i,
  /agent was not enabled/i,
];

/** The domain Chrome says must be enabled first; undefined when the error is no such complaint. */
export function enableNeeded(
  method: string,
  error: ProtocolError | undefined,
  schema?: Schema
): string | undefined {
  if (!error || error.code === -32601) return undefined;
  const own = method.slice(0, method.indexOf('.'));
  if (NEEDS_OWN_ENABLE.some(pattern => pattern.test(error.message))) return own;
  for (const pattern of NEEDS_ENABLE) {
    const named = pattern.exec(error.message)?.[1];
    if (named === undefined) continue;
    if (!schema) return /^[A-Z]/.test(named) ? named : undefined;
    const domain = schema.domains.find(
      d => d.domain.toLowerCase() === named.toLowerCase()
    );
    if (domain) return domain.domain;
  }
  // The text names no domain (or no known one): the method's own domain is the one to enable, except
  // for the lifecycle methods themselves, which never ask for their own enable.
  const name = method.slice(method.indexOf('.') + 1);
  if (
    LIFECYCLE_NAMES.has(name) ||
    !NOT_ENABLED.some(p => p.test(error.message))
  )
    return undefined;
  return !schema || schema.domains.some(d => d.domain === own)
    ? own
    : undefined;
}

/** Domains the router never enables on its own: enabling them changes traffic or needs parameters. */
const MANUAL_ENABLE: Readonly<Record<string, string>> = {
  Fetch:
    'Fetch.enable pauses requests: call it yourself with the patterns to intercept',
};

/** The one-line way out when a domain stays disabled. */
export const enableHint = (domain: string) =>
  `hint: enable the domain in the same call: enable:["${domain}"] (or run it inside a plan)`;

export type RouterDeps = {
  schema?: Schema;
  /** The attached target's connection. */
  page: Sender;
  /** True when the attached target is the browser itself (`--browser`): there is no second scope. */
  pageIsBrowser?: boolean;
  /** The browser-level connection shared by every router of one run. */
  hub: BrowserHub;
  /** The installed Chrome's name for the unsupported message. */
  version?: () => Promise<string | undefined>;
  log?: (line: string) => void;
};

/** The one browser-level connection of a run, opened on first use and closed at the end. */
export class BrowserHub {
  private opening?: Promise<Lane>;
  private connection?: { close(): void };
  constructor(
    private readonly open: () => Promise<{ send: Sender; close(): void }>
  ) {}

  lane(): Promise<Lane> {
    this.opening ??= this.open().then(connection => {
      this.connection = connection;
      return { name: 'browser', send: connection.send, enabled: new Set() };
    });
    return this.opening;
  }

  /** Domains enabled on the browser connection so far (empty when it never opened). */
  async enabled(): Promise<ReadonlySet<string>> {
    return this.connection ? (await this.lane()).enabled : new Set();
  }

  close() {
    this.connection?.close();
    this.connection = undefined;
  }
}

export type RouteOutcome = {
  result: Payload;
  scope: 'page' | 'browser';
  /** Domains enabled by this call, in the order they were sent. */
  enabled: string[];
};

const errorWith = (
  message: string,
  from: unknown,
  extra: Record<string, unknown> = {}
) => {
  const error = new Error(message, { cause: from });
  const protocolError =
    from instanceof Error && 'protocolError' in from
      ? from.protocolError
      : undefined;
  return Object.assign(error, protocolError ? { protocolError } : {}, extra);
};

export class CdpRouter {
  readonly page: Lane;

  constructor(private readonly deps: RouterDeps) {
    this.page = { name: 'page', send: deps.page, enabled: new Set() };
  }

  /** The browser lane; a `--browser` target is its own browser lane. */
  private browser(): Promise<Lane> {
    return this.deps.pageIsBrowser
      ? Promise.resolve(this.page)
      : this.deps.hub.lane();
  }

  /** Enables `domain` on a lane with Chrome's own ordering: a refusal names what must come first. */
  private async enableOn(
    lane: Lane,
    domain: string,
    sent: string[],
    visiting = new Set<string>()
  ): Promise<boolean> {
    if (lane.enabled.has(domain)) return true;
    const method = `${domain}.enable`;
    if (this.deps.schema) {
      const info = enableInfo(
        this.deps.schema.domains.find(d => d.domain === domain) ?? {
          domain,
        }
      );
      if (!info.available || !info.bare) return false;
    }
    if (domain in MANUAL_ENABLE) return false;
    const refusal = policyRefusal(method, false);
    if (refusal) throw new Error(refusal);
    visiting.add(domain);
    try {
      await lane.send(method, {});
    } catch (error) {
      const first = enableNeeded(
        method,
        protocolErrorOf(error),
        this.deps.schema
      );
      if (!first || first === domain || visiting.has(first)) throw error;
      if (!(await this.enableOn(lane, first, sent, visiting))) throw error;
      await lane.send(method, {});
    }
    lane.enabled.add(domain);
    sent.push(domain);
    return true;
  }

  /** One send on one lane; Chrome's "enable X first" is answered once by enabling X and retrying. */
  private async sendOn(
    lane: Lane,
    method: string,
    params: Payload,
    sent: string[]
  ): Promise<Payload> {
    try {
      return await lane.send(method, params);
    } catch (error) {
      const needed = enableNeeded(
        method,
        protocolErrorOf(error),
        this.deps.schema
      );
      if (!needed) throw error;
      let enabled = false;
      try {
        enabled = await this.enableOn(lane, needed, sent);
      } catch (enableError) {
        this.deps.log?.(
          `[WARNING] enabling ${needed} for ${method} failed: ${enableError instanceof Error ? enableError.message : String(enableError)}`
        );
      }
      if (!enabled)
        throw errorWith(
          `${(error as Error).message}\n[HINT] ${needed} must be enabled first and this tool cannot do it alone: ${MANUAL_ENABLE[needed] ?? `send ${needed}.enable with its parameters (see protocol ${needed}.enable)`}\n${enableHint(needed)}`,
          error
        );
      try {
        return await lane.send(method, params);
      } catch (again) {
        // Still not enabled after one enable: say how to enable it with the call.
        if (enableNeeded(method, protocolErrorOf(again), this.deps.schema))
          throw errorWith(
            `${(again as Error).message}\n${enableHint(needed)}`,
            again
          );
        throw again;
      }
    }
  }

  private unsupported(method: string, from: unknown, version?: string) {
    const [domain, name] = method.split('.');
    return errorWith(
      `[UNSUPPORTED] ${method} is not supported by this Chrome${version ? ` (${version})` : ''}: the page and the browser connection both answered -32601 "wasn't found". A headless or custom build may not implement it. Docs: ${docsUrl(domain, 'method', name)}`,
      from
    );
  }

  /** Sends one call according to `scope`; returns where it ran and which domains it enabled. */
  async route(
    method: string,
    params: Payload,
    scope: Scope = 'auto'
  ): Promise<RouteOutcome> {
    const enabled: string[] = [];
    const run = async (lane: Lane) => ({
      result: await this.sendOn(lane, method, params, enabled),
      scope: lane.name,
      enabled,
    });
    if (scope === 'page' || this.deps.pageIsBrowser) {
      try {
        return await run(this.page);
      } catch (error) {
        if (
          this.deps.pageIsBrowser &&
          scopeVerdict(method, protocolErrorOf(error)) !== 'other'
        )
          throw errorWith(
            `${(error as Error).message}\n[HINT] this connection is the browser itself; page methods need a page target (drop --browser)`,
            error
          );
        throw error;
      }
    }
    if (scope === 'browser') return run(await this.browser());
    let first: unknown;
    try {
      return await run(this.page);
    } catch (error) {
      if (scopeVerdict(method, protocolErrorOf(error)) === 'other') throw error;
      first = error;
    }
    try {
      return await run(await this.browser());
    } catch (second) {
      const both =
        scopeVerdict(method, protocolErrorOf(first)) === 'notFound' &&
        scopeVerdict(method, protocolErrorOf(second)) === 'notFound';
      if (!both) throw second;
      throw this.unsupported(method, second, await this.deps.version?.());
    }
  }

  /**
   * Enables explicit domains in dependency order. Unknown names are refused with the nearest names;
   * domains without a parameterless enable are skipped and reported.
   */
  async enable(
    names: readonly string[],
    scope: Scope = 'auto'
  ): Promise<{ enabled: string[]; notes: string[] }> {
    const { schema } = this.deps;
    const sent: string[] = [],
      notes: string[] = [];
    const ordered = schema ? enableOrder(schema, names) : [...new Set(names)];
    for (const domain of ordered) {
      const info = schema
        ? enableInfo(schema.domains.find(d => d.domain === domain)!)
        : undefined;
      if (info && !info.available) {
        notes.push(`${domain} has no enable command; skipped`);
        continue;
      }
      if (info && !info.bare) {
        notes.push(
          `${domain}.enable needs parameters (${info.params
            .filter(p => !p.optional)
            .map(p => p.name)
            .join(', ')}); send it with cdp`
        );
        continue;
      }
      if (
        this.page.enabled.has(domain) ||
        (await this.deps.hub.enabled()).has(domain)
      )
        continue;
      const outcome = await this.route(`${domain}.enable`, {}, scope);
      (outcome.scope === 'page' ? this.page : await this.browser()).enabled.add(
        domain
      );
      sent.push(...outcome.enabled, domain);
    }
    return { enabled: [...new Set(sent)], notes };
  }

  /** Enables the domain of an event on the page connection so a listener sees it; never fails the caller. */
  async ensureEvents(
    event: string
  ): Promise<{ enabled: string[]; note?: string }> {
    const domain = event.slice(0, event.indexOf('.'));
    const sent: string[] = [];
    if (domain in MANUAL_ENABLE)
      return { enabled: [], note: `${event}: ${MANUAL_ENABLE[domain]}` };
    try {
      await this.enableOn(this.page, domain, sent);
    } catch (error) {
      return {
        enabled: sent,
        note: `${event}: enabling ${domain} failed: ${error instanceof Error ? error.message : String(error)}`,
      };
    }
    return { enabled: sent };
  }
}

/**
 * The requested domains ordered so a domain comes after the requested domains it depends on (the
 * schema's `dependencies`, followed through any domain). Unknown names throw with the nearest names.
 */
export function enableOrder(schema: Schema, names: readonly string[]) {
  const known = new Map(schema.domains.map(d => [d.domain, d]));
  const requested = new Set<string>();
  for (const name of names) {
    if (!known.has(name))
      throw new Error(
        `[ENABLE] ${name} is not a domain of the installed Chrome${nearestDomains(schema, name).length ? `; did you mean ${nearestDomains(schema, name).join(', ')}` : ''}. Use protocol * to list the domains.`
      );
    requested.add(name);
  }
  const ordered: string[] = [],
    seen = new Set<string>();
  const visit = (domain: string) => {
    if (seen.has(domain)) return;
    seen.add(domain);
    for (const dependency of known.get(domain)?.dependencies ?? [])
      visit(dependency);
    if (requested.has(domain)) ordered.push(domain);
  };
  for (const name of requested) visit(name);
  return ordered;
}
