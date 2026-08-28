import { describe, it, expect } from 'vitest';
import { isTypingTarget } from './is-typing-target';

function attached<T extends HTMLElement>(el: T): T {
  document.body.appendChild(el);
  return el;
}

describe('isTypingTarget', () => {
  it('catches a text input', () => {
    expect(isTypingTarget(attached(document.createElement('input')))).toBe(true);
  });

  it('catches a textarea', () => {
    expect(isTypingTarget(attached(document.createElement('textarea')))).toBe(true);
  });

  it('catches a select', () => {
    expect(isTypingTarget(attached(document.createElement('select')))).toBe(true);
  });

  it('catches a contenteditable element', () => {
    // jsdom does not implement the `isContentEditable` getter (it always
    // reads false regardless of the contenteditable attribute), so the
    // property is stubbed directly to exercise this branch — see
    // https://github.com/jsdom/jsdom/issues/1670.
    const div = attached(document.createElement('div'));
    Object.defineProperty(div, 'isContentEditable', { value: true, configurable: true });
    expect(isTypingTarget(div)).toBe(true);
  });

  it('does not flag a plain div', () => {
    expect(isTypingTarget(attached(document.createElement('div')))).toBe(false);
  });

  it('does not flag a link', () => {
    expect(isTypingTarget(attached(document.createElement('a')))).toBe(false);
  });

  it('does not flag null (nothing focused)', () => {
    expect(isTypingTarget(null)).toBe(false);
  });
});
