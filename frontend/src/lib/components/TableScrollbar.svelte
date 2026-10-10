<script lang="ts">
  import { tick } from 'svelte';
  import { tableScrollPosition, clampTableScroll, type ScrollBounds } from '../util/tableScroll';

  let { target, enabled = true }: { target?: HTMLDivElement; enabled?: boolean } = $props();
  let bar: HTMLDivElement;
  let position = $state<ReturnType<typeof tableScrollPosition>>(null);
  let offset = $state(0); let maximum = $state(0);
  const thumbWidth = $derived(position ? Math.min(position.width,Math.max(28,position.width*position.width/position.contentWidth)) : 0);
  const thumbTravel = $derived(Math.max(0,(position?.width || 0)-thumbWidth));
  const thumbLeft = $derived((position?.left || 0)+(maximum?offset/maximum*thumbTravel:0));
  let drag: { pointer: number; x: number; offset: number } | null = null;

  $effect(() => {
    const table = target;
    if (!table || !enabled) { position=null; return; }
    let frame = 0;
    let active = true;
    const measure = () => {
      frame=0;
      const viewport=window.visualViewport;
      const bounds: ScrollBounds={left:viewport?.offsetLeft || 0, top:viewport?.offsetTop || 0, right:(viewport?.offsetLeft || 0)+(viewport?.width || innerWidth), bottom:(viewport?.offsetTop || 0)+(viewport?.height || innerHeight)};
      // Respect a vertically scrolling workspace as well as the document. The
      // floating control must never escape a clipped/nested table viewport.
      for(let parent=table.parentElement;parent&&parent!==document.documentElement;parent=parent.parentElement){
        const style=getComputedStyle(parent),rect=parent.getBoundingClientRect();
        if(/auto|scroll|hidden|clip/.test(style.overflowX)){bounds.left=Math.max(bounds.left,rect.left+parent.clientLeft);bounds.right=Math.min(bounds.right,rect.left+parent.clientLeft+parent.clientWidth);}
        if(/auto|scroll|hidden|clip/.test(style.overflowY)){bounds.top=Math.max(bounds.top,rect.top+parent.clientTop);bounds.bottom=Math.min(bounds.bottom,rect.top+parent.clientTop+parent.clientHeight);}
      }
      const next=tableScrollPosition(table.getBoundingClientRect(),bounds,table.scrollWidth,table.clientWidth);
      maximum=Math.max(0,table.scrollWidth-table.clientWidth);offset=clampTableScroll(table.scrollLeft,table.scrollWidth,table.clientWidth);
      // Avoid component updates for every vertical scroll when geometry is
      // unchanged. There is no polling and no record/API activity here.
      if(JSON.stringify(next)!==JSON.stringify(position))position=next;
      // Wait for the newly shown/resized scroll track before syncing; an old
      // hidden track has no scroll range and would clamp the requested offset.
      void tick().then(()=>{if(active && position && bar && Math.abs(bar.scrollLeft-table.scrollLeft)>.5)bar.scrollLeft=table.scrollLeft;});
    };
    const schedule = () => { if(!frame)frame=requestAnimationFrame(measure); };
    const resize = new ResizeObserver(schedule);
    resize.observe(table);if(table.firstElementChild)resize.observe(table.firstElementChild);
    window.addEventListener('resize',schedule);
    document.addEventListener('scroll',schedule,{capture:true,passive:true});
    window.visualViewport?.addEventListener('resize',schedule);
    window.visualViewport?.addEventListener('scroll',schedule);
    schedule();
    return () => { active=false;cancelAnimationFrame(frame);resize.disconnect();window.removeEventListener('resize',schedule);document.removeEventListener('scroll',schedule,true);window.visualViewport?.removeEventListener('resize',schedule);window.visualViewport?.removeEventListener('scroll',schedule); };
  });

  function scrollTable() {
    if(position && target && Math.abs(target.scrollLeft-bar.scrollLeft)>.5)target.scrollLeft=clampTableScroll(bar.scrollLeft,target.scrollWidth,target.clientWidth);
  }
  function moveTo(value: number) {
    if(!target)return;
    target.scrollLeft=clampTableScroll(value,target.scrollWidth,target.clientWidth);
    bar.scrollLeft=target.scrollLeft;offset=target.scrollLeft;
  }
  function startDrag(event: PointerEvent) {
    if(!position || !target || event.button!==0)return;
    event.preventDefault();bar.focus({preventScroll:true});
    if(event.clientX<thumbLeft || event.clientX>thumbLeft+thumbWidth)moveTo((event.clientX-position.left-thumbWidth/2)/Math.max(1,thumbTravel)*maximum);
    drag={pointer:event.pointerId,x:event.clientX,offset:target.scrollLeft};bar.setPointerCapture(event.pointerId);
  }
  function moveDrag(event: PointerEvent) {
    if(drag?.pointer===event.pointerId)moveTo(drag.offset+(event.clientX-drag.x)/Math.max(1,thumbTravel)*maximum);
  }
  function endDrag(event: PointerEvent) {
    if(drag?.pointer!==event.pointerId)return;
    drag=null;if(bar.hasPointerCapture(event.pointerId))bar.releasePointerCapture(event.pointerId);
  }
  function keyboard(event: KeyboardEvent) {
    if(!target)return;
    const step=event.key==='ArrowLeft'?-64:event.key==='ArrowRight'?64:event.key==='PageUp'?-target.clientWidth*.8:event.key==='PageDown'?target.clientWidth*.8:null;
    if(step!==null || ['Home','End'].includes(event.key)){
      event.preventDefault();
      moveTo(event.key==='Home'?0:event.key==='End'?target.scrollWidth:target.scrollLeft+(step || 0));
    }
  }
</script>

<div class="table-scrollbar" bind:this={bar} hidden={!position} style:left={`${position?.left || 0}px`} style:top={`${position?.top || 0}px`} style:width={`${position?.width || 0}px`} role="scrollbar" aria-label="Scroll record columns" aria-controls={target?.id} aria-orientation="horizontal" aria-valuemin="0" aria-valuemax={maximum} aria-valuenow={offset} aria-valuetext={`${Math.round(maximum?offset/maximum*100:0)}% across record columns`} tabindex="0" onscroll={scrollTable} onkeydown={keyboard} onpointerdown={startDrag} onpointermove={moveDrag} onpointerup={endDrag} onpointercancel={endDrag} onlostpointercapture={()=>drag=null}>
  <div class="scroll-width" style:width={`${position?.contentWidth || 0}px`}></div>
  <div class="scroll-track" style:left={`${position?.left || 0}px`} style:top={`${(position?.top || 0)+4}px`} style:width={`${position?.width || 0}px`}></div>
  <div class="scroll-thumb" style:left={`${thumbLeft}px`} style:top={`${(position?.top || 0)+4}px`} style:width={`${thumbWidth}px`}></div>
</div>

<style>
  .table-scrollbar{position:fixed;z-index:9;height:14px;overflow-x:auto;overflow-y:hidden;background:var(--surface);border-radius:3px;scrollbar-width:none;overscroll-behavior-inline:contain;touch-action:none;cursor:default;user-select:none;}
  .table-scrollbar[hidden]{display:none;}
  .scroll-width{height:1px;pointer-events:none;}
  .table-scrollbar::-webkit-scrollbar{display:none;}
  .scroll-track,.scroll-thumb{position:fixed;height:6px;pointer-events:none;border-radius:999px;}
  .scroll-track{background:var(--surface-2);}.scroll-thumb{background:var(--line-strong);}
  .table-scrollbar:hover .scroll-thumb{background:var(--muted);}
  .table-scrollbar:focus-visible{outline:2px solid var(--primary);outline-offset:-2px;}
</style>
