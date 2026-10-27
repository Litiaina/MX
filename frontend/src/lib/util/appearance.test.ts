import { describe, expect, it } from 'vitest';
import { APPEARANCE_PRESETS, DEFAULT_APPEARANCE, appearanceScale, primaryForeground, usesProductAccent, validAccentColor } from './appearance';

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
    expect(appearanceScale(100, 500).fontRatio).toBe(2);
    expect(appearanceScale(100, 200).fontRatio).toBe(2);
  });
});

describe('personal accent colors', () => {
  it('adapts only the default product accent, never explicit saved colors', () => {
    expect(usesProductAccent('sage','#c94128',null)).toBe(true);
    expect(usesProductAccent('sage','#c94128','#c94128')).toBe(false);
    expect(usesProductAccent('blue','#1d4ed8',null)).toBe(false);
    expect(usesProductAccent('sage','#663399',null)).toBe(false);
  });
  it('shares the product theme while retaining every existing organization preset', () => {
    expect(DEFAULT_APPEARANCE.primary_color).toBe('#c94128');
    expect(DEFAULT_APPEARANCE.sidebar_color).toBe('#253429');
    expect(APPEARANCE_PRESETS.sage).toEqual(['#c94128', '#253429']);
    expect(APPEARANCE_PRESETS.blue).toEqual(['#1d4ed8', '#0f172a']);
    expect(primaryForeground(DEFAULT_APPEARANCE.primary_color)).toBe('#ffffff');
  });
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
