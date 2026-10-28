export const DEFAULT_ICON = '/images/system-icon.png';
let generation = 0;

// Branding paths are relative to MX, never to /office/ or a document URL.
export function brandIconUrl(value: unknown, origin = location.origin): string {
  if (typeof value !== 'string' || !value.trim()) return new URL(DEFAULT_ICON, origin).href;
  try {
    const url = new URL(value.trim(), `${origin}/`);
    if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password) throw new Error('Unsafe logo URL');
    return url.href;
  } catch { return new URL(DEFAULT_ICON, origin).href; }
}

export function applyBrandIcon(value: unknown): void {
  const current = ++generation;
  const href = brandIconUrl(value);
  let link = document.querySelector<HTMLLinkElement>('link[rel="icon"]');
  if (!link) { link = document.createElement('link'); link.rel = 'icon'; document.head.append(link); }
  // Do not retain a fixed SVG MIME type when an uploaded PNG/JPEG is selected.
  link.removeAttribute('type'); link.removeAttribute('sizes'); link.href = href;
  if (href === brandIconUrl(DEFAULT_ICON)) return;
  // link.onerror is not consistently delivered for favicons. A regular image
  // request shares the browser cache and gives us a dependable fallback signal.
  const probe = new Image();
  probe.onerror = () => { if (current === generation) link!.href = brandIconUrl(DEFAULT_ICON); };
  probe.src = href;
}
