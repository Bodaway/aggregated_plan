/**
 * Stacking layers for the memory UI.
 *
 * Inline `zIndex` values rather than Tailwind classes on purpose: the value has
 * to be readable from a test, and a `z-[${n}]` template would never reach
 * Tailwind's JIT. Two of these have to be *strictly* above the task sheets —
 * `TaskEditSheet` is rendered after the page content in `SearchProvider`, so at
 * an equal z-index DOM order hands it the win and it covers the memory sheet
 * entirely: the screen greys out and nothing appears.
 */

/** `TaskEditSheet` / `TaskCreateSheet` panels — Tailwind `z-50`. */
export const TASK_SHEET_Z = 50;

/** The capture chip: above the sheets, so a selection made inside one stays capturable. */
export const CAPTURE_CHIP_Z = 60;

export const MEMORY_BACKDROP_Z = 70;
export const MEMORY_SHEET_Z = 80;
