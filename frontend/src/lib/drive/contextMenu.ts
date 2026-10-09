/** Portalled, viewport-clamped menu. Listeners exist only while Drive opens it. */
export function driveMenu(node: HTMLElement, options: { x: number; y: number; anchor?: HTMLElement; close: () => void }) {
  const previous = document.activeElement as HTMLElement | null;
  document.body.appendChild(node);
  const position = () => {
    const viewport = window.visualViewport;
    const left = viewport?.offsetLeft || 0, top = viewport?.offsetTop || 0;
    const width = viewport?.width || innerWidth, height = viewport?.height || innerHeight;
    node.style.maxHeight = `${height-16}px`; node.style.maxWidth = `${width-16}px`;
    const rect = node.getBoundingClientRect();
    const anchor = options.anchor?.getBoundingClientRect();
    node.style.left = `${Math.max(left+8,Math.min(anchor?.left ?? options.x,left+width-rect.width-8))}px`;
    node.style.top = `${Math.max(top+8,Math.min(anchor?.bottom ?? options.y,top+height-rect.height-8))}px`;
    node.style.visibility = 'visible';
  };
  const outside = (event: PointerEvent) => { if (!node.contains(event.target as Node)) options.close(); };
  // Selecting a row can wrap the mobile toolbar and trigger scroll anchoring.
  // Do not dismiss the menu because of that programmatic/layout scroll.
  const scroll = () => { if(options.anchor) position(); };
  const userScroll = (event: Event) => { if (!node.contains(event.target as Node)) options.close(); };
  const key = (event: KeyboardEvent) => {
    const items = [...node.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)')];
    const current = items.indexOf(document.activeElement as HTMLButtonElement);
    if (['ArrowDown','ArrowUp','Home','End'].includes(event.key)) {
      event.preventDefault(); const index=event.key==='Home'?0:event.key==='End'?items.length-1:(current+(event.key==='ArrowDown'?1:-1)+items.length)%items.length;
      items[index]?.focus();
    } else if (event.key === 'Escape' || event.key === 'Tab') { event.preventDefault(); options.close(); }
  };
  const observer = new ResizeObserver(position); observer.observe(node);
  document.addEventListener('pointerdown',outside,true); document.addEventListener('scroll',scroll,true); document.addEventListener('wheel',userScroll,{capture:true,passive:true}); document.addEventListener('touchmove',userScroll,{capture:true,passive:true}); node.addEventListener('keydown',key); window.addEventListener('resize',options.close); window.visualViewport?.addEventListener('resize',position);
  position(); node.querySelector<HTMLElement>('[role="menuitem"]:not(:disabled)')?.focus({preventScroll:true});
  return { destroy() { observer.disconnect(); document.removeEventListener('pointerdown',outside,true); document.removeEventListener('scroll',scroll,true); document.removeEventListener('wheel',userScroll,true); document.removeEventListener('touchmove',userScroll,true); node.removeEventListener('keydown',key); window.removeEventListener('resize',options.close); window.visualViewport?.removeEventListener('resize',position); node.remove(); queueMicrotask(()=>{if(document.activeElement === document.body && previous?.isConnected) previous.focus({preventScroll:true});}); } };
}
