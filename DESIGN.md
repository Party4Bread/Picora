---
name: Picora
description: A compact native sticker picker and library that keeps sticker art in focus.
colors:
  light-panel: "#fafbfc"
  light-window: "#ffffff"
  light-field: "#eef1f4"
  light-subtle: "#f2f4f6"
  light-text: "#242b32"
  light-muted: "#5d6b78"
  light-selection: "#d8efe8"
  light-accent: "#106c52"
  light-border: "#dce2e7"
  light-hover: "#e4eaef"
  dark-panel: "#181b1f"
  dark-window: "#1f2328"
  dark-field: "#22272d"
  dark-subtle: "#1e2227"
  dark-text: "#e6eaee"
  dark-muted: "#9da8b4"
  dark-selection: "#204442"
  dark-accent: "#6ed7b7"
  dark-border: "#313840"
  dark-hover: "#2d353d"
typography:
  heading:
    fontFamily: Noto Sans KR
    fontSize: 22px
  body:
    fontFamily: Noto Sans KR
    fontSize: 14px
  button:
    fontFamily: Noto Sans KR
    fontSize: 14px
  small:
    fontFamily: Noto Sans KR
    fontSize: 12px
rounded:
  control: 6px
  thumbnail: 8px
  window: 10px
spacing:
  item: 8px
  picker-margin: 12px
  library-margin: 24px
components:
  button:
    rounded: "{rounded.control}"
    padding: 6px 12px
    typography: "{typography.button}"
  navigation-row:
    height: 32px
    rounded: "{rounded.control}"
    typography: "{typography.body}"
  sticker-tile:
    width: 80px
    height: 80px
    rounded: "{rounded.thumbnail}"
  search:
    height: 36px
    padding: 8px 12px
    rounded: "{rounded.control}"
---

# Design System: Picora

## Overview

Picora is a compact utility inspired by the operating system's emoji picker. Sticker art supplies the visual character; quiet neutral surfaces and restrained teal selection keep the controls readable without competing with it. Korean and Latin content share the bundled Noto Sans KR typeface (OFL).

The implementation is native Rust with egui/eframe. This document records the shared styling in `src/app/design.rs` and the picker, library, and thumbnail components; dimensions are egui logical points, represented as px in the portable tokens above. Preserve the existing native interface rather than introducing web styling or decorative artwork.

## Colors

The frontmatter records paired light and dark palettes. System, Light, and Dark are available appearance choices.

- **Primary:** the teal accent identifies selection outlines, selected navigation text, and links. The softer selection fill supports it without overpowering sticker art.
- **Neutral:** panel is the main canvas; window is the floating-window surface; field supports inputs and solid controls; subtle separates the library sidebar and quiet controls from the canvas.
- **Text:** use normal text for labels and muted text for counts, source metadata, hints, and routine status.
- **States:** hover uses a neutral fill; active controls use the selection fill. Borders use the neutral border token, with muted strokes on hover. Error text retains egui's theme-specific error color.

## Typography

Noto Sans KR Regular is prepended to egui's proportional font family, retaining egui fallbacks. Headings use the heading token, controls and navigation use body/button, and metadata uses small. Emphasis uses egui's `strong()` treatment rather than a separate bundled bold face.

The library wordmark uses 23 points and the picker header uses 18 points. Long navigation labels truncate to one line and expose their full text on hover; counts remain aligned to the right. Preserve Korean source titles rather than replacing them with shortened English labels.

## Layout

The resizable library opens at 960 × 700 with a minimum of 640 × 460. Its fixed sidebar is 200 points wide with 12-point inner margins; the main content uses 24-point margins. Sidebar rows have 4-point vertical spacing, and the collection list scrolls independently above bottom-aligned Accounts, Settings, and Open picker actions.

The picker uses 12-point margins and stacks a compact header, full-width search, collection controls, and sticker grid. A 48-point bottom strip holds 38-point pack thumbnails in a horizontal scroll area above the short status footer. At content widths below 380 points, the collection menu reads “More” and uses a 68-point width instead of the 140-point “Packs & groups” control.

At picker heights below 400 points, the separate collection/count row is omitted; a selected pack or group name appears in the header, truncated with a full-title tooltip. Taller pickers show collection context above the grid.

Sticker grids adapt their column count to available width using 80-point cells and 8-point gaps. Keep cell sizes stable while resizing. Long grids use virtualized vertical scrolling, and solid scrollbars remain visible when content overflows.

## Elevation & Depth

The main interface is flat: spacing, tonal surfaces, and thin separators establish hierarchy. Sticker tiles have no resting frame, shadow, or enclosing card. Floating windows and popups retain egui's native depth treatment; do not infer a custom shadow system from the flat main canvas.

## Shapes

Controls and navigation rows use softly rounded corners; thumbnails use a slightly larger radius for their interaction background. Floating windows use the window radius. Selection and keyboard-focus outlines are thin (1 point), drawn inside thumbnail and navigation bounds. Sticker images retain their original proportions and sit within a 7-point tile inset; rounded interaction backgrounds do not imply cropping the artwork.

## Components

- **Buttons:** compact neutral controls with the shared padding and a 30-point minimum interaction height. The picker Library action has no frame at rest. Hover is neutral; active state is teal-tinted.
- **Search:** a single-line field spanning the content width, with the “Search stickers…” hint. Picker search receives focus on opening; Up/Down transfers navigation to the grid.
- **Navigation:** full-width rows with a transparent resting background, neutral hover, and teal selection. Keyboard focus has a selection-colored outline. Saved packs show right-aligned counts; custom groups share the same row treatment.
- **Collection controls:** Favorites, Recent, and All use selectable controls beside the packs/groups menu. The bottom thumbnail strip offers direct pack switching and title tooltips.
- **Sticker tiles:** unboxed at rest, neutral on hover, teal-filled and outlined when selected. Keyboard focus also gets an outline. Animated media plays on hover; loading uses a small ring, and failed media displays a clickable Retry label with error details on hover. Labels, pack titles, and source information live in tooltips rather than permanent captions.
- **Discovery:** thumbnail-and-text rows separated by thin rules, with source metadata and links. Pack previews use a native floating window and the shared sticker grid.
- **Status and empty states:** the short footer shows muted status, a busy indicator when needed, and visible error text. Empty collections and searches provide a concise explanation and a direct recovery action.

## Do's and Don'ts

- **Do** keep sticker art as the primary visual content and use the shared theme tokens for both appearances.
- **Do** preserve keyboard and mouse parity: arrows navigate, Enter chooses, Escape hides the picker, and context menus expose original-file copy and group actions.
- **Do** keep long titles discoverable, scrolling visible, and failures actionable.
- **Don't** add decorative heroes, generated artwork, web UI, or framed cards around every sticker.
- **Don't** stretch sticker art, shrink tiles to force more columns, or use accent fills for unrelated decoration.
