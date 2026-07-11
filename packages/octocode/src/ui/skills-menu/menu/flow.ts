import { c, dim } from '../../../utils/colors.js';
import { loadInquirer } from '../../../utils/prompts.js';
import { runMarketplaceFlow } from '../marketplace.js';
import { getAllInstalledSkills, getSkillsInfo } from './data.js';
import {
  pressEnterToContinue,
  showSkillsMenu,
  selectInstalledSkill,
  showSkillActions,
} from './prompts.js';
import { openSkillLocation, removeSkill } from './actions.js';

async function manageInstalledSkills(): Promise<void> {
  let inManageMenu = true;

  while (inManageMenu) {
    const installedSkills = getAllInstalledSkills();

    if (installedSkills.length === 0) {
      console.log();
      console.log(`  ${c('yellow', 'INFO')} No skills installed`);
      console.log(`  ${dim('Browse the marketplace to install skills')}`);
      console.log();
      await pressEnterToContinue();
      return;
    }

    const selectedSkill = await selectInstalledSkill(installedSkills);

    if (selectedSkill === 'back') {
      inManageMenu = false;
      continue;
    }

    let inSkillActions = true;
    while (inSkillActions) {
      const action = await showSkillActions(selectedSkill);

      switch (action) {
        case 'remove': {
          const removed = await removeSkill(selectedSkill);
          if (removed) {
            await pressEnterToContinue();
            inSkillActions = false;
          }
          break;
        }

        case 'view':
          await openSkillLocation(selectedSkill);
          await pressEnterToContinue();
          break;

        case 'back':
        default:
          inSkillActions = false;
          break;
      }
    }
  }
}

export async function runSkillsMenu(): Promise<void> {
  await loadInquirer();

  let info = getSkillsInfo();

  if (!info.sourceExists) {
    console.log(`  ${c('yellow', 'WARN')} Skills source directory not found.`);
    console.log(`  ${dim('This may happen if running from source.')}`);
    console.log();
    await pressEnterToContinue();
    return;
  }

  if (info.skillsStatus.length === 0) {
    console.log(`  ${dim('No skills available.')}`);
    console.log();
    await pressEnterToContinue();
    return;
  }

  let inSkillsMenu = true;
  while (inSkillsMenu) {
    info = getSkillsInfo();

    const installedSkills = getAllInstalledSkills();
    const installedCount = installedSkills.length;

    const choice = await showSkillsMenu(installedCount);

    switch (choice) {
      case 'manage':
        await manageInstalledSkills();
        break;

      case 'marketplace':
        await runMarketplaceFlow();
        break;

      case 'back':
      default:
        inSkillsMenu = false;
        break;
    }
  }
}
