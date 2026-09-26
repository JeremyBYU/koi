# How do I feed the fish?

## Recommendation

- Feed with a left click. Turn on mouse tracking with SGR encoding
  (`?1000h ?1003h ?1006h`); in Rust, crossterm's `EnableMouseCapture` does
  exactly this. It works in Ghostty directly and inside tmux when tmux has
  `set -g mouse on`.
- Treat click positions as whole cells and drop the pellet at the center of
  the clicked cell, with a little random scatter. A koi pellet does not need
  sub-cell aim, and cell precision is what tmux gives you anyway.
- Add pixel-precise clicks (mode 1016) only as optional polish when running
  outside tmux. tmux 3.4 (the version on this machine) does not support it, so
  it cannot be the main path.
- Keyboard: a soft on-screen reticle moved with arrows or `hjkl`, `space` to
  drop food there, and `f` to toss food somewhere near the fish. This also
  covers tmux with `mouse off`.
- muster tie-in: poll muster's `GET /api/herd` every few seconds and let the
  pond quietly reflect the agent herd. One koi per agent, and a waiting agent's
  koi drifts to the surface. Details at the end.

## Terminal mouse modes

Mouse reporting is turned on with DECSET private modes:

| Mode | What it does |
|---|---|
| `?1000h` | report button press and release |
| `?1002h` | also report motion while a button is held (drag) |
| `?1003h` | report all motion, button or not (hover) |
| `?1006h` | SGR encoding: `CSI < Cb ; Cx ; Cy M` on press, `...m` on release. Decimal, 1-based, no 223-column limit |
| `?1016h` | SGR-Pixels: same format as 1006, but Cx/Cy are pixels, and motion reports fire per pixel instead of per cell |

1016 needs 1006 and one of 1000/1002/1003 to be on as well
([vtdn: SGR-Pixel](https://vtdn.dev/docs/decset/mode1016-sgr-pixel/),
[vtdn: SGR mouse](https://vtdn.dev/docs/decset/mode1006-sgr-mouse/)).
Always turn every mode back off on exit, including on panic, or the user's
shell gets mouse garbage.

Support for 1016: Ghostty (since 1.0), kitty, WezTerm, foot, xterm, iTerm2,
Konsole. Not supported: tmux, GNU Screen, Zellij, Alacritty
([vtdn](https://vtdn.dev/docs/decset/mode1016-sgr-pixel/)). Ghostty and kitty
report device pixels
([SwiftTerm #686](https://github.com/migueldeicaza/SwiftTerm/issues/686)).
In pixel mode, coordinates can go negative when the pointer leaves the window
([Ghostty discussion #9647](https://github.com/ghostty-org/ghostty/discussions/9647)),
so clamp them.

### Rust specifics

crossterm's `EnableMouseCapture` writes `?1000h ?1002h ?1003h ?1015h ?1006h`
and parses SGR reports into `MouseEvent { column, row, kind, modifiers }`
([crossterm event.rs](https://github.com/crossterm-rs/crossterm/blob/master/src/event.rs)).
It does not know about 1016. Since 1016 reports use the same byte format as
1006, writing `\x1b[?1016h` yourself after `EnableMouseCapture` makes crossterm
hand you pixel values in `column`/`row` (minus one, since it converts from
1-based). That works, but it is a quiet reinterpretation of a field, so gate it
behind a flag that records whether 1016 is actually on. Confirm it with DECRQM
(`CSI ? 1016 $ p`; the reply `CSI ? 1016 ; 1 $ y` means set, `; 0` means
unsupported). tmux will answer "not recognized".

## Mapping a click to pond coordinates

Keep the simulation in its own continuous world units (floats), independent of
the renderer. Every input path converts to world coordinates once.

**Half-block renderer.** Each cell holds two vertical pond pixels (`▀` with
foreground = top pixel, background = bottom pixel). A cell click at
1-based `(col, row)` maps to pond pixel `x = col - 1 + 0.5`,
`y = (row - 1) * 2 + 1.0`, the center of the cell. The click cannot tell the
top half from the bottom half. Add a random offset of up to about half a cell
so repeated clicks do not stack pellets on a perfect grid; it also reads as a
natural toss.

If 1016 is on, use `x = px / cell_w`, `y = px_y / cell_h * 2` instead, which
gives the half-block resolution exactly.

**Pixel graphics renderer (Kitty graphics protocol).** The pond is an image of
`W x H` pixels scaled over the pond's cell rectangle. From a cell click, map
the cell center: `x = (col - 1 + 0.5) * cell_w * (W / pond_px_w)`, and the
same for y. From a 1016 click, use the pixel directly, minus the pond's
top-left offset in pixels. You need the cell size in pixels: send `CSI 16 t`
(reply `CSI 6 ; h ; w t`) or divide `TIOCGWINSZ`'s `ws_xpixel`/`ws_ypixel` by
the column and row counts
([kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/)).
Re-query on every resize, and fall back to assuming a 1:2 cell aspect if both
return zero.

**Is cell precision enough?** Yes for feeding. A Ghostty cell is roughly 8 to
10 px wide and 18 to 20 px tall at common font sizes. Pellets spread on the
water and koi swim to them over a body length or two, so a one-cell error is
invisible. The only place pixel precision would show is a hover cursor drawn
as a small shadow on the water, where motion snapping to cells looks steppy.
Solve that by easing the drawn cursor toward the last reported position
instead of depending on 1016.

## What feeding should feel like

- Click: one pellet (or a small pinch of 3 to 5) lands with a ripple impulse.
  Koi within sensing range turn toward it; nearer fish arrive first.
- Click and drag (mode 1002): a line of scattered pellets along the path, rate
  limited so it cannot flood the pond.
- Hover (mode 1003, optional): a faint shadow or hand reflection on the water.
  Koi could drift toward it hoping for food, which is what real koi do. Cheap to
  add, and it works under tmux.
- Right click or scroll: leave unbound for now. Scroll inside tmux with
  `mouse on` goes to the app while mouse mode is active, which is fine.
- Cap total pellets (for example 40) and let uneaten ones sink and fade, so the
  pond never gets busy.

## Keyboard alternatives

| Key | Action |
|---|---|
| arrows / `hjkl` | move a reticle (a faint ring on the water), with smooth easing |
| `space` / `enter` | drop food at the reticle |
| `f` | toss food at a random spot near the koi |
| `q` / `esc` | quit |

Hide the reticle after a few seconds of no keyboard input, so it does not
spoil the view. The reticle is the full fallback: it works with no mouse, with
tmux `mouse off`, and over SSH.

## Behavior under tmux

- tmux only forwards mouse events to a pane when the outer tmux has
  `set -g mouse on`. With it off, tmux never asks Ghostty for mouse reports and
  the app gets nothing
  ([tmux FAQ](https://github.com/tmux/tmux/wiki/FAQ)). Detect this: if no mouse
  event arrives within the session, the keyboard path still works. Show a one-line
  hint on first start (`mouse off in tmux? try: set -g mouse on`) when `$TMUX`
  is set.
- With `mouse on`, tmux translates events to pane-relative cells and re-encodes
  them in the format the pane requested (1006 included). Coordinates are
  already relative to the pane, so no offset math is needed.
- tmux has no 1016 support at all, including tmux 3.4 on this machine
  ([vtdn](https://vtdn.dev/docs/decset/mode1016-sgr-pixel/)). Passthrough does
  not help, because mouse reports travel from the terminal to the app and tmux
  rewrites them. Under tmux the game is always cell precision.
- Holding Shift while clicking bypasses app mouse capture in most terminals,
  so the user can still select text.
- Graphics (relevant only if the pixel renderer is chosen): Kitty graphics
  need `set -g allow-passthrough on`, DCS-wrapped sequences, and Unicode
  placeholders (`U+10EEEE`, `U=1`) so tmux can place and clip the image as
  text. Ghostty supports placeholders
  ([Mitchell Hashimoto](https://x.com/mitchellh/status/1818696111999299976),
  [kitty protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/),
  [Ghostty discussion #7224](https://github.com/ghostty-org/ghostty/discussions/7224)).
  Click mapping does not change: the image still occupies a known cell
  rectangle.
- Cell pixel size under tmux: `CSI 16 t` and `TIOCGWINSZ` pixel fields may be
  answered by tmux, not Ghostty. Verify on tmux 3.4 in the prototype; the 1:2
  fallback covers a zero answer.

## Tie-in with muster

muster is a daemon on this workstation that watches tmux panes and Claude Code
transcripts and serves a phone dashboard of the "herd": repos, worktrees, and
agents, each with a status of `waiting`, `working`, `done`, `idle`, or
`dormant`. It listens on port 8630 by default and exposes
`GET /api/herd`, which returns every agent with `status`, `status_since`,
`name`, and `wait` (what it is waiting on). It is read-only for our purposes;
the pond would only poll it.

Light ideas, from least to most involved:

1. **One koi per agent.** Poll `/api/herd` every 5 to 10 seconds. Each live
   agent owns a koi, colored by repo. `working` koi swim normally, `idle` and
   `done` koi rest near the bottom, and a `waiting` koi slowly rises and circles
   near the surface. Glancing at the pond tells you someone needs you, with no
   alert sound or badge. Agents that go dormant swim off the edge. If muster is
   not running, the pond just has its own fish.
2. **A finished turn feeds the pond.** When an agent goes `working` to `done`,
   a pellet drops at a random spot and the fish gather. Ambient,
   non-demanding progress feedback: ripples mean work finished somewhere.
3. **Click a surfacing koi to go to it.** Clicking a `waiting` koi shows its
   agent name and `wait` text as a faint caption, and a second click (or
   `enter`) runs `tmux switch-client -t <pane_id>` to jump to that pane. This is
   the only idea that acts on anything, so keep it opt-in.

Idea 1 alone is probably the right first tie-in: it adds no controls and uses
one endpoint.
