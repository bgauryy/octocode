import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const dir = path.dirname(fileURLToPath(import.meta.url));
const section = (file: string): string => fs.readFileSync(path.join(dir, file), 'utf8').trim();

export const authority = section('authority.md');
export const thinkFirst = section('think-first.md');
export const workMode = section('work-mode.md');
export const memory = section('memory.md');
export const tools = section('tools.md');
export const searchAndResearch = section('search-and-research.md');
export const octocodeCli = section('octocode-cli.md');
export const skills = section('skills.md');
export const code = section('code.md');
export const docs = section('docs.md');
export const output = section('output.md');
export const context = section('context.md');
export const agents = section('agents.md');
export const browserAgent = section('browser-agent.md');
export const safety = section('safety.md');
