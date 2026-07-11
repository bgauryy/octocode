import type { MarketplaceSkill } from '../../configs/skills-marketplace.js';
import { z } from '@octocodeai/octocode-tools-core/zod';

export interface GitHubTreeItem {
  path: string;
  mode: string;
  type: 'blob' | 'tree';
  sha: string;
  size?: number;
  url: string;
}

export interface GitHubTreeResponse {
  sha: string;
  url: string;
  tree: GitHubTreeItem[];
  truncated: boolean;
}

export const CachedSkillsDataSchema = z.object({
  timestamp: z.number(),
  skills: z.array(z.object({}).passthrough()),
});

export type CachedSkillsData = {
  timestamp: number;
  skills: MarketplaceSkill[];
};

export interface SkillsShResult {
  id: string;
  skillId: string;
  name: string;
  installs: number;

  source: string;
}

export interface SkillsShSearchResponse {
  results: SkillsShResult[];
  count: number;
}
