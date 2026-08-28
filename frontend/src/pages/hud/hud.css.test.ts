import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, it, expect } from 'vitest';

const CSS = readFileSync(resolve(__dirname, 'hud.css'), 'utf8');

describe('HUD visual foundation', () => {
  it('scales with its container, not the viewport', () => {
    expect(CSS).toMatch(/container-type:\s*size/);
    expect(CSS).toMatch(/font-size:\s*clamp\(10px,\s*0\.74cqw,\s*30px\)/);
    expect(CSS).not.toMatch(/\dvw/);
  });

  it('reflows below the laptop-width breakpoint', () => {
    expect(CSS).toMatch(/@container hud \(max-width:\s*1500px\)/);
  });

  it('blurs panel backdrops so a busy desktop cannot drown the text', () => {
    expect(CSS).toMatch(/backdrop-filter:\s*blur/);
  });

  it('spends its glow on the dominant panel only', () => {
    // Tokens, not hex, carry the glow now — count declarations that shadow
    // in --cn-teal, not a literal rgba(8, 247, 254, ...) that a token-based
    // sheet will never contain. Exactly one: the budget is spent, not
    // withheld, and never duplicated onto a second panel.
    const lit = (CSS.match(/box-shadow:[^;]*var\(--cn-teal\)/g) ?? []).length;
    expect(lit).toBe(1);
  });

  it('takes every colour from the CyberNord tokens', () => {
    const hardcoded = CSS.match(/#[0-9a-fA-F]{6}/g) ?? [];
    expect(hardcoded).toEqual([]);
  });
});
