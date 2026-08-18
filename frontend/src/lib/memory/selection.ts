/**
 * Turns a raw text selection into the `title` / `body` pair the `remember`
 * mutation expects.
 *
 * A memory title is one sentence — that is the contract the whole memory layer
 * rests on — so a long selection is cut at the last sentence end that fits and
 * the remainder becomes the body. With no sentence end in reach, the title is
 * an elided excerpt and the body keeps the selection whole: a truncated title
 * must never be the only surviving copy of what the user selected.
 */

export const SELECTION_TITLE_MAX = 120;

const SENTENCE_END = /[.!?]/;

export interface SelectionSplit {
  readonly title: string;
  readonly body: string | null;
}

/** Collapses every whitespace run — newlines included — into single spaces. */
function normalize(raw: string): string {
  return raw.replace(/\s+/g, ' ').trim();
}

/** Index of the last sentence end at or before `limit`, or -1. */
function lastSentenceEnd(text: string, limit: number): number {
  for (let i = Math.min(limit, text.length) - 1; i >= 0; i--) {
    const char = text[i];
    if (!SENTENCE_END.test(char)) continue;
    const next = text[i + 1];
    if (next === undefined || next === ' ') return i;
  }
  return -1;
}

export function splitSelection(raw: string): SelectionSplit {
  const text = normalize(raw);
  if (text === '') return { title: '', body: null };
  if (text.length <= SELECTION_TITLE_MAX) return { title: text, body: null };

  const end = lastSentenceEnd(text, SELECTION_TITLE_MAX);
  if (end !== -1) {
    const body = text.slice(end + 1).trim();
    return { title: text.slice(0, end + 1), body: body === '' ? null : body };
  }

  const window = text.slice(0, SELECTION_TITLE_MAX);
  const lastSpace = window.lastIndexOf(' ');
  const excerpt = lastSpace > 0 ? window.slice(0, lastSpace) : window;
  return { title: `${excerpt.trimEnd()}…`, body: text };
}
