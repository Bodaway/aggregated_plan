import type { Config } from 'tailwindcss';

/**
 * One accent ramp expressed in a single token.
 *
 * The 50/100 rungs are what light-theme code uses for tinted badge grounds,
 * so they resolve to the accent barely mixed into the page ground; 200 is a
 * hairline; 300 up is the accent itself. Keeping every rung defined matters —
 * a missing one falls back to Tailwind's own bright default and reads as a
 * hole punched in the palette.
 */
function accent(token: string): Record<string, string> {
  const c = `var(${token})`;
  return {
    50: `color-mix(in srgb, ${c} 12%, var(--cn-bg))`,
    100: `color-mix(in srgb, ${c} 17%, var(--cn-bg))`,
    200: `color-mix(in srgb, ${c} 30%, transparent)`,
    300: `color-mix(in srgb, ${c} 60%, var(--cn-fg))`,
    /* 400 and 500 stay the pure accent: those are the rungs light-theme code
       fills with. The darker rungs are what it writes TEXT with — measured on
       a dark ground they came back at 2.8-3.2:1, under AA — so they lift
       toward the foreground instead. Same hue, enough luminance to read. */
    400: `color-mix(in srgb, ${c} 82%, var(--cn-fg))`,
    500: c,
    600: `color-mix(in srgb, ${c} 78%, var(--cn-fg))`,
    700: `color-mix(in srgb, ${c} 70%, var(--cn-fg))`,
    800: `color-mix(in srgb, ${c} 64%, var(--cn-fg))`,
    900: `color-mix(in srgb, ${c} 58%, var(--cn-fg))`,
    950: `color-mix(in srgb, ${c} 58%, var(--cn-fg))`,
  };
}

const config: Config = {
  darkMode: ['class'],
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        /* --- the neutral and accent scales, remapped onto CyberNord ---
           The app was written light: `bg-white` cards on a `bg-gray-50`
           ground, `text-gray-800` copy, `border-gray-200` hairlines — 1253
           such utilities across 62 files. Rather than rewrite them, invert
           what the scale MEANS. Low numbers stay "recessive surface", high
           numbers stay "assertive ink"; only now the surfaces are dark and
           the ink is light, so every existing className keeps its intent and
           flips at once.

           The accents follow the same logic: the 50/100 rungs, used as tinted
           badge grounds, become the accent barely mixed into the page ground,
           while 500+ stay the accent itself. Painting the desktop theme
           repaints the whole app, exactly as it already repaints the HUD. */
        white: 'var(--cn-surface)',
        black: 'var(--cn-bg)',
        gray: {
          50: 'var(--cn-bg)',
          100: 'color-mix(in srgb, var(--cn-surface) 55%, var(--cn-bg))',
          200: 'color-mix(in srgb, var(--cn-dim) 55%, transparent)',
          300: 'var(--cn-dim)',
          /* Raised after measuring every route: at 40/55/70 the muted rungs
             landed between 2.8:1 and 4.0:1 against the panel they sit on,
             under WCAG AA for body text. These are what `text-gray-500` and
             friends resolve to, and this app uses them for real prose. */
          400: 'color-mix(in srgb, var(--cn-fg) 58%, var(--cn-dim))',
          500: 'color-mix(in srgb, var(--cn-fg) 70%, var(--cn-dim))',
          600: 'color-mix(in srgb, var(--cn-fg) 82%, var(--cn-dim))',
          700: 'color-mix(in srgb, var(--cn-fg) 92%, var(--cn-dim))',
          800: 'var(--cn-fg)',
          900: 'var(--cn-fg)',
        },
        slate: {
          50: 'var(--cn-bg)',
          100: 'color-mix(in srgb, var(--cn-surface) 55%, var(--cn-bg))',
          200: 'color-mix(in srgb, var(--cn-dim) 55%, transparent)',
          300: 'var(--cn-dim)',
          /* Raised after measuring every route: at 40/55/70 the muted rungs
             landed between 2.8:1 and 4.0:1 against the panel they sit on,
             under WCAG AA for body text. These are what `text-gray-500` and
             friends resolve to, and this app uses them for real prose. */
          400: 'color-mix(in srgb, var(--cn-fg) 58%, var(--cn-dim))',
          500: 'color-mix(in srgb, var(--cn-fg) 70%, var(--cn-dim))',
          600: 'color-mix(in srgb, var(--cn-fg) 82%, var(--cn-dim))',
          700: 'color-mix(in srgb, var(--cn-fg) 92%, var(--cn-dim))',
          800: 'var(--cn-fg)',
          900: 'var(--cn-fg)',
        },
        blue: accent('--cn-teal'),
        indigo: accent('--cn-purple'),
        purple: accent('--cn-purple'),
        green: accent('--cn-green'),
        emerald: accent('--cn-green'),
        red: accent('--cn-red'),
        rose: accent('--cn-red'),
        yellow: accent('--cn-yellow'),
        amber: accent('--cn-orange'),
        orange: accent('--cn-orange'),
        cn: {
          bg: 'var(--cn-bg)',
          surface: 'var(--cn-surface)',
          dim: 'var(--cn-dim)',
          fg: 'var(--cn-fg)',
          blue: 'var(--cn-blue)',
          green: 'var(--cn-green)',
          yellow: 'var(--cn-yellow)',
          red: 'var(--cn-red)',
          purple: 'var(--cn-purple)',
          teal: 'var(--cn-teal)',
          orange: 'var(--cn-orange)',
        },
      },
      fontFamily: {
        cn: ['var(--cn-font)'],
      },
      /* Angular, like the HUD. `rounded-lg` is everywhere in this codebase and
         soft corners are most of what made it read as a generic light app;
         the pill and the circle keep their meaning (badges, status dots). */
      borderRadius: {
        none: '0',
        sm: '1px',
        DEFAULT: '2px',
        md: '2px',
        lg: '3px',
        xl: '3px',
        '2xl': '4px',
        '3xl': '4px',
        full: '9999px',
      },
      /* Light-theme drop shadows are invisible on a dark ground and only
         muddy it. Depth comes from the hairline and the surface step. */
      boxShadow: {
        none: 'none',
        sm: '0 1px 0 color-mix(in srgb, var(--cn-fg) 4%, transparent) inset',
        DEFAULT: '0 1px 0 color-mix(in srgb, var(--cn-fg) 4%, transparent) inset',
        md: '0 1px 0 color-mix(in srgb, var(--cn-fg) 5%, transparent) inset',
        lg: '0 0 0 1px color-mix(in srgb, var(--cn-dim) 60%, transparent)',
        xl: '0 0 0 1px color-mix(in srgb, var(--cn-dim) 70%, transparent)',
        '2xl': '0 0 34px color-mix(in srgb, var(--cn-teal) 10%, transparent)',
      },
    },
  },
  plugins: [],
};

export default config;
