// The HAR 1.2 fields the HAR readers in this folder touch. A HAR may come from Chrome or another
// tool, so every field is optional; text fields that the readers stringify may hold any JSON scalar.
import type { JsonScalar } from './measure-types.mjs';

export type HarHeader = { name?: string; Name?: string; value?: JsonScalar };

/** A cookie, query parameter or form parameter row. */
export type HarParam = { name: string; value?: JsonScalar; fileName?: string };

export type HarPostData = {
  mimeType?: string;
  text?: string;
  params?: HarParam[];
  comment?: string;
};

export type HarContent = {
  mimeType?: string;
  size?: number;
  text?: string;
  encoding?: string;
  comment?: string;
};

export type HarEntry = {
  startedDateTime?: string;
  time?: number;
  request?: {
    method?: string;
    url?: string;
    headers?: HarHeader[];
    cookies?: HarParam[];
    queryString?: HarParam[];
    postData?: HarPostData;
  };
  response?: {
    status?: number;
    redirectURL?: string;
    headers?: HarHeader[];
    cookies?: HarParam[];
    content?: HarContent;
    bodySize?: number;
  };
  // HAR custom fields carry a leading underscore by spec.
  _resourceType?: JsonScalar;
  _failed?: unknown;
  _errorText?: string;
  _blockedReason?: string;
};

/** A parsed HAR file, shaped as the spec says; readers still guard `log.entries` before paging it. */
export type HarFile = { log?: { entries?: HarEntry[] } };
