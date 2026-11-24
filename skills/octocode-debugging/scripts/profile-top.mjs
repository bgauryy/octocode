#!/usr/bin/env node
// Summarizes a V8 profile from `node --cpu-prof` (.cpuprofile) or `node --heap-prof` (.heapprofile) as a table of
// the heaviest functions, so the agent reads a few lines instead of a multi-megabyte JSON file. No dependencies.
import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const USAGE = `Usage: node profile-top.mjs <file.cpuprofile | file.heapprofile | directory> [options]
  A directory means its newest .cpuprofile or .heapprofile; the header names how many it holds.
  --top <n>               Rows to print (default 20); the header states how many functions matched.
  --sort <self|total>     self: cost inside the function; total: including what it calls (default self).
  --filter <text>         Only functions whose name or location contains the text.
  --from <s>, --to <s>    CPU only: keep samples in this window, seconds from the profile's start (one phase of a run).
  --owner <text>          Charge each cost to the nearest frame whose location contains the text (your code), so time or
                          bytes inside libraries and runtime internals land on the call site that caused them.
  --callers <text>        Instead of the table: the call chains (up to 6 callers) that reach functions whose name or
                          location contains the text, by the cost under them.
CPU rows are milliseconds; heap rows are bytes the profile sampled (from --heap-prof: still allocated when it was written).
Total counts a function once per stack, so recursion is not double counted.`;

const HIDDEN = new Set(["(root)", "(idle)"]);

function fail(message) {
	console.error(`profile-top: ${message}`);
	process.exit(2);
}

function parseArgs(argv) {
	const options = { target: undefined, top: 20, sort: "self", filter: "", from: 0, to: Infinity, owner: "", callers: "" };
	for (let i = 0; i < argv.length; i++) {
		const arg = argv[i];
		if (arg === "--help") {
			console.log(USAGE);
			process.exit(0);
		}
		if (!arg.startsWith("--")) {
			options.target = arg;
			continue;
		}
		const value = argv[++i];
		if (value === undefined) fail(`${arg} needs a value`);
		if (arg === "--top") options.top = Number(value);
		else if (arg === "--sort" && (value === "self" || value === "total")) options.sort = value;
		else if (arg === "--filter") options.filter = value.toLowerCase();
		else if (arg === "--from" || arg === "--to") {
			const seconds = Number(value);
			if (!Number.isFinite(seconds) || seconds < 0) fail(`${arg}: seconds from the profile's start`);
			options[arg.slice(2)] = seconds;
		} else if (arg === "--owner") options.owner = value;
		else if (arg === "--callers") options.callers = value.toLowerCase();
		else fail(`bad option ${arg} ${value}\n${USAGE}`);
	}
	if (!options.target) fail(USAGE);
	if (!Number.isInteger(options.top) || options.top < 1) fail("--top: a positive integer");
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
		const mine = owner && location(node.callFrame).includes(owner) ? entry : charged;
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

const options = parseArgs(process.argv.slice(2));
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
const percent = (value) => `${summary.all > 0 ? ((100 * value) / summary.all).toFixed(1) : "0.0"}%`;
const matched = summary.rows
	.filter((row) => !options.filter || `${row.name} ${row.where}`.toLowerCase().includes(options.filter))
	.sort((a, b) => b[options.sort] - a[options.sort]);
const rows = matched.slice(0, options.top);

const newest = candidates > 1 ? ` (newest of ${candidates} profiles; pass a file for another isolate)` : "";
if (options.callers) {
	const chains = [...summary.chains].sort((a, b) => b[1] - a[1]);
	console.log(`${file}${newest}\n${summary.header}; ${Math.min(options.top, chains.length)} of ${chains.length} caller chains`);
	for (const [chain, cost] of chains.slice(0, options.top))
		console.log(`${number(cost).padStart(10)}  ${percent(cost).padStart(6)}  ${chain}`);
	process.exit(0);
}
const charged = options.owner ? `; costs charged to the nearest frame in "${options.owner}"` : "";
console.log(
	`${file}${newest}\n${summary.header}${charged}; sorted by ${options.sort}; ${rows.length} of ${matched.length} functions`,
);
const table = [[`self ${summary.unit}`, "self%", `total ${summary.unit}`, "total%", "function", "location"]].concat(
	rows.map((row) => [number(row.self), percent(row.self), number(row.total), percent(row.total), row.name, row.where]),
);
const widths = table[0].map((_, column) => Math.max(...table.map((cells) => cells[column].length)));
for (const cells of table)
	console.log(
		cells
			.map((cell, column) => (column < 4 ? cell.padStart(widths[column]) : cell.padEnd(widths[column])))
			.join("  ")
			.trimEnd(),
	);
