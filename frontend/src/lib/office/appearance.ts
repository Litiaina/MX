import { apiJson, hasAuthTokens } from '../api/client';
import type { DeploymentResponse, PreferencesResponse } from '../api/domain';
import { DEFAULT_APPEARANCE, usesProductAccent, validAccentColor, applyPrimaryAccent } from '../util/appearance';
import { applyBrandIcon } from '../util/branding';

const KEY = 'mx_office_appearance';
interface OfficeAppearance { theme:'light'|'dark'|'system'; accent:string; fontPercent:number; reducedMotion:boolean; logo:string }
function valid(value: unknown): OfficeAppearance | null {
  if (!value || typeof value !== 'object') return null;
  const item = value as Partial<OfficeAppearance>;
  if (!['light','dark','system'].includes(item.theme || '') || typeof item.accent !== 'string' || (item.accent && !validAccentColor(item.accent)) || typeof item.logo !== 'string' || item.logo.length > 4096 || typeof item.fontPercent !== 'number' || !Number.isFinite(item.fontPercent) || item.fontPercent < 62.5 || item.fontPercent > 200 || typeof item.reducedMotion !== 'boolean') return null;
  return item as OfficeAppearance;
}
export function rememberOfficeAppearance(): void {
  const root = document.documentElement;
  const snapshot = valid({ theme:root.dataset.theme || 'light', accent:root.style.getPropertyValue('--primary').trim(), fontPercent:parseFloat(root.style.fontSize) || 100, reducedMotion:root.dataset.reducedMotion === 'true', logo:document.querySelector<HTMLLinkElement>('link[rel="icon"]')?.href || '' });
  try { if (snapshot) sessionStorage.setItem(KEY, JSON.stringify(snapshot)); } catch { /* Storage restrictions must never stop opening a file. */ }
}
function apply(snapshot: OfficeAppearance): void {
  const root = document.documentElement;
  root.dataset.theme = snapshot.theme; root.dataset.reducedMotion = String(snapshot.reducedMotion);
  root.style.fontSize = `${snapshot.fontPercent}%`;
  applyPrimaryAccent(snapshot.accent, !snapshot.accent);
  applyBrandIcon(snapshot.logo);
}
export function initializeOfficeAppearance(): void {
  let cached:OfficeAppearance|null = null;
  try { cached = valid(JSON.parse(sessionStorage.getItem(KEY) || 'null')); } catch { /* Invalid or unavailable cache: use product defaults. */ }
  let theme:OfficeAppearance['theme'] = 'light';
  try { const hint=sessionStorage.getItem('mx_office_theme');if(hint==='dark'||hint==='system')theme=hint; } catch { /* Optional legacy hint. */ }
  apply(cached || {theme,accent:'',fontPercent:100,reducedMotion:false,logo:''});
  // Refresh independently of document loading. Offline/cached editors must not
  // wait for these requests or lose their appearance when the LAN is down.
  void Promise.allSettled([
    apiJson<DeploymentResponse>('/mx/v1/deployment/config', {maxAttempts:1,timeoutMs:5000}, false),
    hasAuthTokens() ? apiJson<PreferencesResponse>('/mx/v1/account/preferences', {maxAttempts:1,timeoutMs:5000}) : Promise.resolve(null)
  ]).then(([deploymentResult, preferencesResult]) => {
    const deployment = deploymentResult.status === 'fulfilled' ? deploymentResult.value.config : null;
    const preferences = preferencesResult.status === 'fulfilled' ? preferencesResult.value?.preferences : null;
    if (!deployment && !preferences) return;
    const appearance = deployment?.appearance || DEFAULT_APPEARANCE;
    const accent = validAccentColor(preferences?.accent_color) ? preferences.accent_color : appearance.primary_color;
    const preferredTheme=preferences?.theme || (deployment?appearance.default_theme:cached?.theme) || 'light';
    const snapshot:OfficeAppearance = {
      theme:preferredTheme==='dark'||preferredTheme==='system'?preferredTheme:'light',
      accent:usesProductAccent(appearance.preset, appearance.primary_color, preferences?.accent_color) ? '' : accent,
      fontPercent:Math.max(62.5,Math.min(200,(preferences?.font_size_px ?? 16*(preferences?.font_scale_percent ?? 100)/100)/16*100)),
      reducedMotion:preferences?.reduced_motion ?? cached?.reducedMotion ?? false,
      logo:deployment?.branding.logo_url ?? cached?.logo ?? ''
    };
    // If one service failed, preserve the known effective personal appearance.
    if (!preferences && cached) { snapshot.theme=cached.theme;snapshot.accent=cached.accent;snapshot.fontPercent=cached.fontPercent; }
    if (!deployment && !validAccentColor(preferences?.accent_color) && cached) snapshot.accent=cached.accent;
    apply(snapshot);
    try { sessionStorage.setItem(KEY,JSON.stringify(snapshot)); } catch { /* Non-fatal. */ }
  });
}
