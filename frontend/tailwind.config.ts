import type { Config } from 'tailwindcss';

const config: Config = {
  darkMode: ['class'],
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
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
    },
  },
  plugins: [],
};

export default config;
