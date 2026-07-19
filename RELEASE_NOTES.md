## Horizontal bar mode (0.7.0)

- New `horizontal = true` setting renders a single-row bar at the top of the screen, dmenu-style. Entries flow left-to-right, packed into greedy pages with `<` / `>` markers when they overflow. Navigation is via Left/Right arrows (edge-triggered: they move the selection once the caret can't travel further)
- Centering is now a separate `center` option; setting `width` no longer forces centering. `center = false` pins the window to the monitor top-left
- Backward compatible — `horizontal = false` (the default) keeps the classic vertical layout unchanged
