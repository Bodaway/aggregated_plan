/**
 * True when `el` is where the user is currently typing (or picking a value
 * from a native form control), so a global one-key shortcut must back off
 * rather than steal the keystroke.
 *
 * Covers `<input>`, `<textarea>`, `<select>` and any element the page has
 * made editable via `contenteditable`. Deliberately does not match on ARIA
 * `role="textbox"` / `role="combobox"` alone: every such widget in this
 * codebase today (see HeaderSearchBar) puts that role on a real `<input>`,
 * which the tag check already catches. A role-only match would start
 * guarding a widget that has no underlying native control — a case that
 * does not exist here yet — at the cost of a shortcut going the other way:
 * a decorative `role="textbox"` on a non-interactive element would then
 * silently swallow every shortcut placed on it.
 */
export function isTypingTarget(el: Element | null): boolean {
  if (!el) return false;
  const tag = el.tagName;
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true;
  return (el as HTMLElement).isContentEditable === true;
}
