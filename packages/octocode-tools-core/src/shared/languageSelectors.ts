export type {
  LanguageSelectorKind,
  LanguageSelector,
} from './languageSelectors/data.js';

export {
  classifyLanguageSelector,
  toGithubCodeLanguageParams,
  toLocalSearchLanguageParams,
  toLocalFileLanguageGlobs,
  toStructuralSearchIncludeGlobs,
  toGithubRepositoryLanguage,
} from './languageSelectors/classify.js';

export type {
  GithubCodeLanguageParams,
  LocalSearchLanguageParams,
} from './languageSelectors/classify.js';
