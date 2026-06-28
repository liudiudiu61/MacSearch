import type { Config } from 'tailwindcss';

export default {
  content: ['./index.html', './src/**/*.{vue,ts}'],
  theme: {
    extend: {
      colors: {
        ink: '#101114',
        mist: '#F5F6F8',
        line: '#D9DDE5',
        accent: '#2F6FEB',
        warn: '#B42318'
      },
      boxShadow: {
        panel: '0 24px 80px rgb(16 17 20 / 0.18)'
      }
    }
  },
  plugins: []
} satisfies Config;
