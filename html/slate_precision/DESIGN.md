# Design System Document: The Precision Console

## 1. Overview & Creative North Star: The Precision Console
The design system is anchored by the **"Precision Console"**—a creative North Star that treats the interface not as a collection of web components, but as a high-performance laboratory instrument. 

Moving away from the "floating card" aesthetic of modern SaaS, this system embraces the **Integrated Canvas**. It prioritizes stability, density, and clinical authority. We break the standard template look through **intentional data density**—where whitespace is used as a structural separator rather than just "breathing room"—and an editorial approach to information hierarchy that mimics a high-end macOS inspector. The result is an environment that feels silent, expensive, and profoundly capable.

---

## 2. Colors: Tonal Architecture
The palette is a disciplined range of silvers and deep slates, punctuated by a singular, authoritative blue.

### The "No-Line" Rule
Explicitly, 1px solid borders are prohibited for sectioning. Structural boundaries must be defined solely through background color shifts. To separate a sidebar from a main stage, transition from `surface-container-low` to `surface`. This creates a seamless, "milled from a single block" feel that is characteristic of high-end hardware.

### Surface Hierarchy & Nesting
Treat the UI as a series of physical layers using the surface-container tiers:
*   **Base Layer:** `surface` (#fcf8fb) for the primary window background.
*   **Recessed Sections:** `surface-container-low` (#f5f3f6) for global navigation or utility sidebars.
*   **Active Workspaces:** `surface-container-lowest` (#ffffff) for data-entry fields or log outputs, providing the highest contrast for readability.
*   **Elevated Overlays:** `surface-container-high` (#e9e7ed) for ephemeral panels that sit "above" the logic flow.

### The "Subtle Polish" Exception
While the user request avoids playful gradients, we apply a **"Micro-axial Gradient"** to the `primary` (#005cba) CTA buttons. A 1% shift from `primary` to `primary_dim` adds a metallic, tactile weight that flat colors cannot achieve, grounding the button as a physical switch.

---

## 3. Typography: Editorial Authority
The system utilizes a disciplined hierarchy based on the provided scale, interpreted through the lens of a macOS native environment.

*   **The Command Scale (Display & Headline):** Used sparingly for system states (e.g., "System Healthy"). These use `headline-sm` with tight tracking (-0.02em) to feel authoritative and architectural.
*   **The Data Scale (Title & Body):** `body-md` is the workhorse. In diagnostic views, alignment is more important than font size. Use `label-md` for metadata—it should feel like a label on a hardware circuit.
*   **Alignment as Structure:** Typography is used to create "invisible columns." Labels (`on_surface_variant`) must strictly right-align against their data values (`on_surface`) to create a clear vertical spine in the layout.

---

## 4. Elevation & Depth: Tonal Layering
Depth is achieved through "stacking" rather than traditional drop shadows.

*   **The Layering Principle:** To create a "lifted" effect for an inspector panel, place a `surface_container_highest` element over a `surface_dim` background. This creates a natural, soft edge that is easier on the eyes than a high-contrast border.
*   **Ambient Shadows:** If a floating popover is required, use an extra-diffused shadow.
    *   *Spec:* `0px 4px 24px rgba(14, 14, 10, 0.08)`. The shadow color is a tinted version of `inverse_surface` to mimic natural ambient light.
*   **The Ghost Border:** If a separator is required for accessibility, use a "Ghost Border": the `outline_variant` token at 15% opacity. It should be felt, not seen.
*   **Glassmorphism:** Use `surface` at 80% opacity with a `20px` backdrop blur for sidebars. This allows the user’s wallpaper to subtly influence the UI, a hallmark of native macOS Pro apps.

---

## 5. Components: The Diagnostic Toolkit

### Buttons
*   **Primary:** `primary` background with `on_primary` text. `DEFAULT` (0.25rem) roundedness. No heavy shadows; use a 1px inner stroke of `primary_fixed_dim` at 20% to simulate a beveled edge.
*   **Tertiary (Utility):** No background. Use `secondary` text. These should feel like integrated text links until hovered, at which point they settle into a `surface_container_high` background.

### Input Fields
*   **The "Integrated" Input:** Avoid the "floating box." Use `surface_container_lowest` with a bottom-only `outline` at 20% opacity. When focused, the `primary` accent should be a subtle 2px glow rather than a thick border.

### Lists & Tables
*   **Forbid Dividers:** Do not use horizontal lines between rows. Use the Spacing Scale (e.g., `12px` vertical gap) and a subtle `surface_container_low` hover state to define row boundaries.
*   **Status Indicators:** Use `primary` (Active), `error` (Critical), and `outline` (Inactive). These are small (8px) "LED" style circles with no glow, maintaining the "Pro Utility" calm.

### Specialized: The Inspector Panel
A dense, multi-column layout for hardware specs. Use `label-sm` for keys and `body-sm` for values. All keys must be `on_surface_variant` (grey) to recede, while values are `on_surface` (near-black) to pop.

---

## 6. Do's and Don'ts

### Do
*   **Do** use `surface_dim` to create "wells" for terminal or log outputs.
*   **Do** favor asymmetric layouts. A heavy sidebar on the left with a wide, airy data visualization on the right creates a sophisticated, non-template feel.
*   **Do** use `md` (0.375rem) roundedness for large containers and `sm` (0.125rem) for small elements like checkboxes to maintain a precise, "machined" look.

### Don't
*   **Don't** use 100% black. Use `inverse_surface` (#0e0e10) for deep tones to keep the UI feeling "calm" rather than harsh.
*   **Don't** use "playful" animations. Transitions should be "Snappy" (150ms-200ms) and use a linear-out-slow-in easing curve to feel like a high-performance tool.
*   **Don't** use purple or gradients that shift hues. The brand's soul is in its monochromatic precision and the singular blue accent.