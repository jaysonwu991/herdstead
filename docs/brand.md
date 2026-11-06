# Brand

## The name

**herdstead** — *herd* + *stead*: a homestead for a herd. A place that stays put
while the work moves; a runtime your agents come home to. Say it **HERD-sted**.

Tagline: *a homestead for your herd.*

The name is a coinage, not a dictionary word. "Stead" is the old word for a place
or farmstead (`homestead`, `instead`, `steadfast`), which is exactly the promise:
panes that persist, sessions you reattach to.

## The mark

A stead whose window is a pane grid, with one pane lit for the agent that has
focus. The roof is the hearth; the grid is the workspace; the lit cell is the
pane you are looking at.

```
  ▲            roof, hearth amber
 ┌─────┬───┐
 │     │   │   window mullions, mist on ink
 ├─────┼───┤
 │     │ █ │   one pane lit
 └─────┴───┘
```

It is built from six shapes on a 96-unit grid, which is why it still reads at
16 px.

## Assets

| file | intrinsic size | use |
|---|---|---|
| `assets/logo.svg` | 400 × 96 | horizontal lockup for dark backgrounds |
| `assets/logo-light.svg` | 400 × 96 | horizontal lockup for light backgrounds |
| `assets/logo-mark.svg` | 96 × 96 | square mark: icon, avatar, README header |
| `assets/logo-mark-mono.svg` | 96 × 96 | one-ink mark for print and single-colour use |
| `assets/social-preview.png` | 1280 × 640 | GitHub social preview (upload it in the repository settings) |

The wordmark and tagline are set in the system sans (`ui-sans-serif`,
`-apple-system`, `Segoe UI`, `Roboto`), so the lockups render with the local
interface font. The square mark has no text and is identical everywhere.

The README uses `logo-mark.svg` rather than a lockup because the mark carries its
own background and therefore reads on both a light and a dark page.

## Palette

| name | hex | where |
|---|---|---|
| ink | `#10161F` | the mark's badge, mullions, single-ink artwork |
| mist | `#D7DCE5` | the window, wordmark on dark |
| hearth | `#F0A64A` | the roof, the lit pane, the tagline on dark |
| ember | `#A6620F` | the tagline on light backgrounds, for contrast |

The interface palettes are separate and richer ([themes](themes.md)); these four
are the brand's, and they should not be recoloured per theme.

## Using it

- **Minimum sizes:** 16 px for the square mark, 160 px wide for a lockup. Below
  that, use the mark alone.
- **Clear space:** keep at least the badge's corner radius (one fifth of the mark's
  width) clear on every side.
- **Do:** keep the mark square, keep the lit pane in the bottom-right, keep the
  roof amber.
- **Do not:** stretch, rotate, outline, add a second accent, or put the dark
  lockup on a light background (there is a light one). Do not re-letter the
  wordmark in another face; use the mark if you need a different type.
- The mark may be used to refer to herdstead in articles, package listings and
  integrations. Do not use it as the icon of an unrelated project.

## Regenerating previews

The SVGs are the source of truth. To make PNGs on macOS:

```bash
qlmanage -t -s 512 -o /tmp assets/logo-mark.svg      # icon preview
qlmanage -t -s 800 -o /tmp assets/logo.svg           # lockup preview
```

`assets/social-preview.png` is ready for GitHub's social preview (1280 × 640).
GitHub has no API for that image; set it in **Settings → General → Social
preview**. To rebuild it, render a 1280 × 1280 square with the design centred in
the middle 640 rows and crop the middle band with `sips -c 640 1280` —
`qlmanage` always pads its thumbnail to a square, so an exact-size band is the
reliable way to get 16:9 out of it.
