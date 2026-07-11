import { c, bold, dim } from '../../../utils/colors.js';
import { select, search } from '../../../utils/prompts.js';
import { separatorChoice } from '../../../utils/prompt-separator.js';
import {
  SKILLS_MARKETPLACES,
  type MarketplaceSource,
  type MarketplaceSkill,
  isLocalSource,
} from '../../../configs/skills-marketplace.js';
import {
  formatMarketplace,
  formatSkill,
  isSkillInstalled,
  RECOMMENDED_SKILLS,
  type MarketplaceMenuChoice,
  type SkillMenuChoice,
} from './helpers.js';

export async function selectMarketplace(
  starsMap: Map<string, number>
): Promise<MarketplaceMenuChoice> {
  console.log();
  console.log(`  ${bold('Select a marketplace to browse:')}`);
  console.log();

  const localSources = SKILLS_MARKETPLACES.filter(s => isLocalSource(s));
  const githubSources = SKILLS_MARKETPLACES.filter(s => !isLocalSource(s));

  const sortedGitHubSources = [...githubSources].sort(
    (a, b) => (starsMap.get(b.id) ?? 0) - (starsMap.get(a.id) ?? 0)
  );

  const sortedMarketplaces = [...localSources, ...sortedGitHubSources];

  const choices: Array<{
    name: string;
    value: MarketplaceMenuChoice;
    description?: string;
  }> = [];

  for (const source of sortedMarketplaces) {
    choices.push({
      name: formatMarketplace(source, starsMap.get(source.id)),
      value: source,
      description: dim(source.url),
    });
  }

  choices.push(
    separatorChoice<{
      name: string;
      value: MarketplaceMenuChoice;
    }>()
  );
  choices.push({
    name: `${c('dim', '- Back to skills menu')}`,
    value: 'back',
  });

  const choice = await select<MarketplaceMenuChoice>({
    message: '',
    choices,
    pageSize: 10,
    loop: false,
    theme: {
      prefix: '  ',
      style: {
        highlight: (text: string) => c('magenta', text),
      },
    },
  });

  return choice;
}

export async function browseSkills(
  source: MarketplaceSource,
  skills: MarketplaceSkill[]
): Promise<SkillMenuChoice> {
  console.log();
  console.log(`  ${bold(source.name)} - ${skills.length} skills available`);
  console.log(`  ${dim(source.url)}`);
  console.log();

  const sortedSkills = [...skills].sort((a, b) => {
    const aInstalled = isSkillInstalled(a.name);
    const bInstalled = isSkillInstalled(b.name);
    if (aInstalled !== bInstalled) {
      return aInstalled ? -1 : 1;
    }

    const aRecommended = RECOMMENDED_SKILLS.has(a.name);
    const bRecommended = RECOMMENDED_SKILLS.has(b.name);
    if (aRecommended !== bRecommended) {
      return aRecommended ? -1 : 1;
    }

    return a.displayName.localeCompare(b.displayName);
  });

  const skillChoices = sortedSkills.map(skill => {
    const installed = isSkillInstalled(skill.name);
    return {
      name: formatSkill(skill, installed),
      value: skill as SkillMenuChoice,

      description: skill.category ? `[${skill.category}]` : undefined,
    };
  });

  const backChoice = {
    name: `${c('dim', '- Back to marketplaces')}`,
    value: 'back' as SkillMenuChoice,
  };

  const choice = await search<SkillMenuChoice>({
    message: `Type to filter skills (${skills.length} available)`,
    source: (term: string | undefined) => {
      if (!term || !term.trim()) {
        return [...skillChoices, backChoice];
      }

      const lowerTerm = term.toLowerCase();
      const filtered = skillChoices.filter(choice => {
        if (typeof choice.value === 'string') return false;
        const skill = choice.value;
        return (
          skill.name.toLowerCase().includes(lowerTerm) ||
          skill.displayName.toLowerCase().includes(lowerTerm) ||
          skill.description.toLowerCase().includes(lowerTerm) ||
          skill.category?.toLowerCase().includes(lowerTerm)
        );
      });

      return [...filtered, backChoice];
    },
    pageSize: 20,
    theme: {
      prefix: '  ',
      style: {
        highlight: (text: string) => c('magenta', text),
      },
    },
  });

  return choice;
}
