# Muesli brand guide (for coding agents)

Muesli is an open-source meeting notes app, a clone of Granola.
**Slogan:** *Granola's raw, unbaked cousin*

Follow this file whenever you touch the app icon, the menu bar icon, the logo, colours or type, in the app, README, website or docs. Use the asset files in this folder as they are. Do not redraw, recolour or "improve" the logo.

---

## 1. Asset map

```
muesli-brand/
├── BRAND.md                      ← this file
├── logo/
│   ├── mark.svg                  ← logo mark, light backgrounds
│   ├── mark-on-dark.svg          ← logo mark, dark backgrounds
│   ├── mark-mono.svg             ← one colour, uses currentColor
│   ├── lockup.svg                ← mark + "muesli" wordmark (text outlined)
│   ├── lockup-on-dark.svg
│   └── lockup-slogan.svg         ← mark + wordmark + slogan
├── app-icon/
│   ├── app-icon-1024.svg / .png  ← macOS app icon master (light)
│   ├── app-icon-dark-1024.svg    ← dark variant (optional, e.g. docs/website)
│   ├── extension-icon-512.png    ← the tile cropped to its edges, for the Claude Desktop extension
│   └── AppIcon.iconset/          ← all macOS sizes, ready for iconutil
└── menubar/
    ├── muesliTemplate.svg
    ├── muesliTemplate.png        ← 18 × 18 px
    ├── muesliTemplate@2x.png     ← 36 × 36 px
    ├── muesliTranscribing(@2x).png        ← with the red dot, while transcribing
    ├── muesliUpdate(@2x).png              ← with the Pumpkin seed dot, when an update is ready
    └── muesliTrayLight(Transcribing|Update)(@2x).png ← milk versions for a dark Windows taskbar
```

Suggested place in the repo: `assets/brand/` (copy the whole folder).

---

## 2. The logo

**Concept:** a muesli bowl with a voice waveform rising out of it: what gets said in the meeting, served up raw.

**Construction** (viewBox `0 0 100 100`, all strokes round caps and joins):

```svg
<svg viewBox="0 0 100 100" fill="none" stroke="#3B2A1E" stroke-width="5"
     stroke-linecap="round" stroke-linejoin="round">
  <!-- bowl: closed, flat rim at y=62 -->
  <path d="M14 62 L86 62 C86 79 70 89 50 89 C30 89 14 79 14 62 Z"/>
  <!-- waveform: 7 bars, uneven heights, all start at y=52 (gap above the rim) -->
  <path d="M22 52 L22 44"/>
  <path d="M31 52 L31 32"/>
  <path d="M40 52 L40 38"/>
  <path d="M49 52 L49 18" stroke="#5B7A3A"/>  <!-- tallest bar = accent -->
  <path d="M58 52 L58 30"/>
  <path d="M67 52 L67 26"/>
  <path d="M76 52 L76 44"/>
</svg>
```

Rules:
- Bowl and wave together fit an invisible circle. Never draw that circle.
- The wave is **not** symmetric, and it never touches or crosses the rim.
- Only the tallest bar carries the accent colour. Everything else is one colour.
- Line art only: no fills, gradients, shadows or 3D.
- Clear space: at least the height of the bowl on every side.
- Minimum size: 16 px for the mark, 96 px wide for the lockup.
- Don't: rotate, stretch, outline, add bars, make the wave symmetric, recolour other bars, or put the mark on a busy photo.

**Wordmark:** always lowercase `muesli`, Outfit Medium (500), tracking −3%. In the lockup files the text is already outlined, so no font is needed.

**Slogan:** `Granola's raw, unbaked cousin`, written exactly like that (sentence case, curly or straight apostrophe, no full stop). Use it on the README hero, website, App Store or GitHub description and the About window. It is set in Outfit Regular or Medium: `pumpkin-seed` at 24 px and up, `pumpkin-seed-dark` when smaller, roast or milk elsewhere.

---

## 3. Colour tokens

| Token          | Hex       | Use |
|----------------|-----------|-----|
| `roast`        | `#3B2A1E` | Logo lines, wordmark, body text, dark backgrounds |
| `pumpkin-seed` | `#5B7A3A` | Accent bar in the logo, slogan at 24 px and up |
| `pumpkin-seed-dark` | `#4E6B31` | Green text below 24 px (links), primary button background |
| `pumpkin-seed-light` | `#A9C77C` | Accent on dark backgrounds |
| `oat`          | `#EADFC8` | App icon tile, subtle surfaces |
| `milk`         | `#F6F1E7` | Page background, lines on dark |
| `muted`        | `#6B5A4B` | Secondary text on milk/oat |
| `line`         | `#DCD0B8` | Hairline borders and dividers |

Text contrast (WCAG AA): roast (12.2:1) and muted (5.9:1) on milk pass for all text. `pumpkin-seed` on milk is 4.35:1, so use it only for large text (24 px and up) and graphics; for smaller green text, and for buttons with milk text, use `pumpkin-seed-dark` (5.4:1). Never put text in `pumpkin-seed-light` on light backgrounds.

```css
:root {
  --muesli-roast: #3B2A1E;
  --muesli-pumpkin-seed: #5B7A3A;
  --muesli-pumpkin-seed-dark: #4E6B31;
  --muesli-pumpkin-seed-light: #A9C77C;
  --muesli-oat: #EADFC8;
  --muesli-milk: #F6F1E7;
  --muesli-muted: #6B5A4B;
  --muesli-line: #DCD0B8;
  --muesli-font: "Outfit", system-ui, sans-serif;
}
```

```swift
import SwiftUI
extension Color {
    static let muesliRoast       = Color(red: 0x3B/255, green: 0x2A/255, blue: 0x1E/255)
    static let muesliPumpkinSeed = Color(red: 0x5B/255, green: 0x7A/255, blue: 0x3A/255)
    static let muesliPumpkinSeedDark  = Color(red: 0x4E/255, green: 0x6B/255, blue: 0x31/255)
    static let muesliPumpkinSeedLight = Color(red: 0xA9/255, green: 0xC7/255, blue: 0x7C/255)
    static let muesliOat         = Color(red: 0xEA/255, green: 0xDF/255, blue: 0xC8/255)
    static let muesliMilk        = Color(red: 0xF6/255, green: 0xF1/255, blue: 0xE7/255)
}
```

In native macOS UI, keep system colours for standard controls and use `pumpkin-seed-dark` as the app's accent colour (`AccentColor` in the asset catalog).

---

## 4. Typography

- **Outfit** (Google Fonts, SIL Open Font License). Medium 500 for headings and the wordmark, Regular 400 for body.
- In the native macOS app, use the system font (SF Pro) for UI. Outfit is for the brand: website, README images, About window title.

---

## 5. macOS app icon

- Master: `app-icon/app-icon-1024.svg`: 1024 × 1024 canvas, rounded tile 824 × 824 at (100,100), corner radius 185, oat background, roast lines, green accent bar. This follows Apple's macOS icon grid; do not add your own extra rounding or mask.
- Build the `.icns`:
  ```sh
  iconutil -c icns assets/brand/app-icon/AppIcon.iconset -o Muesli.icns
  ```
- Or in Xcode: drag the PNGs from `AppIcon.iconset` into `Assets.xcassets/AppIcon.appiconset` (16, 32, 128, 256, 512 at 1x and 2x).
- `extension-icon-512.png` is the 824 × 824 tile of the master, cropped to its edges and scaled to 512 px, without the 100 px transparent margin of the macOS icon grid. Claude Desktop draws the extension icon in a box of its own, so with the margin the tile looked too small there. Regenerate it from the master if the master changes (crop 100,100 to 924,924).
- Electron/Tauri: point the bundler's icon setting at the `.icns` (macOS) and `app-icon-1024.png` (other platforms).

## 6. macOS menu bar icon

- Use `menubar/muesliTemplate.png` + `@2x` (or the SVG/PDF). It's a **template image**: black on transparent, simplified to 5 bars with a heavier line. macOS tints it for light mode, dark mode and the highlighted state.
- Never use the coloured logo or the app icon in the menu bar.
- Keep the `Template` suffix in the file name, and also set it in code:

```swift
// AppKit
let image = NSImage(named: "muesliTemplate")
image?.isTemplate = true
image?.size = NSSize(width: 18, height: 18)
statusItem.button?.image = image

// SwiftUI (macOS 13+)
MenuBarExtra { ContentView() } label: {
    Image("muesliTemplate").renderingMode(.template)
}
```

In an asset catalog, set the image set's **Render As** to *Template Image*.

- While transcribing, the icon carries a small red dot (`#B3382C`) at its top right: `menubar/muesliTranscribing.png` + `@2x`. This is the one exception to the template rule: the dot image is not a template image, so macOS does not tint it.
- When an update is ready (and Muesli is not busy), the icon carries the same dot in `pumpkin-seed` (`#5B7A3A`) instead: `menubar/muesliUpdate.png` + `@2x`, and `muesliTrayLightUpdate.png` + `@2x` for a dark Windows taskbar. Not a template image either. The red transcribing dot wins when both apply. These are the transcribing images with the dot recoloured; regenerate them the same way if those change.
- Windows: the taskbar is dark by default, so the tray uses milk-coloured versions on a dark taskbar (`menubar/muesliTrayLight.png`, `muesliTrayLightTranscribing.png`, each with `@2x`) and the black ones on a light taskbar.

---

## 7. UI style notes

- Calm, warm and plain: milk backgrounds, roast text, one green accent.
- Cards: soft tint (`oat`) or a hairline `line` border with rounded corners. **No coloured accent stripes on the top or left edge of cards.** Use tints, dots or small badges for status instead.
- Icons in the UI: thin line icons with round caps, to match the logo.
- No emoji as icons, no gradients, no glassy effects.

## 8. Voice

- Lowercase product name in the logo, but write "Muesli" with a capital in running text.
- Friendly, short and plain. The Granola reference is a wink, not a jab: never criticise Granola, and don't use Granola's logo, colours or screenshots.
- Example README opener: **Muesli: Granola's raw, unbaked cousin.** An open-source meeting notes app that runs on your Mac.
