// Only a complete primary-pointer gesture on the backdrop dismisses a dialog.
// Dragging a selection from inside the dialog must not close it on pointer-up.
export function dismissBackdrop(node: HTMLElement, close: () => void) {
  let pointer: number | null = null;
  const down = (event: PointerEvent) => {
    pointer = event.target === node && event.button === 0 ? event.pointerId : null;
    // Keep the dialog's current focus if dismissal asks for confirmation.
    // Otherwise clicking a non-focusable backdrop moves focus to the page body.
    if (pointer !== null) event.preventDefault();
  };
  const up = (event: PointerEvent) => {
    const dismiss = pointer === event.pointerId && event.target === node;
    pointer = null;
    if (dismiss) close();
  };
  const cancel = () => { pointer = null; };
  node.addEventListener('pointerdown', down);
  node.addEventListener('pointerup', up);
  node.addEventListener('pointercancel', cancel);
  return { update(next: () => void) { close = next; }, destroy() {
    node.removeEventListener('pointerdown', down);
    node.removeEventListener('pointerup', up);
    node.removeEventListener('pointercancel', cancel);
  } };
}
