// Vendor skill-directory map and platform selection for skill-sync.
import { join, relative, sep } from 'node:path';
import { homedir } from 'node:os';

export const USER_SKILL_HOME = process.env.HOME || process.env.USERPROFILE || homedir();

/** User-scope vendor skill roots (global). Project scope is out of band — pass --project-root. */
export const VENDORS = {
  codex: {
    id: 'codex',
    label: 'Codex / shared agents',
    user: join(USER_SKILL_HOME, '.agents', 'skills'),
    project: '.agents/skills',
    notes: 'Codex default in Octocode CLI; shared cross-agent dir',
  },
  claude: {
    id: 'claude',
    label: 'Claude Code',
    user: join(USER_SKILL_HOME, '.claude', 'skills'),
    project: '.claude/skills',
    notes: '',
  },
  cursor: {
    id: 'cursor',
    label: 'Cursor',
    user: join(USER_SKILL_HOME, '.cursor', 'skills'),
    project: '.cursor/skills',
    notes: '',
  },
  opencode: {
    id: 'opencode',
    label: 'OpenCode',
    user: join(USER_SKILL_HOME, '.config', 'opencode', 'skills'),
    project: '.opencode/skills',
    notes: '',
  },
  pi: {
    id: 'pi',
    label: 'Pi',
    user: join(USER_SKILL_HOME, '.pi', 'agent', 'skills'),
    project: '.pi/skills',
    notes: 'pi install /local/path may store a path reference instead of copying',
  },
  copilot: {
    id: 'copilot',
    label: 'GitHub Copilot',
    user: join(USER_SKILL_HOME, '.copilot', 'skills'),
    project: '.github/skills',
    notes: '',
  },
  gemini: {
    id: 'gemini',
    label: 'Gemini CLI',
    user: join(USER_SKILL_HOME, '.gemini', 'skills'),
    project: '.gemini/skills',
    notes: 'Also honors ~/.agents/skills on some setups',
  },
};

export const TOP = ['claude', 'cursor', 'codex'];
export const PRIMARY = [
  'codex',
  'claude',
  'cursor',
  'opencode',
  'pi',
  'copilot',
  'gemini',
];
export const VENDOR_IDS = Object.keys(VENDORS);

export function resolvePlatformIds(spec) {
  const raw = String(spec || 'top').trim().toLowerCase();
  if (!raw || raw === 'top') return [...TOP];
  if (raw === 'all') return [...PRIMARY];
  const ids = raw.split(',').map((s) => s.trim()).filter(Boolean);
  const bad = ids.filter((id) => !VENDORS[id]);
  if (bad.length) {
    console.error(`Unknown platform(s): ${bad.join(', ')}. Valid: ${VENDOR_IDS.join(', ')}, top, all`);
    process.exit(1);
  }
  return [...new Set(ids)];
}

export function printVendors(asJson) {
  const list = VENDOR_IDS.map((id) => {
    const v = VENDORS[id];
    return {
      id,
      label: v.label,
      user: v.user,
      userRelativePath: relative(USER_SKILL_HOME, v.user).split(sep).join('/'),
      project: v.project,
      notes: v.notes,
      top: TOP.includes(id),
    };
  });
  if (asJson) {
    console.log(
      JSON.stringify(
        { vendors: list, top: TOP },
        null,
        2
      )
    );
    return;
  }
  console.log('Vendor skill locations (user scope):\n');
  for (const v of list) {
    const mark = v.top ? ' [top]' : '';
    console.log(`${v.id}${mark}`);
    console.log(`  ${v.label}`);
    console.log(`  user:    ${v.user}`);
    console.log(`  project: ${v.project ? `<repo>/${v.project}` : '(none)'}`);
    if (v.notes) console.log(`  notes:   ${v.notes}`);
    console.log('');
  }
  console.log(`top = ${TOP.join(',')}`);
}
