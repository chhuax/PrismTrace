# Midnight Obsidian Design System

### 1. Overview & Creative North Star
**Creative North Star: "The Machined Monolith"**
Midnight Obsidian is a high-density, technical design system inspired by professional macOS console environments and aviation telemetry. It rejects the "softness" of consumer web apps in favor of a precision-engineered, brutalist aesthetic. The system emphasizes high-information density, mono-spaced rhythm, and tactical depth. It breaks the traditional grid through "machined" compartmentalization—using razor-sharp dividers and varying background depths to organize complex data streams without visual clutter.

### 2. Colors
The palette is rooted in deep obsidian blacks and slate grays, punctuated by high-visibility "Status Primary" blues and "Alert" reds.

*   **The "No-Line" Rule:** Sectioning is primarily achieved through background color shifts (e.g., `surface-container-low` vs `surface-container-high`). When physical separation is required, use "Machined Borders": a 1px line with 30% opacity (`outline-variant/30`). Never use solid 1px borders for general layout.
*   **Surface Hierarchy & Nesting:** 
    *   **Lowest (#000000):** Deepest wells for code blocks and activity logs.
    *   **Low (#0F131C):** Secondary workspace areas.
    *   **Surface (#0B0E14):** The primary canvas background.
    *   **High/Highest (#18202E):** Interactive sidebars and inspection panels.
*   **The "Glass & Gradient" Rule:** Use `backdrop-blur-xl` with 90% opacity for global headers to create a "floating" tech-glass feel.
*   **Signature Textures:** Interactive states use subtle 5% color overlays (e.g., `primary/5`) rather than heavy fills.

### 3. Typography
The system uses a unified Inter scale for UI controls, but shifts to JetBrains Mono for all data-driven content to reinforce the "developer-first" identity.

**Typography Scale:**
*   **Display/Headline:** Inter, Bold, Tracking -0.05em. Used for primary system headers.
*   **Technical Labels:** 9px - 10px, Mono-spaced, Uppercase, Tracking 0.1em. This is the workhorse for metadata and headers.
*   **Data Rows:** 12px (0.75rem), Mono-spaced. Optimized for legibility in dense tables.
*   **UI Secondary:** 14px (0.875rem), Inter. Used for standard navigation and search.

The typographic rhythm is intentionally small, maximizing screen real estate for professional workflows while maintaining accessibility through high-contrast text (`#DCE5FD` on `#0B0E14`).

### 4. Elevation & Depth
Elevation is communicated through "Tonal Layering" and inner shadows rather than traditional drop shadows.

*   **The Layering Principle:** Depth is "sunk" into the UI. The `surface-container-lowest` is used for "wells"—areas that contain logs or code—giving the impression they are etched into the console.
*   **Ambient Shadows:** Use `shadow-sm` for sticky headers to provide minimal separation.
*   **The "Well-Glow" Shadow:** For nested containers, use an inset shadow: `inset 0 2px 4px rgba(0,0,0,0.4)` to simulate depth.
*   **Machined Borders:** Use `border-r` or `border-b` with `rgba(63, 72, 90, 0.3)` to create the appearance of milled aluminum components.

### 5. Components
*   **Buttons:** Razor-sharp (2px radius). Primary buttons use a solid `primary` fill; secondary buttons use a "well" style with `outline-variant/40`.
*   **Inputs:** Minimalist search fields with 1px `outline-variant/30` rings. No background fill on focus; only a subtle glow shift.
*   **Status Pills:** Small, circular 4px dots next to mono-spaced text (e.g., Emerald-400 for OK, Error for Fail).
*   **Data Tables:** No vertical borders. Horizontal "Machined" dividers only. Row hovering triggers a subtle `primary/5` tint.
*   **Tabs:** Segmented control style using `surface-container-lowest` as a background track with a `primary-container` active indicator.

### 6. Do's and Don'ts
**Do:**
*   Use uppercase labels for all metadata to enhance the "instrumental" feel.
*   Maintain 1px precision for all dividers.
*   Use mono-spaced fonts for any value that might change (numbers, timestamps, IDs).

**Don't:**
*   Never use rounded corners greater than 4px (except for status dots).
*   Avoid vibrant gradients; use solid tonal shifts or 1px accent borders.
*   Do not use standard blue for links; use `primary` (#A9C7FF) or `secondary` (#989EAC) for a more professional tone.