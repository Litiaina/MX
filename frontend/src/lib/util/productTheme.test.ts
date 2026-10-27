import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

// Test the active token layer, not the historical legacy stylesheet.
const source = readFileSync(new URL('../../styles/tokens.css', import.meta.url), 'utf8');
const light = source.slice(source.indexOf(':root {'), source.indexOf("[data-theme='dark']"));
const dark = source.split("[data-theme='dark'] {")[1].split('}')[0];
const system = source.split("[data-theme='system'] {")[1].split('}')[0];
const token = (css: string, name: string) => {
  const value = new RegExp(`--${name}:\\s*(#[a-f0-9]{6})`, 'i').exec(css)?.[1];
  if (!value) throw Error(`Missing theme token: ${name}`);
  return value;
};
function luminance(hex: string) {
  return [1, 3, 5].map(i => parseInt(hex.slice(i, i + 2), 16) / 255)
    .map(v => v <= .04045 ? v / 12.92 : ((v + .055) / 1.055) ** 2.4)
    .reduce((sum, v, i) => sum + v * [.2126, .7152, .0722][i], 0);
}
function contrast(a: string, b: string) {
  const x = luminance(a), y = luminance(b);
  return (Math.max(x, y) + .05) / (Math.min(x, y) + .05);
}

describe('MX product visual language', () => {
  it('matches the reference accent, ink, rail and sage content surfaces', () => {
    expect(token(light, 'primary')).toBe('#c94128');
    expect(token(light, 'text')).toBe('#20221e');
    expect(token(light, 'sidebar')).toBe('#f5f6f1');
    expect(token(light, 'surface')).toBe('#ffffff');
    expect(contrast('#ffffff', token(light, 'primary'))).toBeGreaterThanOrEqual(4.5);
  });

  for (const [name, css] of [['light', light], ['dark', dark], ['system-dark', system]]) {
    it(`${name}: every category badge remains readable`, () => {
      for (const color of ['slate','blue','green','amber','violet','rose']) {
        expect(contrast(token(css,`tag-${color}-text`),token(css,`tag-${color}-bg`)),color).toBeGreaterThanOrEqual(4.5);
      }
    });
    it(`${name}: readable text, secondary labels, status feedback and collaboration`, () => {
      for (const [fg, bg] of [
        ['text', 'bg'], ['text', 'surface'], ['text-2', 'surface'],
        ['muted', 'surface'], ['muted', 'surface-2'],
        ['danger', 'danger-surface'], ['success', 'success-surface'],
        ['warning', 'warning-surface'], ['collab-text', 'collab-bg'],
        ['collab-muted', 'collab-bg'], ['collab-selected-text', 'collab-selected']
      ]) {
        expect(contrast(token(css, fg), token(css, bg)), `${fg} / ${bg}`).toBeGreaterThanOrEqual(4.5);
      }
    });
  }

  it('keeps automatic dark mode identical to the explicit dark palette', () => {
    expect(token(dark,'bg')).toBe('#090a0b');
    expect(token(dark,'primary')).toBe('#2563eb');
    for (const name of ['sidebar', 'bg', 'surface', 'surface-2', 'surface-3', 'text', 'text-2', 'muted', 'line', 'line-strong', 'chrome', 'collab-selected']) {
      expect(token(system, name)).toBe(token(dark, name));
    }
  });
});
