# Design Resources & Inspiration

Curated repos for design systems, color palettes, animations, frameworks, and 3D. Use these as reference when building `DESIGN.md` or choosing tooling.

---

## Component Libraries & Design Systems

### shadcn/ui — Open-Code Component System
**Repo:** [shadcn-ui/ui](https://github.com/shadcn-ui/ui) | 111k+ stars
**Use for:** Default component library recommendation. Copy-paste components built on Radix primitives + Tailwind CSS.

| What to reference | Where |
|-------------------|-------|
| Component patterns | [skills/shadcn/SKILL.md](https://github.com/shadcn-ui/ui/blob/main/skills/shadcn/SKILL.md) |
| Styling rules | [skills/shadcn/rules/styling.md](https://github.com/shadcn-ui/ui/blob/main/skills/shadcn/rules/styling.md) |
| Form patterns | [skills/shadcn/rules/forms.md](https://github.com/shadcn-ui/ui/blob/main/skills/shadcn/rules/forms.md) |
| Composition rules | [skills/shadcn/rules/composition.md](https://github.com/shadcn-ui/ui/blob/main/skills/shadcn/rules/composition.md) |
| Customization | [skills/shadcn/customization.md](https://github.com/shadcn-ui/ui/blob/main/skills/shadcn/customization.md) |

**Key patterns:** Semantic tokens (`bg-primary`, not `bg-blue-500`), `gap-*` not `space-*`, `size-*` for equal dimensions, `cn()` for conditional classes, `FieldGroup`/`Field` for forms, `asChild` for custom triggers.

---

### Material UI — Material Design for React
**Repo:** [mui/material-ui](https://github.com/mui/material-ui) | 98k+ stars
**Use for:** Enterprise apps needing Material Design 3 compliance. Full component suite with theming engine.

| What to reference | Where |
|-------------------|-------|
| Component docs | [docs/pages/material-ui](https://github.com/mui/material-ui/tree/master/docs/pages/material-ui) |
| System utilities | [docs/pages/system](https://github.com/mui/material-ui/tree/master/docs/pages/system) |
| Theme configuration | `docs/` Next.js app at [mui.com/material-ui](https://mui.com/material-ui/) |

**When to choose:** Projects that need a comprehensive, battle-tested component library with built-in theming, RTL support, and enterprise-grade accessibility.

---

### Material Web — Google's MD3 Web Components
**Repo:** [material-components/material-web](https://github.com/material-components/material-web)
**Use for:** Framework-agnostic Material Design 3 implementation using native Web Components.

| What to reference | Where |
|-------------------|-------|
| Design tokens | [tokens/](https://github.com/material-components/material-web/tree/main/tokens) |
| Typography system | [typography/](https://github.com/material-components/material-web/tree/main/typography) |
| Color system | [color/](https://github.com/material-components/material-web/tree/main/color) |
| Elevation | [elevation/](https://github.com/material-components/material-web/tree/main/elevation) |
| Components | `button/`, `checkbox/`, `dialog/`, `tabs/`, `textfield/`, etc. |

**When to choose:** Projects using Lit, vanilla JS, or any framework. Token architecture is an excellent reference for structuring design tokens.

---

### DaisyUI — Tailwind CSS Component Library
**Repo:** [saadeghi/daisyui](https://github.com/saadeghi/daisyui) | 35k+ stars
**Use for:** Rapid prototyping with 35+ built-in themes. Pure CSS components on top of Tailwind.

| What to reference | Where |
|-------------------|-------|
| Theme system | [daisyui.com/docs/themes](https://daisyui.com/docs/themes/) |
| Color system | [daisyui.com/docs/colors](https://daisyui.com/docs/colors/) |
| Component list | [daisyui.com/components](https://daisyui.com/components/) |

**Key insight:** DaisyUI's theme system uses semantic color names (`primary`, `secondary`, `accent`, `neutral`, `base-100/200/300`) that swap via a single `data-theme` attribute. Excellent pattern for multi-theme DESIGN.md specs.

**When to choose:** Rapid prototyping, theme-heavy apps, or projects where zero-JS CSS components are preferred.

---

### Chakra UI — Accessible Component System
**Repo:** [chakra-ui/chakra-ui](https://github.com/chakra-ui/chakra-ui) | 40k+ stars
**Use for:** Accessible, composable components with style props and theming.

| What to reference | Where |
|-------------------|-------|
| Theme structure | `packages/` monorepo |
| Compositions | [apps/compositions](https://github.com/chakra-ui/chakra-ui/tree/main/apps/compositions) |
| MCP integration | [apps/mcp](https://github.com/chakra-ui/chakra-ui/tree/main/apps/mcp) |

**When to choose:** Projects needing runtime theme switching, style props API, or strong accessibility defaults.

---

### Primer CSS — GitHub's Design System
**Repo:** [primer/css](https://github.com/primer/css)
**Use for:** Reference for professional, utility-first CSS architecture. Production-proven at GitHub scale.

| What to reference | Where |
|-------------------|-------|
| Design system docs | [primer.style/css](https://primer.style/css) |
| Source modules | [src/](https://github.com/primer/css/tree/main/src) — `core/`, `product/`, `marketing/` |
| React components | [primer/react](https://github.com/primer/react) (recommended over CSS) |

**Key insight:** Primer splits CSS into `core` (base utilities), `product` (app UI), and `marketing` (landing pages). Good pattern for large DESIGN.md scoping.

---

## Color Palettes & Themes

### Nord — Arctic Color Palette
**Repo:** [nordtheme/nord](https://github.com/nordtheme/nord) | 6k+ stars
**Use for:** Calm, professional, arctic-inspired color palette. 16 colors in 4 groups.

**Palette (from `src/nord.css`):**

| Group | Colors | Hex Values | Usage |
|-------|--------|-----------|-------|
| **Polar Night** (dark backgrounds) | nord0–nord3 | `#2e3440` `#3b4252` `#434c5e` `#4c566a` | Base backgrounds, elevated surfaces, comments, UI chrome |
| **Snow Storm** (light text) | nord4–nord6 | `#d8dee9` `#e5e9f0` `#eceff4` | Text, light backgrounds, highlights |
| **Frost** (accent blues) | nord7–nord10 | `#8fbcbb` `#88c0d0` `#81a1c1` `#5e81ac` | Classes/types, declarations, primitives, keywords |
| **Aurora** (semantic) | nord11–nord15 | `#bf616a` `#d08770` `#ebcb8b` `#a3be8c` `#b48ead` | Error, warning, success(ish), strings, numbers |

**How to use in DESIGN.md:** Reference Nord as a ready-made dark theme palette. Map `nord0` → `--color-background`, `nord4` → `--color-foreground`, `nord8` → `--color-primary`, `nord11` → `--color-destructive`, etc.

Available as npm package: `npm install nord` — provides CSS, SCSS, Less, and Stylus variables.

---

### Catppuccin — Pastel Theme System
**Repo:** [catppuccin/catppuccin](https://github.com/catppuccin/catppuccin) | 18k+ stars
**Use for:** Warm, pastel color system with 4 flavors from light to dark. 26 named colors per flavor.

**Four flavors (light → dark):**

| Flavor | Base background | Character |
|--------|----------------|-----------|
| **Latte** | Light cream | Light mode — warm and bright |
| **Frappe** | Medium-dark blue | Soft dark — gentle contrast |
| **Macchiato** | Dark blue-gray | Standard dark — balanced |
| **Mocha** | Deep dark | Full dark — highest contrast |

**26 named colors per flavor:** Rosewater, Flamingo, Pink, Mauve, Red, Maroon, Peach, Yellow, Green, Teal, Sky, Sapphire, Blue, Lavender, Text, Subtext1, Subtext0, Overlay2, Overlay1, Overlay0, Surface2, Surface1, Surface0, Base, Mantle, Crust.

**How to use in DESIGN.md:** Catppuccin's 4-flavor system is the gold standard for multi-theme support. Map semantic roles to named colors (e.g., `Red` → destructive, `Blue` → primary, `Green` → success). Style guide at [docs/style-guide.md](https://github.com/catppuccin/catppuccin/blob/main/docs/style-guide.md).

**Ports available for everything:** [github.com/catppuccin](https://github.com/catppuccin) — Tailwind, CSS, VS Code, and 200+ apps.

---

## Animations & Creative Components

### React Bits — 110+ Animated React Components
**Repo:** [DavidHDev/react-bits](https://github.com/DavidHDev/react-bits) | 9k+ stars
**Docs:** [reactbits.dev](https://reactbits.dev/)
**Use for:** Ready-made animated components for text, backgrounds, cursors, and UI effects. Ships in 4 variants: JS-CSS, JS-Tailwind, TS-CSS, TS-Tailwind.

**Component categories:**

| Category | Count | Examples |
|----------|-------|---------|
| **Text Animations** | 23 | BlurText, GlitchText, ShinyText, SplitText, ScrollFloat, DecryptedText, GradientText, RotatingText, ScrambledText, FuzzyText, ASCIIText, CircularText, CountUp |
| **Animations** | 29 | AnimatedContent, BlobCursor, ClickSpark, GlareHover, ImageTrail, Magnet, MetaBalls, MetallicPaint, PixelTrail, Ribbons, SplashCursor, StarBorder, StickerPeel |
| **Backgrounds** | 40 | Aurora, Galaxy, Particles, Waves, LiquidChrome, Iridescence, Hyperspeed, Plasma, Silk, GridDistortion, Lightning, Beams, Orb, Prism, LightRays, Threads |
| **Components** | 18+ | Various UI components |

**How to use in DESIGN.md:** Reference React Bits for Section 7 (Motion & Animation). Install via shadcn CLI:
```bash
npx shadcn@latest add @react-bits/BlurText-TS-TW
```

**Creative tools included:**
- **Background Studio** — Explore animated backgrounds, export as video/image/code
- **Shape Magic** — Inner rounded corners, export as SVG/React/clip-path
- **Texture Lab** — Apply noise, dithering, ASCII effects to images/videos

---

### React Three Fiber — 3D for React
**Repo:** [pmndrs/react-three-fiber](https://github.com/pmndrs/react-three-fiber) | 28k+ stars
**Use for:** 3D scenes, WebGL effects, immersive experiences in React apps. Renders outside React for zero overhead.

| What to reference | Where |
|-------------------|-------|
| Documentation | [r3f.docs.pmnd.rs](https://r3f.docs.pmnd.rs/) |
| Examples | [docs.pmnd.rs/react-three-fiber](https://docs.pmnd.rs/react-three-fiber) |
| Ecosystem | drei (helpers), postprocessing, rapier (physics) |

**How to use in DESIGN.md:** Reference R3F in Section 7 (Motion & Animation) for projects needing:
- 3D product viewers / configurators
- Immersive landing pages and hero sections
- WebGL shader backgrounds (combine with React Bits backgrounds)
- Interactive data visualizations in 3D space

**Key patterns:**
```tsx
import { Canvas } from "@react-three/fiber";
import { OrbitControls, Environment } from "@react-three/drei";

<Canvas>
  <ambientLight />
  <mesh><boxGeometry /><meshStandardMaterial /></mesh>
  <OrbitControls />
  <Environment preset="city" />
</Canvas>
```

**Performance note:** R3F renders on a separate thread. Include in DESIGN.md performance budget as a special case — 3D scenes have their own optimization rules (LOD, instancing, texture compression).

---

## Frameworks

### Remix — Full-Stack React Framework
**Repo:** [remix-run/remix](https://github.com/remix-run/remix) | 31k+ stars
**Use for:** Full-stack React apps with progressive enhancement, nested routing, and web-standard data loading.

| What to reference | Where |
|-------------------|-------|
| Architecture decisions | [decisions/](https://github.com/remix-run/remix/tree/main/decisions) |
| Agent instructions | [AGENTS.md](https://github.com/remix-run/remix/blob/main/AGENTS.md) |
| Documentation | [docs/](https://github.com/remix-run/remix/tree/main/docs) |
| Skills | [skills/](https://github.com/remix-run/remix/tree/main/skills) |

**When to choose over Next.js:** Progressive enhancement priority, form-heavy apps, nested layouts with independent data loading, apps that should work without JavaScript.

**Design implications for DESIGN.md:**
- Forms use native `<form>` with `action` — design for progressive enhancement
- Nested routes → design independent loading states per layout section
- Error boundaries per route → design error UI at each nesting level
- No client-side state needed for server data — simpler reactivity model

---

## Curated Resource Collections

### Awesome Design — Curated Design Resources
**Repo:** [gztchan/awesome-design](https://github.com/gztchan/awesome-design) | 16k+ stars
**Use for:** Discovery and inspiration across all design disciplines.

**Resource categories:**

| Category | What you'll find |
|----------|-----------------|
| **Stock** | Unsplash, Pexels, Pixabay, 500px, Gratisography — free high-res images |
| **Icon & Logo** | Icon libraries, logo generators, SVG resources |
| **Color** | Color palette generators, contrast checkers, gradient tools |
| **Typography** | Font discovery, pairing tools, type specimen galleries |
| **Toolkit** | Design tools, plugins, browser extensions |
| **Prototyping** | Figma, Sketch, InVision, prototyping tools |
| **Mockup** | Device mockups, screenshot tools |
| **User Testing** | Usability testing platforms and resources |
| **Styleguide & Branding** | Brand guidelines, design system examples |
| **Tutorial** | Design courses, articles, learning resources |

**How to use in DESIGN.md:** Reference for Section 1 (Visual Theme) when the user needs inspiration. Point to specific categories for stock imagery, icon selection, or typography pairing.

---

## Quick Selection Guide

| Need | Best resource |
|------|--------------|
| **Default component library** | [shadcn/ui](https://github.com/shadcn-ui/ui) |
| **Enterprise / Material Design** | [MUI](https://github.com/mui/material-ui) or [Material Web](https://github.com/material-components/material-web) |
| **Rapid prototyping with themes** | [DaisyUI](https://github.com/saadeghi/daisyui) |
| **Accessibility-first components** | [Chakra UI](https://github.com/chakra-ui/chakra-ui) |
| **Dark theme palette** | [Nord](https://github.com/nordtheme/nord) (arctic) or [Catppuccin](https://github.com/catppuccin/catppuccin) (pastel) |
| **Multi-theme (light+dark+variants)** | [Catppuccin](https://github.com/catppuccin/catppuccin) — 4 flavors, 26 colors each |
| **Text/UI animations** | [React Bits](https://github.com/DavidHDev/react-bits) — 110+ animated components |
| **3D / WebGL scenes** | [React Three Fiber](https://github.com/pmndrs/react-three-fiber) |
| **Full-stack progressive enhancement** | [Remix](https://github.com/remix-run/remix) |
| **Design inspiration & tools** | [Awesome Design](https://github.com/gztchan/awesome-design) |
| **Professional CSS architecture** | [Primer CSS](https://github.com/primer/css) (GitHub's system) |
| **Design token architecture** | [Material Web tokens/](https://github.com/material-components/material-web/tree/main/tokens) |
