# Component Checklist

Inventory of components every well-designed app should consider. Check existing components against this list during the ANALYZE step.

## Core Components

| Category | Component | Purpose | Priority |
|----------|-----------|---------|----------|
| **Actions** | Button | Primary interaction element | Required |
| | IconButton | Action with icon only (needs `aria-label`) | Required |
| | Link | Navigation action | Required |
| **Forms** | Input | Text entry | Required |
| | Textarea | Multi-line text | Required |
| | Select | Single choice from list | Required |
| | Combobox | Searchable select | High |
| | Checkbox | Boolean toggle | Required |
| | RadioGroup | Exclusive choice | Required |
| | Switch | On/off toggle | Required |
| | Slider | Range value | Medium |
| | DatePicker | Date selection | Medium |
| | FileUpload | File input | Medium |
| | InputOTP | Code entry | Low |
| **Form Layout** | Form | Form wrapper with validation | Required |
| | Field | Label + input + description + error | Required |
| | FieldGroup | Groups of fields | Required |
| | FieldSet | Related field grouping | High |
| **Data Display** | Table | Tabular data | High |
| | Card | Content container | Required |
| | Badge | Status indicator | Required |
| | Avatar | User/entity image | High |
| | Chart | Data visualization | Medium |
| | List | Ordered/unordered items | Required |
| **Navigation** | Navbar | Primary navigation | Required |
| | Sidebar | Secondary navigation | Medium |
| | Breadcrumb | Location hierarchy | High |
| | Tabs | Content sections | Required |
| | Pagination | Page navigation | High |
| | NavigationMenu | Dropdown navigation | Medium |
| **Overlays** | Dialog | Modal interaction | Required |
| | Sheet | Side panel | High |
| | Drawer | Bottom panel (mobile) | High |
| | AlertDialog | Confirmation modal | Required |
| | Popover | Contextual popup | High |
| | Tooltip | Hover information | Required |
| | HoverCard | Rich hover preview | Medium |
| | DropdownMenu | Action menu | Required |
| | ContextMenu | Right-click menu | Low |
| **Feedback** | Toast (Sonner) | Transient notifications | Required |
| | Alert | Persistent messages | Required |
| | Progress | Loading progress | High |
| | Skeleton | Loading placeholder | Required |
| | Spinner | Inline loading | Required |
| | Empty | Empty state | Required |
| | ErrorBoundary | Error recovery | Required |
| **Layout** | Separator | Visual divider | Required |
| | ScrollArea | Custom scrollbar | Medium |
| | Resizable | Adjustable panels | Low |
| | Accordion | Collapsible sections | High |
| | Collapsible | Single expand/collapse | High |
| | AspectRatio | Constrained proportions | Medium |

## Component Quality Checklist

Every component should meet these standards:

### Functionality
- [ ] Works with keyboard (Tab, Enter, Space, Escape, Arrow keys as needed)
- [ ] Supports controlled and uncontrolled modes
- [ ] Handles edge cases (empty, overflow, error states)
- [ ] Responds to `disabled` state correctly

### Accessibility
- [ ] Correct ARIA role and attributes
- [ ] Visible focus indicator
- [ ] Screen reader announces state changes
- [ ] Color contrast meets WCAG AA
- [ ] Touch target ≥ 44x44px

### Styling
- [ ] Uses semantic design tokens (not raw colors)
- [ ] Supports dark mode via token architecture
- [ ] Responsive across breakpoints
- [ ] Variants cover common use cases (size, color, state)

### Composition
- [ ] Composable parts (e.g., `CardHeader`, `CardContent`, `CardFooter`)
- [ ] Supports `asChild` / `render` for custom triggers
- [ ] Accepts `className` for layout customization
- [ ] Uses `cn()` for conditional class merging

## References

- [shadcn/ui Components](https://ui.shadcn.com/docs/components)
- [Radix Primitives](https://www.radix-ui.com/primitives)
- [Ant Design Components](https://ant.design/components/overview)
