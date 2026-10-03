export interface AppearanceScale {
  interfaceRatio: number;
  fontRatio: number;
}

export function appearanceScale(interfacePercent: number, fontPercent: number): AppearanceScale {
  const interfaceRatio = Math.min(1.6, Math.max(.85, Number(interfacePercent) / 100 || 1));
  const fontRatio = Math.min(1.5, Math.max(.625, Number(fontPercent) / 100 || 1));

  return {
    interfaceRatio,
    fontRatio
  };
}

export function validAccentColor(value: string | null | undefined): value is string {
  return typeof value === 'string' && /^#[0-9a-f]{6}$/i.test(value);
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
