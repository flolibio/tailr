# Frontend Design Guidelines

Rules for all changes under `frontend/` (Vue components and `style.css`). These exist because
the same two bug classes keep coming back:

1. **Page width blown out** — an element stretches the whole app wider than the viewport,
   the sidebar scrolls off-screen, nothing is where it should be.
2. **Misaligned / not centered** — rows, icons, or blocks that look right in one context
   and drift in another.

Both are structural, not cosmetic: they come from letting *content size the layout* instead
of *layout constraining the content*. Fix the structure and the symptom disappears for good.

---

## 1. Layout: the layout owns the size, not the content

### 1.1 The root cause of every "page too wide" bug

Flex and grid items default to `min-width: auto`: an item **will not shrink below its
content's intrinsic width**. So a row of non-wrapping tabs, a long file path, a wide table,
or an unbreakable token does this, step by step:

```
long child (white-space: nowrap, no shrink)
  → inflates its flex/grid parent (min-width: auto)
  → inflates the next ancestor
  → inflates the grid track of .app-shell
  → the whole app becomes wider than the viewport
```

The app shell is already `height: 100vh; overflow: hidden` with `html, body, #app { overflow: hidden }`
(`style.css`). When this bug appears anyway, somewhere in the chain an item is missing
`min-width: 0`. That is the whole bug, every time.

Historical incidents (all same root cause):

| Commit | Symptom |
|---|---|
| `34772a6` | Many tabs stretched `.globalbar` past its `1fr` column, page wider than viewport |
| `2f30739` (v1.0.6) | Tab bar overflow fix, round two |
| `5a35822` / `1e967ea` | FilterBar chips overflow — fixed by chip folding + `min-width: 0` |
| `dec2df7` | Filterbar overflow + suggestions dropdown width |
| `a86317b` | MCP page config-path row not centered / misaligned |

### 1.2 Hard rules

1. **Every flex/grid item that contains text or arbitrary-width content gets `min-width: 0`**
   (and `min-height: 0` for the vertical axis). Add it at the point of containment — where
   you want the overflow to *stop*. The comment already in `style.css` on `.globalbar` and
   `.filterbar` documents why; keep following it.

2. **Ellipsize long text at the text element, not the container.** The canonical pattern
   used by `.sidebar-item .item-label` and `.tree-item .item-label`:

   ```css
   .item          { display: flex; align-items: center; } /* flex item: min-width: 0 */
   .item .label   { flex: 1; min-width: 0;
                    white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
   ```

   The `min-width: 0` goes on the flex/grid item; the ellipsis trio goes on the inner text
   child. Setting ellipsis on the flex item itself does nothing.

3. **Overflow lives in a dedicated scroll container at the right level.** Log lines scroll
   horizontally inside the log table (`LogViewer.vue` `.log-viewer`, `overflow-x: auto`),
   tabs scroll inside `TabBar.vue`'s own scroll strip, chips fold inside `FilterBar`. **The
   page itself never scrolls horizontally.** Pick the container where scrolling makes sense
   to the user, give *it* `overflow-x: auto`, and give every ancestor up to the shell
   `min-width: 0` so the overflow actually reaches that container.

4. **No fixed pixel widths on containers of variable content.** Use `max-width` (or nothing)
   so the container can shrink. Fixed widths are only for chrome (icons, gutters, bars) —
   and even those get `flex-shrink: 0` explicitly.

5. **Never `width: 100vw`** — it includes the scrollbar width and itself causes overflow.
   Use `100%` of the parent.

6. **Check the whole chain.** `min-width: 0` on one item doesn't help if an intermediate
   wrapper still refuses to shrink. Walk from the wide element up to the scroll container
   and make sure every flex/grid step allows shrinking.

### 1.3 Known-good patterns in this codebase

Copy these, don't reinvent them:

- **Bar with non-shrinkable children** — `.globalbar` / `.filterbar` in `style.css`:
  `display: flex; align-items: center; min-width: 0;` + inner scroll/fold container.
- **Scrollable tab strip with arrows** — `TabBar.vue`: overflow container + arrow buttons,
  active tab auto-scrolled into view.
- **Ellipsized row label** — `.sidebar-item .item-label`, `.tree-item .item-label`,
  LogViewer row cells (`.log-row` children, `min-width: 0` + ellipsis).
- **Chip folding** — `FilterBar.vue`: chips collapse into an overflow fold instead of
  stretching the bar.

---

## 2. Centering and alignment

1. **Center content inside a bar/row** — the parent is a flex container:
   `display: flex; align-items: center; justify-content: center;`.
   Reference: the config-path row in `settings/McpSection.vue`.

2. **Center a block in its parent (both axes)** — `display: grid; place-items: center;`
   or flex with `justify-content` + `align-items`. Don't fake it with margins.

3. **Vertical centering of icons inside bars** — *always* via the parent's
   `align-items: center`, never via `margin-top` / `position: relative; top: Npx` on the
   icon. Margin-tuned centering breaks when font size, line-height, or bar height changes —
   this caused the tab-arrow alignment fixes (`9f99d07`, `112b0a5`).

4. **`text-align: center` centers text**, not flex children. If it "doesn't work", the
   element you're centering is a flex/grid child — center it with the container.

5. **Vertically stacked icon + text** — wrap in a flex column and let the flexbox do the
   spacing (`gap`), don't pad individual elements.

---

## 3. Color system

The full interactive design map is **`docs/design/log-row-color-system.html`** (v0.7.0
prototype, kept as the source of truth for the log-row color system — open it in a browser
to see light/dark demos of every rule below). The implemented tokens live in `style.css`.

### 3.1 Tokens only — zero hardcoded colors

All colors go through CSS variables defined in `style.css`. Never introduce a raw hex or
`rgba()` in a component. If no token fits, add one to `style.css` (both themes) and use it.

Core tokens:

| Token | Purpose |
|---|---|
| `--bg`, `--bg-2`, `--bg-3` | Surface layers: page → recessed → deepest |
| `--text`, `--text-2`, `--text-3` | Primary / secondary / muted text |
| `--border`, `--border-2` | Hairline / strong border |
| `--accent`, `--accent-hover`, `--accent-light` | Interactive accent |
| `--c-{alert,error,warn,info,debug,trace}-{bg,text,border}` | Log level semantic colors |
| `--kw-1` … `--kw-5` | Keyword highlight hues (HSL channels, shared by chips & `<mark>`) |
| `--shadow-*`, `--focus-ring` | Elevation and keyboard focus |

### 3.2 Level colors are the single semantic source

The `--c-*-text` value of each level is the **root color**. Every row state — hover,
marked (click-selected), bookmarked, jump-highlight — derives from the level's root color,
so an ERROR row reads as red in *every* state. Derive with `color-mix`, never introduce a
second hue:

```css
/* state = same hue, different alpha */
background: color-mix(in srgb, var(--c-error-text) 10%, var(--bg));
```

### 3.3 Alpha ladder for states

States differ by **transparency, not hue**; more user intent = stronger background:

| State | Alpha (over `--bg`) | Extra identity cue |
|---|---|---|
| default | 0% | — |
| bookmarked | 8% | ★ icon (gold `--c-star`) |
| hover | 10% | — |
| marked (selected) | 14% | 3px left color bar |
| jump highlight | 14% + fade pulse | — |
| copy feedback | 10% of `--accent` | ✓ icon turns accent |

Identity is carried by **icons and bars, not by near-indistinguishable hues** — two similar
cyans can't tell "my bookmark" from "my selection" apart; a ★ and a left bar can. Stacked
states keep hover feedback: `.is-bookmarked:hover` deepens to 16%.

### 3.4 Keyword highlights share one hue system

FilterBar chips and in-log `<mark class="kw-mark">` use the same `--kw-1…5` hues so
"the word I searched" is the same color in the chip and in the log. Hues are fixed
(43° amber / 200° sky / 280° violet / 150° emerald / 20° tangerine), the 6th keyword wraps
around. Alpha rules: chip background 14–16%, chip border 30%, `<mark>` background 28–30%,
`<mark>` text at full hue.

### 3.5 Dark theme

Tokens flip at `:root.dark` in `style.css` — components should **never** contain
`body.dark` / `:root.dark` overrides. Dark theme adjusts lightness/saturation of the same
hues (brighter, more saturated for dark backgrounds), it doesn't change hues. When adding
a token, define it in **all three blocks** of `style.css` (`:root`, `:root.dark`, and the
explicit `:root[data-theme="light"]` parity block) and verify readability in both themes.

### 3.6 App frame layering

Fixed background hierarchy, deepest to brightest: **sidebar** `--bg-2` (recessed) →
**globalbar** `color-mix(text 3%, bg)` (subtle recess to seat the active tab) →
**content / statusbar** `--bg` (brightest, the active tab "grows" into it, Chrome-style).
Separation comes from this bg contrast + borders, not from drop shadows everywhere.

---

## 4. Shared primitives

- **Spacing**: `--space-1..5` (4/8/12/16/24). No ad-hoc paddings from thin air; pick the
  nearest scale step.
- **Radius**: `--radius-sm` (6), `--radius` (8), `--radius-lg` (12); pills use `9999px`.
- **Type**: `--font-sans` (UI), `--font-mono` (log content, paths, tokens, endpoints,
  config snippets — anything a user might copy). Font weights via `--fw-*`.
- **Focus**: keyboard focus is `:focus-visible` + `--focus-ring` (global in `style.css`).
  Custom controls must stay focusable (real `<button>`/`<a>` or `tabindex="0"` + key handler).
- **Icons**: `lucide-vue-next`, consistent stroke; `flex-shrink: 0` so they never squash.
- **Motion**: transitions ~.12s; `prefers-reduced-motion` is already handled globally —
  don't opt out per-component.

---

## 5. Pre-merge checklist

Run through this for any UI change; it takes two minutes and catches every bug class above:

- [ ] **Squeeze test**: narrow the window, open many tabs, use long file paths and long
      log lines — no horizontal page scroll, nothing clipped, sidebars intact.
- [ ] **Overflow owner**: any new variable-width content has a designated scroll/fold/
      ellipsis container, and `min-width: 0` on the chain up to it.
- [ ] **Centering**: rows/icons aligned via flex on the parent, not tuned margins.
- [ ] **Both themes**: check light *and* dark — contrast, borders, hover states.
- [ ] **No new hardcoded colors** — grep your diff for `#[0-9a-fA-F]{3,8}` and `rgba(`;
      everything should be a token.
- [ ] **Tokens in all three blocks** if you added one (`:root`, `:root.dark`,
      `:root[data-theme="light"]`).
