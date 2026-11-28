import { useColorScheme } from 'react-native';

export type Palette = {
  bg: string;
  card: string;
  border: string;
  inset: string;
  text: string;
  muted: string;
  accent: string;
  strength: string;
  cardio: string;
  focus: string;
  good: string;
  warn: string;
};

export const palettes: Record<'light' | 'dark', Palette> = {
  dark: {
    bg: '#0E1110',
    card: '#171B1A',
    border: '#242A28',
    inset: '#0F1312',
    text: '#EEF1EF',
    muted: '#8C9794',
    accent: '#4FD1B5',
    strength: '#F2708A',
    cardio: '#5FE0C0',
    focus: '#A99BFF',
    good: '#5FD38D',
    warn: '#F2B24A',
  },
  light: {
    bg: '#F6F7F4',
    card: '#FFFFFF',
    border: '#E3E5E0',
    inset: '#EEF0EB',
    text: '#1B1F1E',
    muted: '#6A7371',
    accent: '#1F8A70',
    strength: '#D6426A',
    cardio: '#1F8A70',
    focus: '#6D5BD9',
    good: '#2E9E5E',
    warn: '#B87A12',
  },
};

export const fontSizes = { title: 26, cardTitle: 18, body: 14, caption: 12 } as const;
export const radius = 16;
/** 8-pt grid. */
export const spacing = (n: number): number => n * 8;

/** Dark is the default; light only when the system explicitly asks for it. */
export function useTheme(): Palette {
  return palettes[useColorScheme() === 'light' ? 'light' : 'dark'];
}
