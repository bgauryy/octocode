import { z } from "zod";

export const PRIMARY_DOMAIN_ORDER = ["ai", "devtools", "web", "security", "repos"] as const;
export const OPTIONAL_DOMAIN_ORDER = ["cross"] as const;
export const DOMAIN_ORDER = [...PRIMARY_DOMAIN_ORDER, ...OPTIONAL_DOMAIN_ORDER] as const;
export const THEME_ORDER = ["ai", "tech", "security", "repositories", "others"] as const;
export const DOMAIN_THEME_MAP = {
  ai: "ai",
  devtools: "tech",
  web: "tech",
  security: "security",
  repos: "repositories",
  cross: "others"
} as const;

export const DomainId = z.enum(DOMAIN_ORDER);
export const ItemType = z.enum(["release", "advisory", "blog", "changelog", "newsletter", "repo"]);
export const COMPACT_COPY_HEAT_THRESHOLD = 45;
export const IMPORTANT_COPY_HEAT_THRESHOLD = 70;
export const IconClass = z.enum([
  "si-ai",
  "si-dt",
  "si-web",
  "si-sec",
  "si-repo",
  "si-cross",
  "si-neutral"
]);

export const ContentEvidence = z.object({
  method: z.literal("full-page"),
  chars: z.number().int().min(200),
  fetchedAt: z.string().datetime({ offset: true }).optional()
});

export const Reference = z.union([
  z.string().url(),
  z
    .object({
      label: z.string().min(1).optional(),
      title: z.string().min(1).optional(),
      source: z.string().min(1).optional(),
      url: z.string().url(),
      kind: z.string().min(1).optional()
    })
    .passthrough()
]);

function cleanText(value: unknown) {
  return typeof value === "string" ? value.trim() : "";
}

function truncateAtWordBoundary(value: string, maxChars: number) {
  const text = cleanText(value).replace(/\s+/g, " ");
  if (!text || text.length <= maxChars) {
    return text;
  }

  const clipped = text.slice(0, maxChars - 1);
  const boundary = clipped.lastIndexOf(" ");
  const base = boundary >= Math.floor(maxChars * 0.6) ? clipped.slice(0, boundary) : clipped;
  return `${base.trimEnd()}...`;
}

function themeForDomain(domain: string) {
  return DOMAIN_THEME_MAP[domain as keyof typeof DOMAIN_THEME_MAP] || "others";
}

function countValues(values: string[]) {
  return values.reduce<Record<string, number>>((acc, value) => {
    const key = cleanText(value);
    if (!key) {
      return acc;
    }
    acc[key] = (acc[key] || 0) + 1;
    return acc;
  }, {});
}

function uniqueCount(values: string[]) {
  return new Set(values.map((value) => cleanText(value)).filter(Boolean)).size;
}

function latestDateISO(items: Array<Record<string, unknown>>) {
  return items.reduce((latest, item) => {
    const candidate = cleanText(item.dateISO) || cleanText(item.date);
    return candidate > latest ? candidate : latest;
  }, "");
}

function referenceCount(item: Record<string, unknown>) {
  return Array.isArray(item.references) ? item.references.length : 0;
}

function buildSectionStats(items: Array<Record<string, unknown>>, sourceCheckCount: number) {
  return {
    itemCount: items.length,
    importantCount: items.filter(
      (item) => typeof item.heat === "number" && item.heat >= IMPORTANT_COPY_HEAT_THRESHOLD
    ).length,
    sourceCount: uniqueCount(items.map((item) => String(item.source || ""))),
    sourceCheckCount,
    referenceCount: items.reduce((sum, item) => sum + referenceCount(item), 0),
    latestDateISO: latestDateISO(items),
    typeCounts: countValues(items.map((item) => String(item.type || "")))
  };
}

function buildReportMeta(
  topItems: Array<Record<string, unknown>>,
  sections: Array<Record<string, unknown>>,
  sourcesChecked: Array<Record<string, unknown>>
) {
  const sectionItems = sections.flatMap((section) =>
    Array.isArray(section.items) ? section.items : []
  ) as Array<Record<string, unknown>>;
  const allItems = [...topItems, ...sectionItems];

  return {
    totalItems: allItems.length,
    highlightCount: topItems.length,
    sourcesChecked: sourcesChecked.length,
    surfacedDomains: uniqueCount(allItems.map((item) => String(item.domain || ""))),
    latestDateISO: latestDateISO(allItems),
    totalReferences: allItems.reduce((sum, item) => sum + referenceCount(item), 0),
    domainCounts: countValues(allItems.map((item) => String(item.domain || ""))),
    themeCounts: countValues(allItems.map((item) => themeForDomain(String(item.domain || "")))),
    typeCounts: countValues(allItems.map((item) => String(item.type || "")))
  };
}

function normalizeItemCopy<T extends Record<string, unknown>>(item: T) {
  const whyImportant = cleanText(item.whyImportant) || cleanText(item.whyInteresting);
  const whyInteresting = cleanText(item.whyInteresting) || whyImportant;
  const heat = typeof item.heat === "number" ? item.heat : Number.NaN;
  const theme = themeForDomain(cleanText(item.domain));
  const shouldAddCompactCopy =
    Boolean(cleanText(item.shortTitle) || cleanText(item.shortDescription)) ||
    (Number.isFinite(heat) && heat < COMPACT_COPY_HEAT_THRESHOLD);

  return {
    ...item,
    whyImportant,
    whyInteresting,
    theme,
    referenceCount: referenceCount(item),
    ...(shouldAddCompactCopy
      ? {
          shortTitle:
            cleanText(item.shortTitle) || truncateAtWordBoundary(String(item.title || ""), 72),
          shortDescription:
            cleanText(item.shortDescription) ||
            truncateAtWordBoundary(String(item.summary || ""), 180)
        }
      : {})
  };
}

export const Item = z
  .object({
    domain: DomainId,
    type: ItemType,
    title: z.string().min(1),
    shortTitle: z.string().min(1).optional(),
    link: z.string().url(),
    source: z.string().min(1),
    sourceUrl: z.string().url().optional(),
    summary: z.string().min(1),
    shortDescription: z.string().min(1).optional(),
    whyImportant: z.string().min(1).optional(),
    whyInteresting: z.string().min(1).optional(),
    references: z.array(Reference).default([]),
    date: z.string().min(1),
    dateISO: z.string().date(),
    heat: z.number().int().min(0).max(100),
    layer: z.number().int().min(0).max(4).optional(),
    stars: z.string().optional(),
    imageUrl: z.string().url().optional(),
    contentEvidence: ContentEvidence
  })
  .passthrough()
  .superRefine((item, ctx) => {
    if (!cleanText(item.whyImportant) && !cleanText(item.whyInteresting)) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: "Items must include whyImportant or whyInteresting.",
        path: ["whyImportant"]
      });
    }
  });

export const SourceCheck = z
  .object({
    source: z.string().min(1),
    status: z.string().min(1),
    found: z.union([z.boolean(), z.number().int().min(0), z.string().min(1)]),
    notes: z.string().default(""),
    url: z.string().url().optional(),
    domain: DomainId.optional(),
    category: z.string().min(1).optional()
  })
  .passthrough();

export const Section = z
  .object({
    id: DomainId,
    name: z.string().min(1),
    icon: z.string().min(1),
    iconClass: IconClass,
    quiet: z.boolean(),
    quietMsg: z.string().optional(),
    items: z.array(Item).default([])
  })
  .passthrough()
  .superRefine((section, ctx) => {
    if (section.quiet && section.items.length > 0) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: "Quiet sections must not include items.",
        path: ["items"]
      });
    }

    for (const [index, item] of section.items.entries()) {
      if (item.domain !== section.id) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          message: `Section item domain must match section id (${section.id}).`,
          path: ["items", index, "domain"]
        });
      }
    }
  });

export const ReportData = z
  .object({
    window: z.string().min(1),
    windowLabel: z.enum(["24h", "48h", "7d", "14d", "30d"]),
    generated: z.string().date(),
    tldr: z.string().min(120),
    topItems: z.array(Item).min(3).max(30),
    sections: z.array(Section).min(PRIMARY_DOMAIN_ORDER.length).max(DOMAIN_ORDER.length),
    sourcesChecked: z.array(SourceCheck).default([])
  })
  .passthrough()
  .superRefine((data, ctx) => {
    const ids = data.sections.map((section) => section.id);
    const uniqueIds = new Set(ids);

    if (uniqueIds.size !== ids.length) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: "Sections must not repeat domain ids.",
        path: ["sections"]
      });
    }

    const missing = PRIMARY_DOMAIN_ORDER.filter((id) => !uniqueIds.has(id));
    if (missing.length > 0) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: `Missing required sections: ${missing.join(", ")}.`,
        path: ["sections"]
      });
    }

    const sectionLinks = new Set(
      data.sections.flatMap((section) => section.items.map((item) => item.link))
    );
    const duplicatedHeroLinks = data.topItems
      .map((item) => item.link)
      .filter((link) => sectionLinks.has(link));

    if (duplicatedHeroLinks.length > 0) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: "Hero items must not be duplicated inside section items.",
        path: ["topItems"]
      });
    }

    const totalItems =
      data.topItems.length + data.sections.reduce((sum, section) => sum + section.items.length, 0);

    if (totalItems < 3 || totalItems > 300) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: "Total item count must stay between 3 and 300.",
        path: ["sections"]
      });
    }
  });

export const SectionPayload = z
  .object({
    id: DomainId,
    name: z.string().min(1),
    icon: z.string().min(1),
    iconClass: IconClass,
    quiet: z.boolean(),
    quietMsg: z.string().optional(),
    items: z.array(Item).default([]),
    sourcesChecked: z.array(SourceCheck).default([])
  })
  .passthrough();

export const ReportMeta = z
  .object({
    window: z.string().min(1),
    windowLabel: z.enum(["24h", "48h", "7d", "14d", "30d"]),
    generated: z.string().date(),
    tldr: z.string().min(120),
    topItems: z.array(Item).min(3).max(30),
    sourcesChecked: z.array(SourceCheck).default([])
  })
  .passthrough();

export function mergeFromSectionDir(
  meta: Record<string, unknown>,
  sectionFiles: Array<Record<string, unknown>>
) {
  const parsedMeta = ReportMeta.parse(meta);
  const parsedSections = sectionFiles.map((file) => SectionPayload.parse(file));

  const allSourcesChecked = [
    ...parsedMeta.sourcesChecked,
    ...parsedSections.flatMap((s) => s.sourcesChecked)
  ];

  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  const sections = parsedSections.map(({ sourcesChecked, ...rest }) => rest);

  return {
    ...parsedMeta,
    sections,
    sourcesChecked: allSourcesChecked
  };
}

export function normalizeReportData(input) {
  const parsed = ReportData.parse(input);
  const topItems = parsed.topItems.map((item) => normalizeItemCopy(item));
  const sectionMap = new Map(parsed.sections.map((section) => [section.id, section]));
  const orderedSections = DOMAIN_ORDER.flatMap((id) => {
    const section = sectionMap.get(id);
    return section
      ? [
          {
            ...section,
            theme: themeForDomain(id),
            items: section.items.map((item) => normalizeItemCopy(item)),
            stats: buildSectionStats(
              section.items as Array<Record<string, unknown>>,
              parsed.sourcesChecked.filter((entry) => entry.domain === id).length
            )
          }
        ]
      : [];
  });

  return {
    ...parsed,
    reportMeta: buildReportMeta(
      topItems as Array<Record<string, unknown>>,
      orderedSections as Array<Record<string, unknown>>,
      parsed.sourcesChecked as Array<Record<string, unknown>>
    ),
    topItems,
    sections: orderedSections
  };
}

export function splitNormalizedReport(normalized: ReturnType<typeof normalizeReportData>) {
  const { sections, ...meta } = normalized;
  const sectionMap = new Map((sections as Array<{ id: string }>).map((s) => [s.id, s]));
  return { meta, sectionMap };
}
