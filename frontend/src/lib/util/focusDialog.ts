/** Keep keyboard focus in the topmost modal and return it to the opener. */
export function focusDialog(node: HTMLElement) {
  const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  const controls = () => [...node.querySelectorAll<HTMLElement>('button:not([disabled]),a[href],input:not([disabled]):not([type="hidden"]),select:not([disabled]),textarea:not([disabled]),[tabindex="0"]')].filter((item) => item.getClientRects().length);
  queueMicrotask(() => (node.querySelector<HTMLElement>('input:not([readonly])') || controls()[0] || node).focus());
  const handler = (event: KeyboardEvent) => {
    if (event.key !== 'Tab' || !node.contains(document.activeElement)) return;
    const items = controls(); const first = items[0]; const last = items.at(-1);
    if (!first) { event.preventDefault(); node.focus(); }
    else if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
    else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
  };
  node.addEventListener('keydown', handler);
  return { destroy() { node.removeEventListener('keydown', handler); if (previous?.isConnected) previous.focus(); } };
}
