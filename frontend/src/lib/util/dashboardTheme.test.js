import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const chart = readFileSync(new URL('../components/ReportChart.svelte', import.meta.url), 'utf8');
const legacy = readFileSync(new URL('../../styles/app.css', import.meta.url), 'utf8');
const tokens = readFileSync(new URL('../../styles/tokens.css', import.meta.url), 'utf8');
const workspaces = readFileSync(new URL('../../styles/workspaces.css', import.meta.url), 'utf8');

function token(block, name) {
  const match = new RegExp(`--${name}:\\s*(#[0-9a-f]{6})`, 'i').exec(block);
  if (!match) throw new Error(`Missing --${name}`);
  return match[1];
}

function luminance(hex) {
  const channels = [1, 3, 5].map((index) => Number.parseInt(hex.slice(index, index + 2), 16) / 255)
    .map((value) => value <= .04045 ? value / 12.92 : ((value + .055) / 1.055) ** 2.4);
  return channels[0] * .2126 + channels[1] * .7152 + channels[2] * .0722;
}

function contrast(foreground, background) {
  const first = luminance(foreground); const second = luminance(background);
  return (Math.max(first, second) + .05) / (Math.min(first, second) + .05);
}

describe('dashboard visualization theme', () => {
  it('uses MX reporting tokens instead of the customizable primary accent', () => {
    expect(chart).not.toContain('var(--primary)');
    expect(chart).not.toMatch(/:\s*#[0-9a-f]{3,8}/i);
    const heroRules = [...legacy.matchAll(/\.hero-kicker\s*\{([^}]*)\}/g)].map((match) => match[1]);
    expect(heroRules.length).toBeGreaterThan(0);
    expect(heroRules.every((rule) => !rule.includes('var(--primary)'))).toBe(true);
    expect(workspaces).toMatch(/\.metric-value\s*\{[^}]*var\(--chart-1\)/s);
    expect(workspaces).toMatch(/\.progress-track i\s*\{[^}]*var\(--chart-1\)/s);
  });

  it('defines complete light, dark, and system-dark reporting palettes', () => {
    const chartOneCount = [...tokens.matchAll(/--chart-1:\s*#[0-9a-f]{6}/gi)].length;
    const chartSurfaceCount = [...tokens.matchAll(/--chart-surface:\s*#[0-9a-f]{6}/gi)].length;

    expect(chartOneCount).toBe(3);
    expect(chartSurfaceCount).toBe(3);
    for (let index = 1; index <= 10; index += 1) {
      expect(tokens).toContain(`--chart-${index}:`);
    }
  });
});

describe('collaboration navigation theme', () => {
  it('defines neutral MX colors for light, dark, and system-dark modes', () => {
    expect([...tokens.matchAll(/--collab-bg:\s*#[0-9a-f]{6}/gi)]).toHaveLength(3);
    expect([...tokens.matchAll(/--collab-selected:\s*#[0-9a-f]{6}/gi)]).toHaveLength(3);
    expect([...tokens.matchAll(/--collab-line:\s*#[0-9a-f]{6}/gi)]).toHaveLength(3);
  });

  it('keeps sidebar selection and creation controls independent of the custom accent', () => {
    expect(workspaces).toMatch(/\.conversation-list > button\.active\s*\{[^}]*var\(--collab-selected-border\)[^}]*var\(--collab-selected\)/s);
    expect(workspaces).toMatch(/\.new-conversation-button\s*\{[^}]*var\(--collab-selected-border\)[^}]*var\(--collab-selected\)[^}]*var\(--collab-icon\)/s);
  });

  it('keeps primary and secondary sidebar text readable in both themes', () => {
    const light = /:root\s*\{([^}]*)\}/s.exec(tokens)?.[1] || '';
    const dark = /\[data-theme='dark'\]\s*\{([^}]*)\}/s.exec(tokens)?.[1] || '';

    for (const block of [light, dark]) {
      expect(contrast(token(block, 'collab-text'), token(block, 'collab-bg'))).toBeGreaterThanOrEqual(4.5);
      expect(contrast(token(block, 'collab-muted'), token(block, 'collab-bg'))).toBeGreaterThanOrEqual(4.5);
    }
  });
});
