import type { MarketplaceSkill } from '../../configs/skills-marketplace.js';

export function searchSkills(
  skills: MarketplaceSkill[],
  query: string
): MarketplaceSkill[] {
  const lowerQuery = query.toLowerCase();
  return skills.filter(
    skill =>
      skill.name.toLowerCase().includes(lowerQuery) ||
      skill.displayName.toLowerCase().includes(lowerQuery) ||
      skill.description.toLowerCase().includes(lowerQuery) ||
      skill.category?.toLowerCase().includes(lowerQuery)
  );
}

export function groupSkillsByCategory(
  skills: MarketplaceSkill[]
): Map<string, MarketplaceSkill[]> {
  const grouped = new Map<string, MarketplaceSkill[]>();

  for (const skill of skills) {
    const category = skill.category || 'Other';
    if (!grouped.has(category)) {
      grouped.set(category, []);
    }
    grouped.get(category)!.push(skill);
  }

  return grouped;
}
