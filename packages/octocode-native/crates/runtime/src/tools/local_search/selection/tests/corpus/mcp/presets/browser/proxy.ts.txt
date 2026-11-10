import { join } from "node:path";
import type { CallToolResult } from "../../client/index.ts";
import { createMcpToolName } from "../../tool-names.ts";
import {
	type BlockedConnection,
	type FilteringProxy,
	type FilteringProxyOptions,
	PROXY_ALL_LOOPBACK,
	startFilteringProxy,
} from "../../../../common/src/browser-proxy.ts";
import { hostKey, portOf } from "../../../../common/src/browser-network.ts";
import { browserMcpUrls } from "../../../../common/src/browser-mcp.ts";
import { hostBrowser } from "../../host.ts";
import { browserStateDir, recordBrowser, recordedProxyPort } from "./owned.ts";
import { restrictWebRtc as restrictProfile } from "../../../../common/src/browser-webrtc.ts";
import { classifyHost, type HostLookup, isHttpUrl } from "../../../../common/src/web-guard.ts";
import { BROWSER_SERVER } from "../../../../common/src/mcp-names.ts";
import { isRecord } from "@octocodeai/octocode-contracts/guards";

/**
 * Page-started requests of the browser server's Chrome. The package launches its own Chrome and has no proxy of its own,
 * but its `open` tool passes `args` to the launcher, whose `--proxyServer` and `--proxyBypassList` flags override any
 * proxy setting in a config file. So the browser server's `open` is rewritten to start Chrome behind the filtering proxy
 * the native `browser` tool uses (`browser/proxy.ts`), which runs in this process. Every request, redirect, frame and
 * WebSocket of every page then passes it: a private, local or unresolvable target is refused with 403 unless the agent
 * opened that host and port itself and the permission check allowed it.
 *
 * The proxy starts with the first `open` and stops on session shutdown. If it is gone while Chrome still runs, Chrome's
 * proxy setting points at a closed port, so page loads fail rather than go direct. Chrome outlives the process that
 * opened it (a browser subagent's follow-up runs in a new process and `open` reuses the browser), so the proxy port each
 * Chrome was given is recorded beside the package's browser state; a later process that reuses that Chrome starts its
 * filter on the same port again (approvals start empty: fail closed). A Chrome that Octocode did not start behind the
 * filter keeps the settings it started with; `open` then says so. The top session stops a headless Chrome it still owns
 * when it shuts down (`./owned.ts`).
 */

const PROXY_OPTIONS = new Set(["--proxyServer", "--proxyBypassList", "--proxyPacUrl"]);
const DEFAULT_PORT = "9222";
const BLOCKED_NOTES_MAX = 20;
const profileDir = (cwd: string, port: string): string => join(browserStateDir(cwd), `cdp-chrome-profile-${port}`);

/** How an `open` relates to the filter: a new Chrome behind it, a reused one whose filter runs (or was restored), or one without it. */
type OpenFilter = "started" | "filtered" | "restored" | "unfiltered";

/** A browser tool call ready to send, and how to present what came back. */
export interface GuardedBrowserCall {
	/** The arguments to send to the server: `open` gains the proxy flags; every other call is unchanged. */
	args: Record<string, unknown>;
	/** Adds the notes the call earned (an attached browser is not filtered; page requests that were blocked) to its result. */
	review(result: CallToolResult): CallToolResult;
}

export interface BrowserGuard {
	/**
	 * Prepares one call to the browser server's tool `tool` (`open`, `run`, ...). Call it from the tool's `execute`: it
	 * runs after the permission check allowed the call, and the private hosts among the call's own URLs that check asked
	 * about become reachable to the pages it loads. Throws, and sends nothing, when the proxy cannot start.
	 *
	 * Calls are prepared one at a time, in the order they were made. The server runs requests in arrival order, and `open`
	 * is slow to prepare (a port probe, the proxy start) while other calls are not, so parallel calls (`open`, then
	 * `snapshot`) would otherwise arrive reversed. A call prepares after the one before it has been handed on (a macrotask
	 * later, so that request is written first). A call that fails, or whose `signal` aborts while it waits, frees its place.
	 */
	call(tool: string, args: Record<string, unknown>, cwd: string, signal?: AbortSignal): Promise<GuardedBrowserCall>;
	/** Stops the proxy and forgets the approvals; safe when it never started and when called again. */
	dispose(): Promise<void>;
}

export interface BrowserGuardOptions {
	/** Resolves host names, for hosts no permission check saw; the `web` tool's resolver by default. */
	lookup?: HostLookup;
	proxy?: Pick<FilteringProxyOptions, "lookup" | "env" | "connect">;
	/** Starts the proxy; a seam for tests. */
	start?: (options: FilteringProxyOptions) => Promise<FilteringProxy>;
	/** Whether something already answers on the CDP `port`; a seam for tests. */
	running?: (port: string) => Promise<boolean>;
}

const stringList = (value: unknown): string[] | undefined =>
	value === undefined
		? []
		: Array.isArray(value) && value.every((item) => typeof item === "string")
			? value
			: undefined;

/** `args` without the launcher's proxy options and their values: a duplicate option is an error, and the model's choice must not win. */
function withoutProxyOptions(args: readonly string[]): string[] {
	const kept: string[] = [];
	for (let index = 0; index < args.length; index++) {
		if (PROXY_OPTIONS.has(args[index])) index++;
		else kept.push(args[index]);
	}
	return kept;
}

const flagValue = (args: readonly string[], flag: string): string | undefined =>
	args.includes(flag) ? args[args.indexOf(flag) + 1] : undefined;

/**
 * `open`'s typed fields as launcher arguments. The package refuses typed fields beside `args`, and the proxy flags can
 * only travel in `args`, so a typed call is rewritten whole: `port` and `url` take a value, `headless: true` is a bare flag.
 */
function typedAsArgs(args: Record<string, unknown>): { rest: Record<string, unknown>; launcher: string[] } {
	const { port, url, headless, ...rest } = args;
	const launcher: string[] = [];
	if (typeof port === "number" || typeof port === "string") launcher.push("--port", String(port));
	if (typeof url === "string") launcher.push("--url", url);
	if (headless === true) launcher.push("--headless");
	return { rest, launcher };
}

async function answers(port: string): Promise<boolean> {
	try {
		return (await fetch(`http://127.0.0.1:${port}/json/version`, { signal: AbortSignal.timeout(750) })).ok;
	} catch {
		return false;
	}
}

export function createBrowserGuard(options: BrowserGuardOptions = {}): BrowserGuard {
	const approved = new Set<string>();
	const blocked = new Set<string>();
	/** Listeners by requested port; `0` is the session's own proxy on a free port. */
	const listeners = new Map<number, Promise<FilteringProxy>>();

	const onBlocked = ({ host }: BlockedConnection) => {
		if (blocked.size < BLOCKED_NOTES_MAX) blocked.add(host);
	};
	const running = options.running ?? answers;

	function start(port = 0): Promise<FilteringProxy> {
		let listener = listeners.get(port);
		if (!listener) {
			listener = (options.start ?? startFilteringProxy)({
				...options.proxy,
				...(port ? { port } : {}),
				onBlocked,
				approved: (hostname, hostPort) => approved.has(hostKey(hostname, hostPort)),
			}).catch((error: unknown) => {
				listeners.delete(port);
				throw error;
			});
			listeners.set(port, listener);
		}
		return listener;
	}

	/** Whether this process already listens on `port` (its own proxy, or one it restored). */
	async function listening(port: number): Promise<boolean> {
		for (const listener of listeners.values()) if ((await listener.catch(() => undefined))?.port === port) return true;
		return false;
	}

	/** A running Chrome on CDP `port`: its recorded filter runs here, is restored here, runs elsewhere, or never existed. */
	async function reuseFilter(cwd: string, port: string): Promise<OpenFilter> {
		const proxyPort = recordedProxyPort(cwd, port);
		if (proxyPort === undefined) return "unfiltered";
		if (await listening(proxyPort)) return "filtered";
		try {
			await start(proxyPort);
			return "restored";
		} catch (error) {
			// Taken: another Octocode process's filter for this Chrome is still running.
			if ((error as NodeJS.ErrnoException).code === "EADDRINUSE") return "filtered";
			throw error;
		}
	}

	/** The private hosts among the call's own URLs that the permission check asked about (or, with no check, that classify private now). */
	async function approveOpened(tool: string, args: Record<string, unknown>): Promise<void> {
		for (const raw of browserMcpUrls(createMcpToolName(BROWSER_SERVER, tool), args)) {
			let url: URL;
			try {
				url = new URL(raw);
			} catch {
				continue;
			}
			if (!isHttpUrl(url)) continue; // A file: or data: page is allowed to open; what it loads is still filtered.
			const key = hostKey(url.hostname, portOf(url));
			const asked = hostBrowser().mcpAsked(key);
			if (asked ?? (await classifyHost(url, options.lookup)).kind !== "public") approved.add(key);
		}
	}

	/** Chrome reads `webrtc.ip_handling_policy` from the profile: without it a page's WebRTC sends UDP past the proxy. */
	async function restrictWebRtc(cwd: string, port: string): Promise<void> {
		if (await running(port)) return; // Chrome would overwrite the file, and `open` reuses it.
		restrictProfile(profileDir(cwd, port));
	}

	/** Ends when the latest call has been handed on. */
	let handedOn: Promise<void> = Promise.resolve();

	return {
		call(tool, args, cwd, signal) {
			const before = handedOn;
			let handOn!: () => void;
			handedOn = new Promise<void>((resolve) => {
				handOn = () => void setImmediate(resolve);
			});
			const turn = new Promise<void>((resolve, reject) => {
				const cancelled = () => signal?.reason ?? new Error("The browser call was cancelled.");
				if (signal?.aborted) return reject(cancelled());
				const abort = () => reject(cancelled());
				signal?.addEventListener("abort", abort, { once: true });
				void before.then(() => {
					signal?.removeEventListener("abort", abort);
					resolve();
				});
			});
			const prepared = turn.then(() => prepare(tool, args, cwd));
			// A cancelled call keeps its place until the call before it is done, so the later ones stay in order.
			const handOnAfterBefore = () => {
				void before.then(handOn);
			};
			void prepared.then(handOn, handOnAfterBefore);
			return prepared;
		},
		async dispose() {
			const closing = [...listeners.values()];
			listeners.clear();
			approved.clear();
			blocked.clear();
			await Promise.all(closing.map((listener) => listener.then((proxy) => proxy.close()).catch(() => undefined)));
		},
	};

	async function prepare(tool: string, args: Record<string, unknown>, cwd: string): Promise<GuardedBrowserCall> {
		await approveOpened(tool, args);
		if (tool === "dev-check") await requireFilteredBrowser(cwd, args.port);
		if (tool !== "open") return { args, review: (result) => review(result) };
		const listed = stringList(args.args);
		if (listed === undefined) return { args, review: (result) => review(result) }; // The server rejects it.
		const { rest, launcher } = typedAsArgs(args);
		const given = [...launcher, ...listed];
		const port = flagValue(given, "--port") ?? DEFAULT_PORT;
		const isolated = !given.includes("--profile") && !given.includes("--dry-run");
		// A reused Chrome ignores launch flags: restore the filter it was started behind instead.
		const reuse = isolated && (await running(port)) ? await reuseFilter(cwd, port) : undefined;
		// A reused browser already behind the filter ignores launch flags, and the package warns about them: send none.
		if (reuse === "filtered" || reuse === "restored") {
			const proxyPort = recordedProxyPort(cwd, port);
			if (proxyPort !== undefined) recordBrowser(cwd, port, proxyPort);
			return { args: { ...rest, args: withoutProxyOptions(given) }, review: (result) => review(result, reuse) };
		}
		const listener = await start();
		if (isolated && !reuse) {
			await restrictWebRtc(cwd, port);
			recordBrowser(cwd, port, listener.port);
		}
		return {
			args: {
				...rest,
				args: [...withoutProxyOptions(given), "--proxyServer", listener.url, "--proxyBypassList", PROXY_ALL_LOOPBACK],
			},
			review: (result) => review(result, reuse ?? "started"),
		};
	}

	/**
	 * `dev-check` launches a Chrome of its own, without launcher flags, when none answers on its port: that Chrome would
	 * not run behind the filter. It may only reuse a browser `open` started behind it.
	 */
	async function requireFilteredBrowser(cwd: string, requested: unknown): Promise<void> {
		const port = typeof requested === "number" || typeof requested === "string" ? String(requested) : DEFAULT_PORT;
		const filter = (await running(port)) ? await reuseFilter(cwd, port) : undefined;
		if (filter === "filtered" || filter === "restored") return;
		throw new Error(
			`dev-check needs a browser Octocode opened on port ${port}: call open with port ${port} first, then dev-check reuses it behind Octocode's private-address filter.`,
		);
	}

	function review(result: CallToolResult, filter?: OpenFilter): CallToolResult {
		const notes: string[] = [];
		if (filter && reusedBrowser(result)) {
			if (filter === "restored")
				notes.push(
					"Octocode filter: reused the running browser and restored its private-address filter; hosts approved in an earlier run must be opened again.",
				);
			else if (filter === "unfiltered" || filter === "started")
				notes.push(
					"Octocode filter: this browser was already running and was not started behind Octocode's private-address filter, so its page requests are not filtered. Call cleanup for this port, then open again for a filtered browser.",
				);
		}
		if (blocked.size > 0) {
			notes.push(
				`Octocode filter: page requests to ${[...blocked].join(", ")} were refused (403) because they are local or private addresses the agent did not open. Open such a host yourself, and have the user approve it, to let its page use it.`,
			);
			blocked.clear();
		}
		return notes.length === 0
			? result
			: { ...result, content: [...(result.content ?? []), { type: "text", text: notes.join("\n") }] };
	}
}

/** Whether an `open` result says the launcher attached to a running browser instead of starting one. */
function reusedBrowser(result: CallToolResult): boolean {
	const found = (value: unknown, depth = 0): boolean =>
		isRecord(value) &&
		(value.reused === true || (depth < 4 && Object.values(value).some((child) => found(child, depth + 1))));
	return found(result.structuredContent);
}

let shared: BrowserGuard | undefined;

/** The session's guard: one proxy for the browser server, shared by the tool and the preset's `dispose`. */
export const browserGuard = (): BrowserGuard => (shared ??= createBrowserGuard());

/**
 * The one helper the browser tool calls: `const call = await guardBrowserCall(tool.name, params, ctx.cwd)`, send
 * `call.args` to the server, and return `call.review(result)`.
 */
export const guardBrowserCall = (
	tool: string,
	args: Record<string, unknown>,
	cwd: string,
	signal?: AbortSignal,
): Promise<GuardedBrowserCall> => browserGuard().call(tool, args, cwd, signal);
