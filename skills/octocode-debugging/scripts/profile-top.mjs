#!/usr/bin/env node
// Summarizes a V8 profile from `node --cpu-prof` (.cpuprofile) or `node --heap-prof` (.heapprofile) as a table of
// the heaviest functions, so the agent reads a few lines instead of a multi-megabyte JSON file; with --sync-io, groups
// a `node --trace-sync-io` log by phase, API and calling frame. No dependencies.
import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const USAGE = `Usage: node profile-top.mjs <file.cpuprofile | file.heapprofile | directory> [options]
       node profile-top.mjs <sync.log> --sync-io [--owner <text>] [--filter <text>] [--top <n>] [--ops <n>]
  A directory means its newest .cpuprofile or .heapprofile; the header names how many it holds.
  --top <n>               Rows to print (default 20); the header states how many functions matched.
  --sort <self|total>     self: cost inside the function; total: including what it calls (default self).
  --filter <text>         Only functions whose name or location contains the text.
  --from <s>, --to <s>    CPU only: keep samples in this window, seconds from the profile's start (one phase of a run).
  --owner <text>          Charge each cost to the nearest frame whose location contains the text (your code), so time or
                          bytes inside libraries and runtime internals land on the call site that caused them.
  --callers <text>        Instead of the table: the call chains (up to 6 callers) that reach functions whose name or
                          location contains the text, by the cost under them.
  --ops <n>               The window covered n operations: adds per-operation columns (cost divided by n), e.g. bytes
                          allocated per request from a node-alloc.mjs profile.
  --sync-io               Read the stderr log of node --trace-sync-io instead of a profile. Each warning is one synchronous
                          operation; rows group them by phase, API (the runtime function your code called, such as
                          readFileSync) and calling frame (the first frame whose location contains --owner, else the
                          first frame outside node: internals). A line starting "phase:" names the phase of the
                          warnings after it; earlier ones are in "(start)".
CPU rows are milliseconds; heap rows are bytes the profile sampled (from --heap-prof: still allocated when it was written).
Total counts a function once per stack, so recursion is not double counted. Prints a text table; exit 0, or 2 with
"profile-top: <reason>" on stderr for invalid options or an unreadable or unrecognized input.`;

const HIDDEN = new Set(["(root)", "(idle)"]);

function fail(message) {
	console.error(`profile-top: ${message}`);
	process.exit(2);
}

function parseArgs(argv) {
	const options = { target: undefined, top: 20, sort: "self", filter: "", from: 0, to: Infinity, owner: "", callers: "", ops: 0, syncIo: false, given: new Set() };
	for (let i = 0; i < argv.length; i++) {
		const arg = argv[i];
		if (arg === "--help") {
			console.log(USAGE);
			process.exit(0);
		}
		if (arg === "--sync-io") {
			options.syncIo = true;
			continue;
		}
		if (!arg.startsWith("--")) {
			options.target = arg;
			continue;
		}
		const value = argv[++i];
		if (value === undefined) fail(`${arg} needs a value`);
		options.given.add(arg);
		if (arg === "--top") options.top = Number(value);
		else if (arg === "--sort" && (value === "self" || value === "total")) options.sort = value;
		else if (arg === "--filter") options.filter = value.toLowerCase();
		else if (arg === "--from" || arg === "--to") {
			const seconds = Number(value);
			if (!Number.isFinite(seconds) || seconds < 0) fail(`${arg}: seconds from the profile's start`);
			options[arg.slice(2)] = seconds;
		} else if (arg === "--owner") options.owner = value;
		else if (arg === "--callers") options.callers = value.toLowerCase();
		else if (arg === "--ops") {
			options.ops = Number(value);
			if (!Number.isInteger(options.ops) || options.ops < 1) fail("--ops: a positive integer of operations");
		} else fail(`bad option ${arg} ${value}\n${USAGE}`);
	}
	if (!options.target) fail(USAGE);
	if (!Number.isInteger(options.top) || options.top < 1) fail("--top: a positive integer");
	const profileOnly = ["--sort", "--from", "--to", "--callers"].filter((flag) => options.given.has(flag));
	if (options.syncIo && profileOnly.length) fail(`${profileOnly.join(", ")}: not used with --sync-io`);
	return options;
}

/** The profile to read, and how many profiles the directory held (1 for a named file). */
function resolveFile(target) {
	let stats;
	try {
		stats = statSync(target);
	} catch (error) {
		fail(`${target}: ${error.code ?? error.message}`);
	}
	if (!stats.isDirectory()) return { file: target, candidates: 1 };
	const profiles = readdirSync(target)
		.filter((name) => name.endsWith(".cpuprofile") || name.endsWith(".heapprofile"))
		.map((name) => path.join(target, name))
		.sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs);
	if (!profiles.length) fail(`no .cpuprofile or .heapprofile in ${target}`);
	return { file: profiles[0], candidates: profiles.length };
}

function location({ url, lineNumber }) {
	if (!url) return "";
	const file = url.startsWith("file://") ? fileURLToPath(url) : url;
	const relative = path.relative(process.cwd(), file);
	const shown = relative && !relative.startsWith("..") && !path.isAbsolute(relative) ? relative : file;
	return `${shown}:${lineNumber + 1}`;
}

/** True when --owner text occurs in the frame's shown location or its absolute path, so "$PWD/src" and "src/" both match. */
function owned(frame, owner) {
	if (!frame.url) return false;
	const file = frame.url.startsWith("file://") ? fileURLToPath(frame.url) : frame.url;
	return location(frame).includes(owner) || `${file}:${frame.lineNumber + 1}`.includes(owner);
}

/**
 * Adds each node's own cost to its function (or, with `owner`, to the nearest frame on its stack whose location contains
 * `owner`) and, once per stack, to every function on the path from the root. With `callers`, also adds the cost under
 * the outermost frame matching it to that frame's chain of callers.
 */
function aggregate(root, children, selfCost, { owner = "", callers = "" } = {}) {
	const rows = new Map();
	const chains = new Map();
	const row = (frame) => {
		const name = frame.functionName || "(anonymous)";
		const where = location(frame);
		const key = `${name}\u0000${where}`;
		if (!rows.has(key)) rows.set(key, { name, where, self: 0, total: 0 });
		return rows.get(key);
	};
	const outside = owner ? row({ functionName: `(outside ${owner})`, url: "" }) : undefined;
	let all = 0;
	const walk = (node, onStack, ancestors, charged, chain) => {
		const entry = row(node.callFrame);
		const cost = selfCost(node);
		const mine = owner && owned(node.callFrame, owner) ? entry : charged;
		(owner && !HIDDEN.has(entry.name) ? (mine ?? outside) : entry).self += cost;
		all += HIDDEN.has(entry.name) ? 0 : cost;
		const stack = new Set(onStack).add(entry);
		for (const each of stack) each.total += cost;
		const here = [...ancestors, entry];
		let key = chain;
		if (!key && callers && `${entry.name} ${entry.where}`.toLowerCase().includes(callers))
			key = here
				.slice(-7)
				.reverse()
				.filter((frame) => !HIDDEN.has(frame.name))
				.map((frame) => `${frame.name} ${frame.where}`.trim())
				.join(" < ");
		if (key) chains.set(key, (chains.get(key) ?? 0) + cost);
		for (const child of children(node)) walk(child, stack, here, mine, key);
	};
	walk(root, new Set(), [], undefined, undefined);
	return { rows: [...rows.values()].filter((entry) => !HIDDEN.has(entry.name)), all, chains };
}

function cpu(profile, options) {
	const byId = new Map(profile.nodes.map((node) => [node.id, node]));
	const time = new Map();
	// Each sample stands for the time until the next one; its timestamp places it in the window.
	let at = profile.startTime;
	const from = profile.startTime + options.from * 1_000_000;
	const to = profile.startTime + options.to * 1_000_000;
	profile.samples.forEach((id, i) => {
		at += profile.timeDeltas[i] ?? 0;
		if (at < from || at > to) return;
		const delta = profile.timeDeltas[i + 1] ?? 0;
		time.set(id, (time.get(id) ?? 0) + Math.max(0, delta));
	});
	const root = profile.nodes.find((node) => node.callFrame.functionName === "(root)") ?? profile.nodes[0];
	const result = aggregate(
		root,
		(node) => (node.children ?? []).map((id) => byId.get(id)).filter(Boolean),
		(node) => (time.get(node.id) ?? 0) / 1_000,
		options,
	);
	const idle = [...byId.values()]
		.filter((node) => node.callFrame.functionName === "(idle)")
		.reduce((sum, node) => sum + (time.get(node.id) ?? 0) / 1_000, 0);
	const wall = (profile.endTime - profile.startTime) / 1_000;
	const window =
		options.from > 0 || options.to < Infinity ? `, window ${options.from}-${options.to === Infinity ? "end" : options.to} s` : "";
	return {
		...result,
		unit: "ms",
		header: `CPU profile: ${wall.toFixed(0)} ms wall${window}, ${result.all.toFixed(0)} ms sampled busy, ${idle.toFixed(0)} ms idle, ${profile.samples.length} samples`,
	};
}

function heap(profile, options) {
	if (options.from > 0 || options.to < Infinity) fail("--from and --to need a CPU profile");
	const result = aggregate(
		profile.head,
		(node) => node.children ?? [],
		(node) => node.selfSize ?? 0,
		options,
	);
	return { ...result, unit: "bytes", header: `Heap profile: ${result.all} bytes sampled` };
}

/** Groups --trace-sync-io warnings by phase, the runtime API called and the calling frame. */
function syncIo(file, options) {
	let text;
	try {
		text = readFileSync(file, "utf8");
	} catch (error) {
		fail(`${file}: ${error.code ?? error.message}`);
	}
	const frame = (line) => {
		const match = /^(?:(.*?) \()?(.*?):(\d+):\d+\)?$/.exec(line.trim());
		if (!match) return { name: line.trim(), url: "", lineNumber: 0, where: "" };
		const url = match[2];
		const lineNumber = Number(match[3]) - 1;
		return { name: match[1] || "(anonymous)", url, lineNumber, where: location({ url, lineNumber }) };
	};
	const events = [];
	let phase = "(start)";
	let current;
	for (const line of text.split("\n")) {
		if (line.startsWith("phase:")) {
			phase = line.slice("phase:".length).trim() || "(unnamed)";
			current = undefined;
		} else if (line.includes("WARNING: Detected use of sync API")) {
			current = { phase, frames: [] };
			events.push(current);
		} else if (current && /^\s+at /.test(line)) current.frames.push(frame(line.replace(/^\s+at /, "")));
		else current = undefined;
	}
	if (!events.length) fail(`${file}: no "Detected use of sync API" warnings (was it written by node --trace-sync-io?)`);
	const groups = new Map();
	for (const event of events) {
		const outside = event.frames.findIndex((each) => !each.url.startsWith("node:"));
		const api = outside > 0 ? event.frames[outside - 1].name : "(unknown)";
		const caller = options.owner ? event.frames.find((each) => owned(each, options.owner)) : event.frames[outside];
		const site = caller ? `${caller.name} ${caller.where}`.trim() : options.owner ? `(no frame in ${options.owner})` : "(runtime frames only)";
		const key = `${event.phase}\u0000${api}\u0000${site}`;
		if (!groups.has(key)) groups.set(key, { phase: event.phase, api, site, count: 0 });
		groups.get(key).count++;
	}
	const matched = [...groups.values()]
		.filter((group) => !options.filter || `${group.api} ${group.site}`.toLowerCase().includes(options.filter))
		.sort((a, b) => b.count - a.count);
	const rows = matched.slice(0, options.top);
	const phases = new Set(events.map((event) => event.phase)).size;
	const by = options.owner ? `the first frame in "${options.owner}"` : "the first frame outside node: internals";
	const perOp = options.ops ? `; per operation over ${options.ops} operations` : "";
	console.log(`${file}\nSync I/O log: ${events.length} synchronous operations in ${phases} phases; grouped by phase, API and ${by}${perOp}; ${rows.length} of ${matched.length} groups`);
	const head = ["ops", "ops%", ...(options.ops ? ["ops/op"] : []), "phase", "api", "frame"];
	printTable(
		[head].concat(rows.map((group) => [String(group.count), `${((100 * group.count) / events.length).toFixed(1)}%`, ...(options.ops ? [(group.count / options.ops).toFixed(3)] : []), group.phase, group.api, group.site])),
		options.ops ? 3 : 2,
	);
}

/** Prints rows of cells as aligned columns: the first `numeric` columns right-aligned. */
function printTable(table, numeric) {
	const widths = table[0].map((_, column) => Math.max(...table.map((cells) => cells[column].length)));
	for (const cells of table)
		console.log(
			cells
				.map((cell, column) => (column < numeric ? cell.padStart(widths[column]) : cell.padEnd(widths[column])))
				.join("  ")
				.trimEnd(),
		);
}

const options = parseArgs(process.argv.slice(2));
if (options.syncIo) {
	syncIo(options.target, options);
	process.exit(0);
}
const { file, candidates } = resolveFile(options.target);
let profile;
try {
	profile = JSON.parse(readFileSync(file, "utf8"));
} catch (error) {
	fail(`${file}: ${error.message}`);
}
const summary = profile?.head
	? heap(profile, options)
	: profile?.nodes
		? cpu(profile, options)
		: fail(`${file}: not a V8 profile`);
const number = (value) => (summary.unit === "ms" ? value.toFixed(1) : String(Math.round(value)));
const perOp = (value) => (value / options.ops).toFixed(summary.unit === "ms" ? 3 : 1);
const percent = (value) => `${summary.all > 0 ? ((100 * value) / summary.all).toFixed(1) : "0.0"}%`;
const matched = summary.rows
	.filter((row) => !options.filter || `${row.name} ${row.where}`.toLowerCase().includes(options.filter))
	.sort((a, b) => b[options.sort] - a[options.sort]);
const rows = matched.slice(0, options.top);

const newest = candidates > 1 ? ` (newest of ${candidates} profiles; pass a file for another isolate)` : "";
const operations = options.ops ? `; per operation over ${options.ops} operations` : "";
if (options.callers) {
	const chains = [...summary.chains].sort((a, b) => b[1] - a[1]);
	console.log(`${file}${newest}\n${summary.header}${operations}; ${Math.min(options.top, chains.length)} of ${chains.length} caller chains`);
	for (const [chain, cost] of chains.slice(0, options.top))
		console.log(`${number(cost).padStart(10)}  ${percent(cost).padStart(6)}  ${options.ops ? `${perOp(cost).padStart(12)}  ` : ""}${chain}`);
	process.exit(0);
}
const charged = options.owner ? `; costs charged to the nearest frame in "${options.owner}"` : "";
console.log(
	`${file}${newest}\n${summary.header}${charged}${operations}; sorted by ${options.sort}; ${rows.length} of ${matched.length} functions`,
);
const unit = summary.unit;
const head = [`self ${unit}`, "self%", `total ${unit}`, "total%", ...(options.ops ? [`self ${unit}/op`, `total ${unit}/op`] : []), "function", "location"];
printTable(
	[head].concat(
		rows.map((row) => [
			number(row.self),
			percent(row.self),
			number(row.total),
			percent(row.total),
			...(options.ops ? [perOp(row.self), perOp(row.total)] : []),
			row.name,
			row.where,
		]),
	),
	options.ops ? 6 : 4,
);
