export interface Rect { left: number; top: number; right: number; bottom: number }

/** Position in viewport CSS pixels, never inside a table's overflow box. */
export function floatingPosition(anchor: Rect, width: number, height: number, viewport: { left: number; top: number; width: number; height: number }) {
  const margin = 8; const gap = 4;
  const left = Math.max(viewport.left + margin, Math.min(anchor.right - width, viewport.left + viewport.width - width - margin));
  const below = viewport.top + viewport.height - anchor.bottom - gap - margin;
  const above = anchor.top - viewport.top - gap - margin;
  const top = below >= height || below >= above ? anchor.bottom + gap : anchor.top - height - gap;
  return { left, top: Math.max(viewport.top + margin, Math.min(top, viewport.top + viewport.height - height - margin)) };
}

/** Svelte action: escape scroll clipping and track scrolling/zoom/resize. */
export function floatingMenu(node: HTMLElement, options: { anchor: HTMLElement; close: () => void }) {
  document.body.appendChild(node);
  let frame = 0;
  const position = () => {
    if (!options.anchor.isConnected) { options.close(); return; }
    const anchor = options.anchor.getBoundingClientRect();
    const viewport = window.visualViewport;
    const bounds = { left: viewport?.offsetLeft || 0, top: viewport?.offsetTop || 0, width: viewport?.width || window.innerWidth, height: viewport?.height || window.innerHeight };
    if (anchor.bottom <= bounds.top || anchor.top >= bounds.top + bounds.height || anchor.right <= bounds.left || anchor.left >= bounds.left + bounds.width) { options.close(); return; }
    node.style.maxWidth = `${Math.max(0, bounds.width - 16)}px`;
    node.style.maxHeight = `${Math.max(0, bounds.height - 16)}px`;
    const rect = node.getBoundingClientRect();
    const result = floatingPosition(anchor, rect.width, rect.height, bounds);
    node.style.left = `${result.left}px`; node.style.top = `${result.top}px`;
    node.style.visibility = 'visible';
  };
  const schedule = () => { cancelAnimationFrame(frame); frame = requestAnimationFrame(position); };
  const resize = typeof ResizeObserver === 'function' ? new ResizeObserver(schedule) : null;
  const intersection = typeof IntersectionObserver === 'function' ? new IntersectionObserver((entries) => {
    if (entries.some((entry) => entry.target === options.anchor && !entry.isIntersecting)) options.close();
  }) : null;
  resize?.observe(node); resize?.observe(options.anchor);
  intersection?.observe(options.anchor);
  window.addEventListener('resize', schedule);
  document.addEventListener('scroll', schedule, true);
  window.visualViewport?.addEventListener('resize', schedule);
  window.visualViewport?.addEventListener('scroll', schedule);
  position();
  node.querySelector<HTMLElement>('[role="menuitem"]')?.focus({ preventScroll: true });
  return { update(next: typeof options) {
    options = next;
    resize?.disconnect(); resize?.observe(node); resize?.observe(options.anchor);
    intersection?.disconnect(); intersection?.observe(options.anchor);
    schedule();
  }, destroy() {
    cancelAnimationFrame(frame); resize?.disconnect(); intersection?.disconnect();
    window.removeEventListener('resize', schedule);
    document.removeEventListener('scroll', schedule, true);
    window.visualViewport?.removeEventListener('resize', schedule);
    window.visualViewport?.removeEventListener('scroll', schedule);
    node.remove();
  } };
}
