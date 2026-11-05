import { astSearch } from './spec-ast.mjs';
import { ghGetFileContent, localFetch } from './spec-fetch.mjs';
import { ghGetHistoryItem, ghSearchCode, ghSearchHistory, ghSearchRepo, ghStructure } from './spec-github.mjs';
import { graphBuild, graphQuery } from './spec-graph.mjs';
import { localSearch } from './spec-local-search.mjs';
import { lspSearch } from './spec-lsp.mjs';
import { artifactSearch, clasify, ghCloneRepo, securityCheck } from './spec-registry.mjs';
import { structureSearch } from './spec-structure.mjs';

export const SPECS = { localSearch, structureSearch, astSearch, localFetch, ghGetFileContent, lspSearch, graphBuild, graphQuery, ghSearchRepo, ghSearchHistory, ghGetHistoryItem, ghStructure, ghSearchCode, artifactSearch, securityCheck, clasify, ghCloneRepo };
