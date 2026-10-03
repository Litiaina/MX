import { describe, expect, it } from 'vitest';
import { appearanceScale, primaryForeground, validAccentColor } from './appearance';

describe('appearanceScale', () => {
  it('keeps type unchanged when only the interface is enlarged', () => {
    const result = appearanceScale(150, 100);

    expect(result.interfaceRatio).toBe(1.5);
    expect(result.fontRatio).toBe(1);
  });

  it('keeps interface dimensions unchanged when only text is enlarged', () => {
    const result = appearanceScale(100, 135);

    expect(result.interfaceRatio).toBe(1);
    expect(result.fontRatio).toBeCloseTo(1.35);
  });

  it('clamps persisted values to the supported ranges', () => {
    expect(appearanceScale(500, 10)).toMatchObject({ interfaceRatio: 1.6, fontRatio: .625 });
  });
});

describe('personal accent colors', () => {
  it('accepts only complete six-digit hexadecimal colors', () => {
    expect(validAccentColor('#2563eb')).toBe(true);
    expect(validAccentColor('#fff')).toBe(false);
    expect(validAccentColor('blue')).toBe(false);
  });

  it('selects readable foregrounds for light and dark accents', () => {
    expect(primaryForeground('#facc15')).toBe('#0f172a');
    expect(primaryForeground('#1d4ed8')).toBe('#ffffff');
  });
});
