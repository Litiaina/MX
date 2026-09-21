import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const css = readFileSync(new URL('../../styles/app.css', import.meta.url), 'utf8');

function color(block, name) {
  const match = new RegExp(`--${name}:\\s*(#[0-9a-f]{6})`, 'i').exec(block);
  if (!match) throw new Error(`Missing --${name} theme token`);
  return match[1];
}

function luminance(hex) {
  const channels = [1, 3, 5].map((index) => Number.parseInt(hex.slice(index, index + 2), 16) / 255)
    .map((value) => value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);
  return channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
}

function contrast(foreground, background) {
  const first = luminance(foreground); const second = luminance(background);
  return (Math.max(first, second) + 0.05) / (Math.min(first, second) + 0.05);
}

describe('theme CSS', () => {
  const dark = [...css.matchAll(/\[data-theme="dark"\]\s*\{([^}]*)\}/g)]
    .map((match) => match[1])
    .find((block) => block.includes('--text:')) || '';

  it('keeps dark theme text tokens at readable contrast', () => {
    expect(contrast(color(dark, 'text'), color(dark, 'bg'))).toBeGreaterThanOrEqual(4.5);
    expect(contrast(color(dark, 'text-2'), color(dark, 'surface'))).toBeGreaterThanOrEqual(4.5);
    expect(contrast(color(dark, 'muted'), color(dark, 'surface'))).toBeGreaterThanOrEqual(4.5);
    expect(contrast(color(dark, 'danger'), color(dark, 'danger-surface'))).toBeGreaterThanOrEqual(4.5);
    expect(contrast(color(dark, 'success'), color(dark, 'success-surface'))).toBeGreaterThanOrEqual(4.5);
  });

  it('uses theme tokens for form labels and legends', () => {
    expect(css).toMatch(/label\s*\{[^}]*color:\s*var\(--text-2\)/s);
    expect(css).toMatch(/legend\s*\{[^}]*color:\s*var\(--text-2\)/s);
    expect(css).toMatch(/\.required-mark\s*\{[^}]*color:\s*var\(--danger\)/s);
  });

  it('keeps the compact header aligned and long text untruncated', () => {
    expect(css).toMatch(/\.workspace-topbar\s*>\s*div\.topbar-context\s*\{[^}]*display:\s*flex/s);
    expect(css).not.toContain('.workspace-topbar > div:first-child');
    const denseCells = css.lastIndexOf('.records-table th,\n.records-table td');
    const longText = css.lastIndexOf('.records-table td.long-text-cell');
    expect(longText).toBeGreaterThan(denseCells);
    expect(css.slice(longText, css.indexOf('}', longText))).toContain('white-space: pre-wrap');
  });
});
