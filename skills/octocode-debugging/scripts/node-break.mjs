#!/usr/bin/env node
// Runs a Node program under the inspector, reports each pause as one JSON line and resumes, so one bash call
// replaces an interactive debugger session. No dependencies: Node 22+ (global WebSocket).
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const USAGE = `Usage: node node-break.mjs [options] -- <script | node flags and script> [args]
  --break <file:line[:condition]>  Pause at that line (repeatable); the condition is a JavaScript expression.
  --eval <expression>              Evaluate at each pause (repeatable). Default: the locals of the top frame.
  --exceptions <all|uncaught>      Also pause where an exception is thrown. Prefer all: Node itself catches throws
                                   in ES modules and async code, so uncaught misses them. Node-internal throws are skipped.
  --steps <n>                      After each breakpoint hit, step over n lines and report each one.
  --max-hits <n>                   Stop pausing after n pauses and let the program finish (default 20).
  --timeout <seconds>              Then pause every thread, print its stack and kill the program (default 30).
A \`debugger;\` statement in the source also pauses, in transformed code (vitest, tsx) too.
Prints one JSON object per line on stdout: hit, step, exception, timeout, max-hits, unbound, exit.
unbound names a --break that matched no loaded code: a wrong path or line, or code that never loaded.
Exits with the program's exit code.`;

const INSPECTOR_NOISE =
	/^(Debugger listening on|For help, see|Debugger attached|Waiting for the debugger to disconnect|Debugger ending on)/;
const MAX_PROPERTIES = 25;
const MAX_TEXT = 200;
const STACK_DEPTH = 15;

function parseArgs(argv) {
	const options = { breaks: [], evals: [], exceptions: "none", steps: 0, maxHits: 20, timeout: 30, command: [] };
	const split = argv.indexOf("--");
	if (split < 0 || split === argv.length - 1 || argv.includes("--help")) {
		console.error(USAGE);
		process.exit(2);
	}
	options.command = argv.slice(split + 1);
	const flags = argv.slice(0, split);
	for (let i = 0; i < flags.length; i++) {
		const flag = flags[i];
		const value = flags[++i];
		if (value === undefined) fail(`${flag} needs a value`);
		if (flag === "--break") {
			const match = /^(.*?):(\d+)(?::(.*))?$/.exec(value);
			if (!match) fail(`--break ${value}: expected file:line or file:line:condition`);
			const same = options.breaks.find((b) => b.file === path.resolve(match[1]) && b.line === Number(match[2]));
			if (same) fail(`--break ${value}: one breakpoint per line; join the conditions with || or &&`);
			options.breaks.push({ file: path.resolve(match[1]), line: Number(match[2]), condition: match[3] ?? "" });
		} else if (flag === "--eval") options.evals.push(value);
		else if (flag === "--exceptions") {
			if (value !== "all" && value !== "uncaught") fail("--exceptions: all or uncaught");
			options.exceptions = value;
		} else if (flag === "--steps" || flag === "--max-hits" || flag === "--timeout") {
			const number = Number(value);
			if (!Number.isFinite(number) || number < 0) fail(`${flag}: a non-negative number`);
			options[flag === "--steps" ? "steps" : flag === "--max-hits" ? "maxHits" : "timeout"] = number;
		} else fail(`unknown option ${flag}\n${USAGE}`);
	}
	return options;
}

function fail(message) {
	console.error(`node-break: ${message}`);
	process.exit(2);
}

const print = (event) => console.log(JSON.stringify(event));
const clip = (text) => (text.length > MAX_TEXT ? `${text.slice(0, MAX_TEXT)}…` : text);
const escapeRegex = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

function displayPath(url) {
	if (!url) return "<eval>";
	const file = url.startsWith("file://") ? fileURLToPath(url) : url;
	const relative = path.relative(process.cwd(), file);
	return relative && !relative.startsWith("..") && !path.isAbsolute(relative) ? relative : file;
}

/** Script URLs by id, filled from Debugger.scriptParsed: call frames in Node carry the id, not the URL. */
const scriptUrl = (target, frame) => frame.url || target.scripts.get(frame.location.scriptId) || "";
const internal = (target, frame) => scriptUrl(target, frame).startsWith("node:");
const where = (target, frame) =>
	`${frame.functionName || "<anonymous>"} ${displayPath(scriptUrl(target, frame))}:${frame.location.lineNumber + 1}`;

/** A Runtime.RemoteObject as a short value: primitives as themselves, objects as their preview or description. */
function render(object) {
	if (!object) return undefined;
	if (object.type === "undefined") return "undefined";
	if ("value" in object) return typeof object.value === "string" ? clip(object.value) : object.value;
	if (object.unserializableValue) return object.unserializableValue;
	const preview = object.preview;
	if (preview?.properties) {
		const inner = preview.properties.map((p) => (preview.subtype === "array" ? p.value : `${p.name}: ${p.value}`));
		const body = `${inner.join(", ")}${preview.overflow ? ", …" : ""}`;
		return clip(
			preview.subtype === "array"
				? `${object.description ?? "Array"} [${body}]`
				: `${object.description ?? "Object"} {${body}}`,
		);
	}
	return clip(object.description ?? object.type);
}

/** One inspector session: the main thread over the WebSocket, or a worker thread through NodeWorker messages. */
class Target {
	constructor(name, sendRaw) {
		this.name = name;
		this.sendRaw = sendRaw;
		this.nextId = 0;
		this.pending = new Map();
		this.workers = new Map();
		this.scripts = new Map();
		this.stepsLeft = 0;
		this.stepping = false;
	}
	send(method, params = {}) {
		const id = ++this.nextId;
		return new Promise((resolve, reject) => {
			this.pending.set(id, { resolve, reject });
			this.sendRaw(JSON.stringify({ id, method, params }));
		});
	}
	receive(message) {
		if (message.id === undefined) return void onEvent(this, message).catch(report);
		const waiter = this.pending.get(message.id);
		this.pending.delete(message.id);
		if (message.error) waiter?.reject(new Error(`${message.error.message}`));
		else waiter?.resolve(message.result);
	}
}

const options = parseArgs(process.argv.slice(2));
const state = { pauses: 0, quiet: false, timedOut: false };
/** Breakpoint ids (any thread) that bound to code, by --break index. */
const bound = new Set();
const breakIndex = new Map();
const report = (error) => print({ event: "error", message: String(error?.message ?? error) });

const child = spawn(process.execPath, ["--inspect-brk=127.0.0.1:0", ...options.command], {
	stdio: ["inherit", "inherit", "pipe"],
});
// A failure here must not leave the program parked at its first line, waiting for a debugger.
const fatal = (error) => {
	report(error);
	child.kill("SIGKILL");
	process.exitCode = 1;
};
process.on("uncaughtException", fatal);
process.on("unhandledRejection", fatal);
child.on("exit", (code, signal) => {
	options.breaks.forEach(({ file, line }, index) => {
		if (!bound.has(index)) print({ event: "unbound", break: `${displayPath(file)}:${String(line)}` });
	});
	print({ event: "exit", code, signal, pauses: state.pauses });
	process.exit(code ?? 1);
});

const url = await new Promise((resolve) => {
	let pending = "";
	child.stderr.on("data", (chunk) => {
		pending += chunk;
		const lines = pending.split("\n");
		pending = lines.pop() ?? "";
		for (const line of lines) {
			const match = /ws:\/\/\S+/.exec(line);
			if (match) resolve(match[0]);
			if (!INSPECTOR_NOISE.test(line)) process.stderr.write(`${line}\n`);
		}
	});
});

const socket = new WebSocket(url);
await new Promise((resolve, reject) => {
	socket.addEventListener("open", resolve, { once: true });
	socket.addEventListener("error", () => reject(new Error(`cannot connect to ${url}`)), { once: true });
});
const main = new Target("main", (raw) => socket.send(raw));
socket.addEventListener("message", ({ data }) => main.receive(JSON.parse(String(data))));

async function setup(target) {
	await target.send("Runtime.enable");
	await target.send("Debugger.enable");
	for (const [index, { file, line, condition }] of options.breaks.entries()) {
		const { breakpointId, locations } = await target.send("Debugger.setBreakpointByUrl", {
			urlRegex: `${escapeRegex(file)}$`,
			lineNumber: line - 1,
			condition,
		});
		breakIndex.set(breakpointId, index);
		if (locations.length > 0) bound.add(index);
	}
	await target.send("Debugger.setPauseOnExceptions", { state: options.exceptions });
	// Worker threads (vitest's default pool) attach as their own sessions; a runtime without NodeWorker skips this.
	await target.send("NodeWorker.enable", { waitForDebuggerOnStart: true }).catch(() => undefined);
	await target.send("Runtime.runIfWaitingForDebugger");
}

async function onEvent(target, { method, params }) {
	if (method === "NodeWorker.attachedToWorker") {
		const { sessionId, workerInfo } = params;
		const worker = new Target(`worker ${workerInfo.workerId}`, (raw) =>
			target.send("NodeWorker.sendMessageToWorker", { sessionId, message: raw }).catch(() => undefined),
		);
		target.workers.set(sessionId, worker);
		await setup(worker);
	} else if (method === "NodeWorker.receivedMessageFromWorker")
		target.workers.get(params.sessionId)?.receive(JSON.parse(params.message));
	else if (method === "NodeWorker.detachedFromWorker") target.workers.delete(params.sessionId);
	else if (method === "Debugger.scriptParsed") target.scripts.set(params.scriptId, params.url);
	else if (method === "Debugger.breakpointResolved") bound.add(breakIndex.get(params.breakpointId));
	else if (method === "Debugger.paused") await onPause(target, params);
	// After the main script ends, Node waits for the debugger to disconnect before it exits.
	else if (method === "Runtime.executionContextDestroyed" && target === main) socket.close();
}

async function values(target, frame) {
	if (options.evals.length > 0) {
		const result = {};
		for (const expression of options.evals) {
			const evaluated = await target.send("Debugger.evaluateOnCallFrame", {
				callFrameId: frame.callFrameId,
				expression,
				generatePreview: true,
				silent: true,
			});
			result[expression] = evaluated.exceptionDetails
				? `throws ${render(evaluated.exceptionDetails.exception) ?? evaluated.exceptionDetails.text}`
				: render(evaluated.result);
		}
		return result;
	}
	const locals = {};
	for (const scope of frame.scopeChain.filter((s) => s.type === "local" || s.type === "block")) {
		const { result } = await target.send("Runtime.getProperties", {
			objectId: scope.object.objectId,
			ownProperties: true,
			generatePreview: true,
		});
		for (const property of result.slice(0, MAX_PROPERTIES))
			if (!(property.name in locals)) locals[property.name] = render(property.value);
	}
	return locals;
}

async function onPause(target, { reason, callFrames, data }) {
	const stack = callFrames
		.filter((frame) => !internal(target, frame))
		.slice(0, STACK_DEPTH)
		.map((frame) => where(target, frame));
	if (state.timedOut) return print({ event: "timeout", thread: target.name, stack });
	// --inspect-brk stops before the first line, and an uncaught exception stops again in Node's own frames as it
	// unwinds: neither is a pause in the program.
	if (reason === "Break on start" || (internal(target, callFrames[0]) && !target.stepping))
		return void (await target.send("Debugger.resume"));
	const event = target.stepping
		? "step"
		: reason === "exception" || reason === "promiseRejection"
			? "exception"
			: "hit";
	const top = callFrames[0];
	const entry = { event, thread: target.name, at: where(target, top), values: await values(target, top), stack };
	if (event === "exception") entry.exception = render(data);
	print(entry);
	state.pauses++;
	if (event === "hit") target.stepsLeft = options.steps;
	target.stepping = target.stepsLeft > 0;
	if (state.pauses >= options.maxHits && !state.quiet) {
		state.quiet = true;
		print({ event: "max-hits", pauses: state.pauses });
		for (const each of [main, ...main.workers.values()]) {
			await each.send("Debugger.setBreakpointsActive", { active: false }).catch(() => undefined);
			await each.send("Debugger.setPauseOnExceptions", { state: "none" }).catch(() => undefined);
		}
		target.stepping = false;
	}
	if (target.stepping) {
		target.stepsLeft--;
		await target.send("Debugger.stepOver");
	} else await target.send("Debugger.resume");
}

if (options.timeout > 0)
	setTimeout(() => {
		state.timedOut = true;
		for (const target of [main, ...main.workers.values()]) target.send("Debugger.pause").catch(() => undefined);
		setTimeout(() => child.kill("SIGKILL"), 1_000).unref();
	}, options.timeout * 1_000).unref();

await setup(main);
