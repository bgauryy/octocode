import { bold, c, dim } from '../../../utils/colors.js';
import { fetchMarketplaceSkills } from '../../../utils/skills-fetch.js';
import { Spinner } from '../../../utils/spinner.js';
import {
  KNOWN_OCTOCODE_SKILLS,
  OCTOCODE_SKILLS_SOURCE,
  RECOMMENDED_SKILL,
} from './types.js';

export async function runListCommand(jsonOutput: boolean): Promise<void> {
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

  if (jsonOutput) {
    const payload = fetchFailed
      ? {
          success: false,
          source: OCTOCODE_SKILLS_SOURCE.url,
          skills: KNOWN_OCTOCODE_SKILLS.map(n => ({ name: n })),
          fallback: true,
        }
      : {
          success: true,
          source: OCTOCODE_SKILLS_SOURCE.url,
          skills: skills.map(s => ({
            name: s.name,
            displayName: s.displayName,
            description: s.description,
          })),
        };
    console.log(JSON.stringify(payload));
    return;
  }

  console.log();
  if (fetchFailed) {
    console.log(
      `  ${bold('Octocode skills')}  ${dim('(live list unavailable — showing known names)')}`
    );
    console.log();
    console.log(`  ${KNOWN_OCTOCODE_SKILLS.join('  ')}`);
  } else {
    console.log(
      `  ${bold('Available Octocode skills')}  ${dim('·')}  ${dim(OCTOCODE_SKILLS_SOURCE.url)}`
    );
    console.log();
    const nameWidth = Math.max(...skills.map(s => s.name.length)) + 2;
    for (const s of skills) {
      const star = s.name === RECOMMENDED_SKILL ? c('yellow', '⭐') : '  ';
      console.log(
        `  ${star}  ${s.name.padEnd(nameWidth)}${dim(s.description)}`
      );
    }
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
