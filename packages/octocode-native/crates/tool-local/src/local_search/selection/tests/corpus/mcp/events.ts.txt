/** Session events: load and connect servers, declare their tools per prompt, wait for them, and shut down. */

import { isDeepStrictEqual } from "node:util";
import { presetEntries } from "./preset.ts";
import { applyServerChanges, reloadMcpConfig } from "./reload.ts";
import { hostPaths } from "./host.ts";
import type { McpContext } from "./host/types.ts";
import { type McpServerEntry, type LoadedMcpConfig, loadMcpConfig } from "./config.ts";
import { reconnect, reconnectSignedIn, startConnection, waitForDirectServers, waitForServers } from "./connections.ts";
import { registerMcpHookCaller } from "./hook-caller.ts";
import { registerMcpReload, registerMcpSwitch, switchMcpServer } from "./switch.ts";
import { reportProblems } from "./report.ts";
import { say, sayUntrustedProject } from "./notice.ts";
import { loadMcpRuntime } from "./runtime.lazy.ts";
import { mcpNamespace } from "../../common/src/mcp-names.ts";
import { isSelected } from "./exposure.ts";
import { deferralOf } from "./deferral.ts";
import {
	MCP_STATUS_SECTION,
	MCPS_SECTION,
	type McpServerListing,
	mcpServerPurposes,
	renderMcpStatusSection,
	renderMcpsSection,
} from "./section.ts";
import { errorMessage } from "@octocodeai/octocode-contracts/guards";
import { isEnabled, type McpServer } from "./server.ts";
import type { McpExtensionState } from "./state.ts";
import { registerTools } from "./tool-registry.ts";
import { takeLaunchWarnings } from "./warnings.ts";

/** Whether the launch the server would get now differs from the one it got; a launch that cannot be prepared does not count. */
function launchChanged(state: McpExtensionState, server: McpServer, launched: McpServerEntry): boolean {
	try {
		return !isDeepStrictEqual(launched, state.prepareLaunch(server.entry, state.sessionCwd));
	} catch {
		return false;
	}
}

/**
 * The servers the prompt describes: enabled servers this session's selection reaches. Each block's content is what
 * `McpPresentation.describe` fixed when the prompt first described the server's catalog, so a reconnect, a failure or
 * a dropped connection changes only the state, which `mcp_status` carries.
 */
function serverListings(state: McpExtensionState): McpServerListing[] {
	const activeTools = state.api.getActiveTools();
	const active = new Set(activeTools);
	// Name order, so an `auto:<bytes>` budget fills the same way whichever server connected first.
	const listed = state.servers
		.filter((server) => isEnabled(server) && !server.unselected)
		.sort((a, b) => (a.entry.name < b.entry.name ? -1 : a.entry.name > b.entry.name ? 1 : 0));
	// A server that leaves the list gets a fresh description if it comes back.
	state.presentation.retainDescribed(new Set(listed.map((server) => server.entry.name)));
	return listed.flatMap((server) => {
		const declared = [...(state.serverTools.get(server.entry.name) ?? [])]
			.filter((name) => active.has(name) && state.definitions.get(name)?.exposure === "direct")
			.map((name) => state.toolOwners.get(name)?.split("\0")[1] ?? name);
		const connectionState = server.connection?.isStopped ? "stopped" : (server.connection?.state ?? "connecting");
		const described = state.presentation.describe(
			server.entry.name,
			declared,
			server.connection?.instructions,
			connectionState === "connected",
			deferralOf(server.entry, declared, state.exposureSetting(), activeTools, state.deferRoles()),
		);
		const undeclared = [...described.tools].filter((tool) => !declared.includes(tool));
		const deferred = described.deferred.size;
		return [
			{
				entry: server.entry,
				state: connectionState,
				declared: described.tools.size > 0,
				...(described.contract ? { contract: described.contract } : {}),
				pending: described.pending,
				...(undeclared.length > 0 ? { undeclared } : {}),
				...(deferred > 0 ? { defers: true, deferred: deferred === described.tools.size } : {}),
			},
		];
	});
}

/**
 * Write the `mcps` base section, the server purposes of the `tools` overview (`mcpServers`) and the `mcp_status`
 * section for one request. Describing comes first: a thin
 * declaration needs the block that describes it (`McpPresentation.isThin`), so connected servers re-register after.
 */
function describeServers(
	state: McpExtensionState,
	options: {
		baseSections: Record<string, string>;
		sections: Record<string, string>;
		mcpServers?: Record<string, string>;
	},
): void {
	const listings = serverListings(state);
	for (const connection of state.connections()) if (connection.state === "connected") registerTools(state, connection);
	options.mcpServers = mcpServerPurposes(listings);
	const mcps = renderMcpsSection(listings, { declared: state.presentation.declared });
	if (mcps) options.baseSections[MCPS_SECTION] = mcps;
	else delete options.baseSections[MCPS_SECTION];
	const status = renderMcpStatusSection(listings, { canManage: state.api.getActiveTools().includes("mcp") });
	if (status) options.sections[MCP_STATUS_SECTION] = status;
	else delete options.sections[MCP_STATUS_SECTION];
}

function defaultLoadConfig(ctx: McpContext, defaults: readonly McpServerEntry[]): LoadedMcpConfig {
	return loadMcpConfig({
		agentDir: hostPaths().agentDir(),
		cwd: ctx.cwd,
		projectTrusted: ctx.isProjectTrusted(),
		defaults,
	});
}

export function registerSessionEvents(state: McpExtensionState): void {
	const { api, options } = state;

	api.on("session_start", (_event, ctx) => {
		state.presentation.reset();
		const load = () => (options.loadConfig ?? defaultLoadConfig)(ctx, presetEntries(options.presets ?? []));
		const loaded = load();
		state.configErrors = loaded.errors;
		state.projectConfig = loaded.projectConfig;
		state.globalConfig = loaded.globalConfig;
		state.projectTrusted = ctx.isProjectTrusted();
		sayUntrustedProject(ctx);
		state.waitedForStartup = false;
		state.sessionCwd = ctx.cwd;
		state.modelRegistry = ctx.modelRegistry;
		state.session = new AbortController();
		const { signal } = state.session;
		state.unregisterSwitch?.();
		state.unregisterSwitch = registerMcpSwitch(api.events, (request) => switchMcpServer(state, request));
		state.unregisterReload?.();
		state.unregisterReload = registerMcpReload(api.events, () => state.track(reloadMcpConfig(state, ctx, load)));
		state.unregisterHookCaller?.();
		state.unregisterHookCaller = registerMcpHookCaller(api.events, async (request) => {
			const callSignal = AbortSignal.any([signal, request.signal]);
			callSignal.throwIfAborted();
			const server = state.findServer(request.server);
			if (!server || !isEnabled(server)) throw new Error(`MCP server "${request.server}" is unavailable`);
			// A server outside the tool selection starts for the hook alone; its tools stay undeclared.
			if (server.unselected && !server.connection) void state.track(startConnection(state, server, undefined, false));
			if (request.waitForConnection || server.unselected) await waitForServers(state, [server], callSignal);
			callSignal.throwIfAborted();
			const connection = server.connection;
			if (!connection || connection.state !== "connected")
				throw new Error(`MCP server "${request.server}" is not connected`);
			// The model path's checks: a hidden or no longer offered tool is not callable, and the host authorizes
			// the registered tool name (its permission gate) before the call reaches the server.
			const owner = `${server.entry.name}\0${request.tool}`;
			const toolName = [...state.toolOwners].find(([, value]) => value === owner)?.[0];
			if (
				!toolName ||
				state.toolExposure(server.entry, request.tool) === "hidden" ||
				!connection.tools.some((offered) => offered.name === request.tool)
			)
				throw new Error(`MCP tool "${request.server}/${request.tool}" is not available`);
			await request.authorize({ toolName, input: request.input });
			callSignal.throwIfAborted();
			// Through the connection's request path like the model's calls: expired-session retry, sign-in marking and
			// retiring-client accounting.
			// A progress handler sends a progress token, so a server's progress keeps the idle timeout from expiring.
			return connection.callTool(request.tool, request.input, {
				signal: callSignal,
				timeoutMs: connection.timeoutMs,
				onProgress: () => undefined,
			});
		});
		state.configuredEntries = loaded.servers;
		const registered = state.registeredServers();
		state.overridden = registered.overridden;
		state.servers = [
			...loaded.servers
				.filter(
					(entry) => !registered.servers.some((server) => mcpNamespace(server.entry.name) === mcpNamespace(entry.name)),
				)
				.filter((entry) => !state.leavesOut(entry.name))
				.map((entry) => ({ entry })),
			...registered.servers,
		];
		for (const server of state.servers)
			server.unselected = isEnabled(server) && !isSelected(state.api, server.entry, state.taskNamed);
		state.emitChange();
		const enabled = state.servers.filter((server) => isEnabled(server) && !server.unselected);
		if (enabled.length === 0) {
			reportProblems(state, ctx);
			return;
		}
		// The MCP client loads only now, so sessions without servers never pay for it. Waiting one
		// event loop turn lets the first render happen before loading and connecting.
		const runtime = new Promise((resolve) => setImmediate(resolve)).then(() => loadMcpRuntime());
		state.pending = Promise.all(enabled.map((server) => startConnection(state, server, runtime)))
			.then(() => {
				if (!signal.aborted) reportProblems(state, ctx);
			})
			.catch((error: unknown) => {
				// The session may have been disposed meanwhile, which makes ctx stale.
				try {
					ctx.ui.notify(`MCP failed to load: ${errorMessage(error)}`, "error");
				} catch {
					// No session is left to show the notice.
				}
			});
	});

	// Navigating the tree rebuilds the loadout of the target branch, and with it the prompt.
	api.on("session_tree", () => {
		state.presentation.rebuild();
	});

	api.on("model_select", () => {
		state.presentation.rebuild();
		for (const connection of state.connections())
			if (connection.state === "connected") registerTools(state, connection);
	});

	// Every prompt describes the servers: the `mcps` base section (cached with the tools, ahead of project context) holds
	// each server's block, fixed once its catalog is described, and the volatile `mcp_status` section names servers that
	// are not ready. A connection change therefore patches only `mcp_status`. The first prompt waits for servers whose
	// tools are declared to the model (`waitForDirectServers`).
	api.on("before_agent_start", async (event, ctx) => {
		if (!ctx.signal?.aborted && !state.session.signal.aborted) {
			const changed = state.servers.filter((server) => {
				const launched = server.connection && state.preparedLaunches.get(server.connection);
				return (
					isEnabled(server) &&
					server.connection?.idle &&
					!server.connection.isStopped &&
					launched &&
					launchChanged(state, server, launched)
				);
			});
			for (const server of changed) void state.track(reconnect(server));
			await waitForServers(state, changed, ctx.signal);
		}
		await waitForDirectServers(state, ctx);
		// Launch warnings (an allowed folder that was refused, say): tell the user, never the model.
		const warnings = [
			...takeLaunchWarnings(),
			...(state.options.presets ?? []).flatMap((preset) => preset.takeWarnings?.() ?? []),
		];
		for (const warning of warnings) say(ctx, warning, "warning");
		describeServers(state, event.systemPromptOptions);
	});

	// Within a run, every later request describes the servers again, so a server that connects mid-run gets its block
	// (and thin declarations) on the next turn, and a state change patches only `mcp_status`.
	api.on("turn_prompt", (event) => {
		describeServers(state, event.systemPromptOptions);
	});

	// An MCP call waits only for its server, including a pending reconnect. Readiness can be pending while the display
	// state is still connected: reconnect first closes the old transport, then opens a new one. Settled promises return
	// immediately.
	api.on("tool_call", async (event, ctx) => {
		const owner = state.toolOwners.get(event.toolName)?.split("\0", 1)[0];
		if (owner === undefined) return;
		const waiting = state.servers.filter((server) => isEnabled(server) && server.ready && server.entry.name === owner);
		await waitForServers(state, waiting, ctx.signal);
	});

	// Pick up sign-ins done outside the session, such as `octocode mcp login` run by the agent.
	api.on("turn_start", async () => {
		if (state.tokensAtSignIn.size > 0) await reconnectSignedIn(state);
	});

	// Servers registered or unregistered during the session connect or disconnect right away.
	api.on("mcp_servers_change", async (_event, ctx) => {
		const { signal } = state.session;
		if (signal.aborted) return;
		const registered = state.registeredServers();
		state.overridden = registered.overridden;
		const defaults = state.configuredEntries
			.filter(
				(entry) =>
					entry.scope === "default" &&
					!state.leavesOut(entry.name) &&
					!registered.servers.some((server) => mcpNamespace(server.entry.name) === mcpNamespace(entry.name)),
			)
			.map((entry) => ({ entry }));
		const desired: McpServer[] = [...registered.servers, ...defaults];
		const next = new Map(desired.map((server) => [mcpNamespace(server.entry.name), server]));
		// Unregistered servers and re-registered ones with a new config are dropped; the latter come back below.
		const removed = state.servers.filter(
			(server) =>
				(server.entry.scope === "extension" &&
					next.get(mcpNamespace(server.entry.name))?.registeredConfig !== server.registeredConfig) ||
				(server.entry.scope === "default" && next.get(mcpNamespace(server.entry.name))?.entry.scope === "extension"),
		);
		const added = desired.filter(
			(server) =>
				!state.servers.some(
					(current) =>
						!removed.includes(current) && mcpNamespace(current.entry.name) === mcpNamespace(server.entry.name),
				),
		);
		await applyServerChanges(state, ctx, removed, added);
	});

	api.on("session_shutdown", async () => {
		state.unregisterHookCaller?.();
		state.unregisterHookCaller = undefined;
		state.unregisterSwitch?.();
		state.unregisterSwitch = undefined;
		state.unregisterReload?.();
		state.unregisterReload = undefined;
		state.session.abort();
		const closing = state.connections();
		state.servers = [];
		state.emitChange();
		await Promise.all([
			...state.backgroundActions,
			...closing.map((connection) => connection.close()),
			...(state.options.presets ?? []).map((preset) => preset.dispose?.(state.sessionCwd)),
		]);
	});
}
