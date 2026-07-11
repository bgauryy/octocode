import { c, dim } from '../../../utils/colors.js';
import { Spinner } from '../../../utils/spinner.js';
import {
  SKILLS_MARKETPLACES,
  type MarketplaceSkill,
  fetchAllMarketplaceStars,
} from '../../../configs/skills-marketplace.js';
import { fetchMarketplaceSkills } from '../../../utils/skills-fetch.js';
import { selectMarketplace, browseSkills } from './browse.js';
import { showSkillDetails, installSkill, deleteSkill } from './actions.js';
import { showOfficialFlowMenu, installAllSkills } from './official-flow.js';
import { isSkillInstalled, pressEnterToContinue } from './helpers.js';

export async function runMarketplaceFlow(): Promise<void> {
  console.log();
  console.log(
    `  ${c('yellow', 'WARN')} ${dim('Community list • Skills install on your behalf')}`
  );

  const starsSpinner = new Spinner('Fetching marketplace info...').start();
  let starsMap: Map<string, number>;
  try {
    starsMap = await fetchAllMarketplaceStars();
    starsSpinner.succeed('Loaded marketplace info');
  } catch {
    starsSpinner.fail('Could not fetch stars');
    starsMap = new Map();
  }

  let inMarketplace = true;

  while (inMarketplace) {
    const source = await selectMarketplace(starsMap);

    if (source === 'back') {
      inMarketplace = false;
      continue;
    }

    console.log();
    const spinner = new Spinner(
      `Loading skills from ${source.name}...`
    ).start();

    let skills: MarketplaceSkill[];
    try {
      skills = await fetchMarketplaceSkills(source);
      spinner.stop();
    } catch (error) {
      spinner.fail(`Failed to load skills`);
      console.log();
      console.log(
        `  ${c('red', 'X')} ${error instanceof Error ? error.message : 'Unknown error'}`
      );
      console.log();
      await pressEnterToContinue();
      continue;
    }

    if (skills.length === 0) {
      console.log();
      console.log(
        `  ${c('yellow', 'WARN')} No skills found in this marketplace`
      );
      console.log();
      await pressEnterToContinue();
      continue;
    }

    let inSkillsBrowser = true;
    while (inSkillsBrowser) {
      const skillChoice = await browseSkills(source, skills);

      if (skillChoice === 'back') {
        inSkillsBrowser = false;
        continue;
      }

      const detailChoice = await showSkillDetails(skillChoice);
      if (detailChoice === 'install') {
        await installSkill(skillChoice);
      } else if (detailChoice === 'delete') {
        await deleteSkill(skillChoice);
      }
    }
  }
}

export async function runOctocodeSkillsFlow(): Promise<void> {
  const source = SKILLS_MARKETPLACES.find(s => s.id === 'octocode-skills');
  if (!source) {
    console.log();
    console.log(`  ${c('red', 'X')} Octocode Skills source not found`);
    console.log();
    await pressEnterToContinue();
    return;
  }

  console.log();
  const spinner = new Spinner(`Loading Octocode Skills...`).start();

  let skills: MarketplaceSkill[];
  try {
    skills = await fetchMarketplaceSkills(source);
    spinner.stop();
  } catch (error) {
    spinner.fail(`Failed to load skills`);
    console.log();
    console.log(
      `  ${c('red', 'X')} ${error instanceof Error ? error.message : 'Unknown error'}`
    );
    console.log();
    await pressEnterToContinue();
    return;
  }

  if (skills.length === 0) {
    console.log();
    console.log(`  ${c('yellow', 'WARN')} No skills found`);
    console.log();
    await pressEnterToContinue();
    return;
  }

  let inFlow = true;
  while (inFlow) {
    const notInstalledCount = skills.filter(
      s => !isSkillInstalled(s.name)
    ).length;

    const menuChoice = await showOfficialFlowMenu(
      skills.length,
      notInstalledCount
    );

    switch (menuChoice) {
      case 'install-all':
        await installAllSkills(skills);
        inFlow = false;
        break;

      case 'browse': {
        let inSkillsBrowser = true;
        while (inSkillsBrowser) {
          const skillChoice = await browseSkills(source, skills);
          if (skillChoice === 'back') {
            inSkillsBrowser = false;
            continue;
          }
          const detailChoice = await showSkillDetails(skillChoice);
          if (detailChoice === 'install') {
            await installSkill(skillChoice);
          } else if (detailChoice === 'delete') {
            await deleteSkill(skillChoice);
          }
        }
        break;
      }

      case 'back':
      default:
        inFlow = false;
        break;
    }
  }
}
