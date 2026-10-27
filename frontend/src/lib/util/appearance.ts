export interface AppearanceScale {
  interfaceRatio: number;
  fontRatio: number;
}

// Matches the product preview without overwriting an organization's saved brand.
export const DEFAULT_APPEARANCE = {
  preset: 'sage', primary_color: '#c94128', sidebar_color: '#253429',
  radius: 'subtle', density: 'normal', default_theme: 'light', content_width: 'wide'
};
export const APPEARANCE_PRESETS: Record<string, [string, string]> = {
  sage: ['#c94128', '#253429'], blue: ['#1d4ed8', '#0f172a'],
  emerald: ['#047857', '#064e3b'], violet: ['#6d28d9', '#2e1065'],
  amber: ['#b45309', '#451a03'], rose: ['#be123c', '#4c0519'], slate: ['#475569', '#0f172a']
};

export function appearanceScale(interfacePercent: number, fontPercent: number): AppearanceScale {
  const interfaceRatio = Math.min(1.6, Math.max(.85, Number(interfacePercent) / 100 || 1));
  const fontRatio = Math.min(2, Math.max(.625, Number(fontPercent) / 100 || 1));

  return {
    interfaceRatio,
    fontRatio
  };
}

export function validAccentColor(value: string | null | undefined): value is string {
  return typeof value === 'string' && /^#[0-9a-f]{6}$/i.test(value);
}

// Let the default product palette adapt to light/dark. Explicit organization or
// personal accents remain explicit; switching themes must never overwrite them.
export function applyPrimaryAccent(color: string, useThemeDefault: boolean) {
  const root = document.documentElement;
  if (useThemeDefault) {
    root.style.removeProperty('--primary');
    root.style.removeProperty('--on-primary');
  } else {
    root.style.setProperty('--primary', color);
    root.style.setProperty('--on-primary', primaryForeground(color));
  }
}

export function usesProductAccent(preset: string, color: string, personal?: string | null) {
  return !validAccentColor(personal) && preset === 'sage' && color.toLowerCase() === '#c94128';
}

export function primaryForeground(color: string): '#ffffff' | '#0f172a' {
  if (!validAccentColor(color)) return '#ffffff';
  const channels = [1, 3, 5].map((index) => Number.parseInt(color.slice(index, index + 2), 16) / 255);
  const luminance = channels
    .map((channel) => channel <= .04045 ? channel / 12.92 : ((channel + .055) / 1.055) ** 2.4)
    .reduce((sum, channel, index) => sum + channel * [.2126, .7152, .0722][index], 0);
  const whiteContrast = 1.05 / (luminance + .05);
  const darkLuminance = .008;
  const darkContrast = (luminance + .05) / (darkLuminance + .05);
  return darkContrast > whiteContrast ? '#0f172a' : '#ffffff';
}
