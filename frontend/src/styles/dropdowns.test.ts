import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const css = readFileSync(new URL('./app.css', import.meta.url), 'utf8');
const dropdowns = readFileSync(new URL('./dropdowns.css', import.meta.url), 'utf8');

describe('shared dropdown styling', () => {
  const rules = [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)]
    .map(([, selector, declarations]) => ({ selector: selector.trim(), declarations }));

  it('keeps a native baseline for listboxes and accessibility fallback', () => {
    const base = rules.find(rule => rule.selector === 'select');
    expect(base?.declarations).toMatch(/appearance:\s*auto\s*;/);
    expect(base?.declarations).toMatch(/background-image:\s*none\s*;/);
    expect(base?.declarations).toMatch(/padding-right:\s*\.65rem\s*;/);
  });

  it('keeps legacy theme rules from overlaying the shared chevron', () => {
    // Includes dark and system-theme rules inside media queries. Drive's
    // component backgrounds can mask this bug, so check the shared rules too.
    for (const rule of rules.filter(rule => /\bselect\b/.test(rule.selector))) {
      expect(rule.declarations, rule.selector).not.toMatch(/background(?:-image)?:[^;]*url\(/);
    }
  });

  it('paints one inset, non-repeating arrow despite component shorthands', () => {
    expect(dropdowns).toMatch(/appearance:\s*none\s*!important/);
    expect(dropdowns).toMatch(/background-image:\s*var\(--mx-select-chevron\)\s*!important/);
    expect(dropdowns).toMatch(/background-repeat:\s*no-repeat\s*!important/);
    expect(dropdowns).toMatch(/background-position:\s*right var\(--mx-select-inset\) center\s*!important/);
    expect(dropdowns).toMatch(/--mx-select-inset:\s*\.75rem/);
    expect(dropdowns).toMatch(/padding-inline-end:\s*calc\(var\(--mx-select-inset\) \+ var\(--mx-select-icon-size\) \+ \.5rem\)\s*!important/);
    expect(dropdowns).toContain('select:not([multiple]):not([size])');
  });

  it('restores the native arrow in forced-colors mode', () => {
    const fallback = dropdowns.slice(dropdowns.indexOf('@media (forced-colors: active)'));
    expect(fallback).toMatch(/appearance:\s*auto\s*!important/);
    expect(fallback).toMatch(/background-image:\s*none\s*!important/);
  });
});
