Load when a web or mobile frontend feature needs state, data fetching, forms, styling, components or routing. [Libraries](libraries.md) holds the rule for adding a library and the version check to run on the chosen set.

# Frontend libraries

## State, data fetching and forms

| Need | Library |
|---|---|
| Server data: fetching, caching, invalidation | TanStack Query 5 (SWR for simple cases). Never mirror remote data into a global store |
| Small global UI state (React, React Native) | Zustand 5; Jotai for many independent atoms; Redux Toolkit 2 for large teams or existing Redux |
| Vue store | Pinia |
| Explicit state machines | XState 5 |
| Shared state across islands or frameworks | nanostores |
| Forms | React Hook Form; TanStack Form for framework-agnostic typed forms |

Start with component and URL state; add a store only when distant components share state.

## UI and mobile

Styling: Tailwind CSS 4; components: shadcn (copies source you own) on Radix primitives. SPA routing: TanStack Router or React Router. React Native: Expo Router for navigation, NativeWind for Tailwind classes, FlashList for long lists.
