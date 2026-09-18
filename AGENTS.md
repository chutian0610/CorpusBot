# AGENTS.md

## UI Design Rules

CorpusBot is a desktop-first product. Interfaces must feel like application
panels, not long web pages.

### Layout

- Treat the app viewport as the design canvas. Use a full-height app shell
  (`h-dvh`, `overflow-hidden`) and keep the window title/status area stable.
- Do not put scrolling on the page root. Only a clearly bounded data list,
  editor body, or form region may scroll.
- Every flexible flex/grid child must define or inherit `min-h-0` so nested
  scroll areas can work predictably.
- Keep primary actions in the same visual region as the data they affect. Do
  not require scrolling to reach the main action.
- Design for resizable desktop windows first. Do not tune only for a fixed
  1280x800 frame.

### Information Design

- Show one clear hierarchy per screen: context, primary data, then secondary
  actions.
- Do not duplicate the same entity in a highlighted summary and a detail list.
  If the last item is promoted, exclude it from the historical list.
- Use stable placement for repeated controls and keep list rows uniform.
- Prefer progressive disclosure for advanced settings.

### States and Accessibility

- Empty, loading, error, and success states must be visible without taking over
  the whole app shell.
- Use semantic headings, labels, buttons, and `role="alert"` for blocking
  errors.
- Preserve focus order and keyboard access when data updates.
- Truncate long paths in list rows, but expose the full path with `title`.

### Validation

- Before delivering a UI change, inspect it in a browser at desktop size.
- Confirm there is no root scrolling, no overlapping controls, and that only
  the intended content region scrolls.
