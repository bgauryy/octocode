import { bold, c, dim } from '../../../utils/colors.js';
import { fetchMarketplaceSkills } from '../../../utils/skills-fetch.js';
import { getAllSkillsMetadata } from '../../../utils/skills.js';
import { Spinner } from '../../../utils/spinner.js';
import { formatSkillName } from './naming.js';
import {
  KNOWN_OCTOCODE_SKILLS,
  OCTOCODE_SKILLS_SOURCE,
  RECOMMENDED_SKILL,
} from './types.js';

type ListedSkill = { name: string; displayName: string; description: string };

/**
 * Skills shipped on disk with this exact CLI install (packages/octocode/skills,
 * see build.mjs). Preferred over a GitHub fetch: it works offline, never rate
 * limits, and always matches what --install-all / --name would actually
 * install for this version — unlike a live `main` branch fetch, which can
 * drift ahead of or behind the installed CLI.
 */
function getBundledSkillsList(): ListedSkill[] {
  try {
    return getAllSkillsMetadata()
      .map(s => ({
        name: s.folder,
        displayName: formatSkillName(s.folder),
        description: s.description,
      }))
      .sort((a, b) => a.name.localeCompare(b.name));
  } catch {
    return [];
  }
}

function printSkillsList(params: {
  jsonOutput: boolean;
  skills: ListedSkill[];
  source: string;
  offline: boolean;
}): void {
  const { jsonOutput, skills, source, offline } = params;

  if (jsonOutput) {
    console.log(JSON.stringify({ success: true, source, offline, skills }));
    return;
  }

  console.log();
  console.log(
    `  ${bold('Available Octocode skills')}  ${dim('·')}  ${dim(source)}`
  );
  console.log();
  const nameWidth = Math.max(...skills.map(s => s.name.length)) + 2;
  for (const s of skills) {
    const star = s.name === RECOMMENDED_SKILL ? c('yellow', '⭐') : '  ';
    console.log(`  ${star}  ${s.name.padEnd(nameWidth)}${dim(s.description)}`);
  }
  console.log();
  console.log(`  ${dim('Install:')}  octocode skill --name <skill-name>`);
  console.log(`  ${dim('Install all:')}  octocode skill --install-all`);
  console.log(`  ${dim('Example:')}  octocode skill --name octocode-research`);
  console.log(
    `  ${dim('Example:')}  octocode skill --add owner/repo/skills --platform common`
  );
  console.log();
}

export async function runListCommand(jsonOutput: boolean): Promise<void> {
  // Prefer the skills bundled with this install (offline, version-accurate)
  // before falling back to a live GitHub fetch of the official skills repo.
  const bundled = getBundledSkillsList();
  if (bundled.length > 0) {
    printSkillsList({
      jsonOutput,
      skills: bundled,
      source: 'bundled with this octocode install',
      offline: true,
    });
    return;
  }

  const spinner = jsonOutput
    ? null
    : new Spinner('Fetching Octocode skills list...').start();

  let skills: Awaited<ReturnType<typeof fetchMarketplaceSkills>> = [];
  let fetchFailed = false;

  try {
    skills = await fetchMarketplaceSkills(OCTOCODE_SKILLS_SOURCE);
  } catch {
    fetchFailed = true;
  }

  spinner?.stop();

  if (fetchFailed || skills.length === 0) {
    const knownSkills = KNOWN_OCTOCODE_SKILLS.map(n => ({
      name: n,
      displayName: formatSkillName(n),
      description: 'Official Octocode skill',
    }));

    if (jsonOutput) {
      console.log(
        JSON.stringify({
          success: false,
          source: OCTOCODE_SKILLS_SOURCE.url,
          skills: knownSkills,
          fallback: true,
        })
      );
      return;
    }

    console.log();
    console.log(
      `  ${bold('Octocode skills')}  ${dim('(live list unavailable — showing known names)')}`
    );
    console.log();
    console.log(`  ${KNOWN_OCTOCODE_SKILLS.join('  ')}`);
    console.log();
    console.log(`  ${dim('Install:')}  octocode skill --name <skill-name>`);
    console.log(`  ${dim('Install all:')}  octocode skill --install-all`);
    console.log(
      `  ${dim('Example:')}  octocode skill --name octocode-research`
    );
    console.log(
      `  ${dim('Example:')}  octocode skill --add owner/repo/skills --platform common`
    );
    console.log();
    return;
  }

  printSkillsList({
    jsonOutput,
    skills: skills.map(s => ({
      name: s.name,
      displayName: s.displayName,
      description: s.description,
    })),
    source: OCTOCODE_SKILLS_SOURCE.url,
    offline: false,
  });
}
