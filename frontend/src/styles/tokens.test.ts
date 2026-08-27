import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const CSS = readFileSync(resolve(__dirname, 'cybernord.css'), 'utf8');

const REQUIRED = [
  '--cn-bg', '--cn-fg', '--cn-dim', '--cn-surface', '--cn-blue', '--cn-green',
  '--cn-yellow', '--cn-red', '--cn-purple', '--cn-teal', '--cn-orange', '--cn-font',
] as const;

describe('tokens CyberNord', () => {
  it('déclare toutes les custom properties sur :root', () => {
    for (const token of REQUIRED) {
      expect(CSS).toContain(`${token}:`);
    }
  });

  it('porte un avertissement de non-édition', () => {
    expect(CSS).toMatch(/généré par apply-theme\.sh/i);
  });

  it('utilise des couleurs hexadécimales à 6 chiffres', () => {
    const colors = CSS.match(/--cn-(?!font)[a-z]+:\s*([^;]+);/g) ?? [];
    expect(colors.length).toBeGreaterThanOrEqual(11);
    for (const decl of colors) {
      expect(decl).toMatch(/#[0-9a-fA-F]{6}/);
    }
  });
});
