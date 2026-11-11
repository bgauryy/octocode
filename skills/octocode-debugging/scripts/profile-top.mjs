#!/usr/bin/env node
// Summarizes a V8 profile from `node --cpu-prof` (.cpuprofile) or `node --heap-prof` (.heapprofile) as a table of
// the heaviest functions, so the agent reads a few lines instead of a multi-megabyte JSON file. No dependencies.
import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const USAGE = `Usage: node profile-top.mjs <file.cpuprofile | file.heapprofile | directory> [options]
  A directory means its newest .cpuprofile or .heapprofile.
  --top <n>               Rows to print (default 20).
  --sort <self|total>     self: cost inside the function; total: including what it calls (default self).
  --filter <text>         Only functions whose name or location contains the text.
CPU rows are milliseconds; heap rows are bytes still allocated when the profile was written.
Total counts a function once per stack, so recursion is not double counted.`;

const HIDDEN = new Set(["(root)", "(idle)"]);

function fail(message) {
	console.error(`profile-top: ${message}`);
	process.exit(2);
}

function parseArgs(argv) {
	const options = { target: undefined, top: 20, sort: "self", filter: "" };
	for (let i = 0; i < argv.length; i++) {
		const arg = argv[i];
		if (arg === "--help") fail(USAGE);
		if (!arg.startsWith("--")) {
			options.target = arg;
			continue;
		}
		const value = argv[++i];
		if (value === undefined) fail(`${arg} needs a value`);
		if (arg === "--top") options.top = Number(value);
		else if (arg === "--sort" && (value === "self" || value === "total")) options.sort = value;
		else if (arg === "--filter") options.filter = value.toLowerCase();
		else fail(`bad option ${arg} ${value}\n${USAGE}`);
	}
	if (!options.target) fail(USAGE);
	if (!Number.isInteger(options.top) || options.top < 1) fail("--top: a positive integer");
	return options;
}

function resolveFile(target) {
	if (!statSync(target).isDirectory()) return target;
	const newest = readdirSync(target)
		.filter((name) => name.endsWith(".cpuprofile") || name.endsWith(".heapprofile"))
		.map((name) => path.join(target, name))
		.sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs)[0];
	if (!newest) fail(`no .cpuprofile or .heapprofile in ${target}`);
	return newest;
}

function location({ url, lineNumber }) {
	if (!url) return "";
	const file = url.startsWith("file://") ? fileURLToPath(url) : url;
	const relative = path.relative(process.cwd(), file);
	const shown = relative && !relative.startsWith("..") && !path.isAbsolute(relative) ? relative : file;
	return `${shown}:${lineNumber + 1}`;
}

/** Adds each node's own cost to its function and, once per stack, to every function on the path from the root. */
function aggregate(root, children, selfCost) {
	const rows = new Map();
	const row = (frame) => {
		const name = frame.functionName || "(anonymous)";
		const where = location(frame);
		const key = `${name}\u0000${where}`;
		if (!rows.has(key)) rows.set(key, { name, where, self: 0, total: 0 });
		return rows.get(key);
	};
	let all = 0;
	const walk = (node, onStack) => {
		const entry = row(node.callFrame);
		const cost = selfCost(node);
		entry.self += cost;
		all += HIDDEN.has(entry.name) ? 0 : cost;
		const stack = new Set(onStack).add(entry);
		for (const each of stack) each.total += cost;
		for (const child of children(node)) walk(child, stack);
	};
	walk(root, new Set());
	return { rows: [...rows.values()].filter((entry) => !HIDDEN.has(entry.name)), all };
}

function cpu(profile) {
	const byId = new Map(profile.nodes.map((node) => [node.id, node]));
	const time = new Map();
	// Each sample stands for the time until the next one.
	profile.samples.forEach((id, i) => {
		const delta = profile.timeDeltas[i + 1] ?? 0;
		time.set(id, (time.get(id) ?? 0) + Math.max(0, delta));
	});
	const root = profile.nodes.find((node) => node.callFrame.functionName === "(root)") ?? profile.nodes[0];
	const result = aggregate(
		root,
		(node) => (node.children ?? []).map((id) => byId.get(id)).filter(Boolean),
		(node) => (time.get(node.id) ?? 0) / 1_000,
	);
	const idle = [...byId.values()]
		.filter((node) => node.callFrame.functionName === "(idle)")
		.reduce((sum, node) => sum + (time.get(node.id) ?? 0) / 1_000, 0);
	const wall = (profile.endTime - profile.startTime) / 1_000;
	return {
		...result,
		unit: "ms",
		header: `CPU profile: ${wall.toFixed(0)} ms wall, ${result.all.toFixed(0)} ms sampled busy, ${idle.toFixed(0)} ms idle, ${profile.samples.length} samples`,
	};
}

function heap(profile) {
	const result = aggregate(
		profile.head,
		(node) => node.children ?? [],
		(node) => node.selfSize ?? 0,
	);
	return { ...result, unit: "bytes", header: `Heap profile: ${result.all} bytes sampled live` };
}

const options = parseArgs(process.argv.slice(2));
const file = resolveFile(options.target);
const profile = JSON.parse(readFileSync(file, "utf8"));
const summary = profile.head ? heap(profile) : profile.nodes ? cpu(profile) : fail(`${file}: not a V8 profile`);
const number = (value) => (summary.unit === "ms" ? value.toFixed(1) : String(Math.round(value)));
const percent = (value) => `${summary.all > 0 ? ((100 * value) / summary.all).toFixed(1) : "0.0"}%`;
const rows = summary.rows
	.filter((row) => !options.filter || `${row.name} ${row.where}`.toLowerCase().includes(options.filter))
	.sort((a, b) => b[options.sort] - a[options.sort])
	.slice(0, options.top);

console.log(`${file}\n${summary.header}; sorted by ${options.sort}`);
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
