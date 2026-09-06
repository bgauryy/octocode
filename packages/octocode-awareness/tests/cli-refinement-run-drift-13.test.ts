/**
 * cli-refinement-run-drift-13.test.ts — CLI/zod contract drift fixes.
 *
 * 1. `refinement set` accepts the zod-advertised plural `--files` (array), keeping `--file` working.
 * 2. `refinement get` accepts `--refinement-id` and narrows rows to it (zod refine_query parity).
 * 3. `work start --run-id ""` must error and never insert an empty-string run id.
 * 4. The schema command registry omits the removed `refinement list` alias.
 */
import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { spawnSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT = resolve(dirname(fileURLToPath(import.meta.url)), '../out/octocode-awareness.js');
const NODE = process.execPath;

interface RunResult { status: number; stdout: string; stderr: string; parsed: Record<string, unknown> | null }
function run(dbPath: string, args: string[], cwd?: string): RunResult {
	const result = spawnSync(NODE, [SCRIPT, '--db', dbPath, ...args], {
		cwd: cwd ?? process.cwd(), encoding: 'utf8', timeout: 30000,
	});
	let parsed: Record<string, unknown> | null = null;
	try { parsed = JSON.parse(result.stdout) as Record<string, unknown>; } catch { /* non-JSON */ }
	return { status: result.status ?? 1, stdout: result.stdout, stderr: result.stderr, parsed };
}

describe('refinement CLI drift', () => {
	let root: string;
	let dbPath: string;
	beforeAll(() => {
		root = mkdtempSync(join(tmpdir(), 'oc-refine-'));
		writeFileSync(join(root, 'package.json'), JSON.stringify({ name: 'private-refine', version: '0.0.0', private: true }) + '\n');
		dbPath = join(root, 'awareness.sqlite3');
	});
	afterAll(() => { rmSync(root, { recursive: true, force: true }); });

	it('set accepts plural --files and stores them', () => {
		const r = run(dbPath, ['refinement', 'set', '--agent-id', 't13', '--workspace', root,
			'--reasoning', 'drift repro', '--remember', 'plural files accepted', '--files', 'src/a.ts', '--compact']);
		expect(r.status, `stdout=${r.stdout}\nstderr=${r.stderr}`).toBe(0);
		expect(r.parsed?.['ok']).toBe(true);
		const refinement = r.parsed?.['refinement'] as { files?: string[]; refinement_id?: string } | undefined;
		expect(refinement?.files).toEqual(['src/a.ts']);
	});

	it('set keeps singular --file working (repeatable)', () => {
		const r = run(dbPath, ['refinement', 'set', '--agent-id', 't13', '--workspace', root,
			'--reasoning', 'singular', '--remember', 'kept', '--file', 'src/b.ts', '--compact']);
		expect(r.status, `stdout=${r.stdout}\nstderr=${r.stderr}`).toBe(0);
	});

	it('get accepts --refinement-id and narrows to that row', () => {
		const created = run(dbPath, ['refinement', 'set', '--agent-id', 't13', '--workspace', root,
			'--reasoning', 'narrow me', '--remember', 'target row', '--compact']);
		const id = (created.parsed?.['refinement'] as { refinement_id: string }).refinement_id;
		const r = run(dbPath, ['refinement', 'get', '--workspace', root, '--refinement-id', id, '--compact']);
		expect(r.status, `stdout=${r.stdout}\nstderr=${r.stderr}`).toBe(0);
		const refinements = r.parsed?.['refinements'] as Array<{ refinement_id: string }> | undefined;
		expect(refinements?.length).toBe(1);
		expect(refinements?.[0]?.refinement_id).toBe(id);
	});

	it('schema registry omits the removed refinement list alias', () => {
		const r = run(dbPath, ['schema', 'commands', '--all', '--compact']);
		expect(r.status, `stdout=${r.stdout}\nstderr=${r.stderr}`).toBe(0);
		expect(r.stdout).not.toContain('refinement list');
	});
});

describe('empty run id defense', () => {
	let root: string;
	let dbPath: string;
	beforeAll(() => {
		root = mkdtempSync(join(tmpdir(), 'oc-empty-run-id-'));
		writeFileSync(join(root, 'package.json'), JSON.stringify({ name: 'private-work', version: '0.0.0', private: true }) + '\n');
		dbPath = join(root, 'awareness.sqlite3');
	});
	afterAll(() => { rmSync(root, { recursive: true, force: true }); });

	it('work start --run-id "" fails and inserts no empty-key row', () => {
		const r = run(dbPath, ['work', 'start', '--agent-id', 't13', '--workspace', root,
			'--rationale', 'bad run id', '--test-plan', 'x', '--file', 'src/x.ts', '--run-id', '', '--compact']);
		expect(r.status, `stdout=${r.stdout}\nstderr=${r.stderr}`).not.toBe(0);
		const db = new DatabaseSync(dbPath, { readOnly: true });
		try {
			const row = db.prepare("SELECT COUNT(*) AS n FROM task_runs WHERE run_id = ''").get() as { n: number };
			expect(row.n).toBe(0);
		} finally { db.close(); }
	});
});
