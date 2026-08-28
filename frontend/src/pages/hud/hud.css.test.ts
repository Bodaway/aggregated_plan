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
    // sheet will never contain. Exactly one shadow total, file-wide — on the
    // panel or on anything nested inside it: the budget is spent once, never
    // withheld and never duplicated onto a second hot spot.
    const lit = (CSS.match(/box-shadow:[^;]*var\(--cn-teal\)/g) ?? []).length;
    expect(lit).toBe(1);
  });

  it('takes every colour from the CyberNord tokens', () => {
    const hardcoded = CSS.match(/#[0-9a-fA-F]{6}/g) ?? [];
    expect(hardcoded).toEqual([]);
  });

  it('keeps panel backgrounds opaque enough to occlude a busy backdrop', () => {
    // Task-2 review finding 4: a dark desktop carrying its own high-contrast
    // detail (a terminal, light monospace on near-black) bled through as
    // legible noise at the old 72% floor. Every panel background built from
    // --cn-bg / --cn-surface must stay at or above the 92% floor that fixed
    // it, or the occlusion regresses silently.
    const opacities = [...CSS.matchAll(/color-mix\(in srgb, var\(--cn-(?:bg|surface)\) (\d+)%,\s*transparent\)/g)]
      .map((m) => Number(m[1]));
    expect(opacities.length).toBeGreaterThan(0);
    for (const pct of opacities) {
      expect(pct).toBeGreaterThanOrEqual(90);
    }
  });
});
