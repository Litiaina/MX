<script lang="ts">
  import { onMount, onDestroy, tick, untrack, getContext } from 'svelte';
  import { writable, type Writable } from 'svelte/store';
  import Bold from '@lucide/svelte/icons/bold';
  import Send from '@lucide/svelte/icons/send';
  import SendRecordDialog from './SendRecordDialog.svelte';
  import type { ModuleReadingMap } from '../api/domain';
  import { columnPresentation, columnTone, fieldIsBold, moduleReadingContext } from '../util/readingPreferences';
  import ModuleReadingDialog from './ModuleReadingDialog.svelte';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import ChevronUp from '@lucide/svelte/icons/chevron-up';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import Eye from '@lucide/svelte/icons/eye';
  import Pencil from '@lucide/svelte/icons/pencil';
  import Search from '@lucide/svelte/icons/search';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import Upload from '@lucide/svelte/icons/upload';
  import History from '@lucide/svelte/icons/history';
  import { ApiError } from '../api/client';
  import type { LiveMessage } from '../live/client';
  import { requestConfirmation } from '../confirmation';
  import type { ColumnSummary, JsonValue, ModulePermission, MxRecord, SchemaResponse } from '../api/domain';
  import { summaryNames, summaryValue } from '../util/columnSummaries';
  import type { Session } from '../api/types';
  import { deleteRecord, getRecord, listRecords, loadModuleSchema, loadSchema } from '../api/workspace';
  import RecordEditor from './RecordEditor.svelte';
  import type {RecordSection} from '../api/moduleViews';
  import ModuleWorkQueue from './ModuleWorkQueue.svelte';
  import ModuleBooks from './ModuleBooks.svelte';
  import ImportRecords from './ImportRecords.svelte';
  import {fieldState,optionLabel} from '../util/formRules';
  import TableScrollbar from './TableScrollbar.svelte';
  import { floatingMenu } from '../util/floating';
  import { windowPieces, mergeWindow, windowAnchor, type RecordViewPosition } from '../util/recordWindow';
  import { recordColumnWidths, snapshotRecordColumns, validColumnSnapshot, type RecordColumnSnapshot } from '../util/recordColumns';

  let { session, accessLevel, moduleUid = '', modulePermission = null, openRecordUid = '', openRequestRevision = 0, focusLinkedAttachments = false, revision = 0, liveMessage = null, recordSingular = 'Record', recordPlural = 'Records',recordSections=[] }: { session: Session; accessLevel: number; moduleUid?: string; modulePermission?: ModulePermission | null; openRecordUid?: string; openRequestRevision?: number; focusLinkedAttachments?: boolean; revision?: number; liveMessage?: LiveMessage | null; recordSingular?: string; recordPlural?: string;recordSections?:RecordSection[] } = $props();
  let schema = $state<SchemaResponse | null>(null);
  const readingPreferences = getContext<Writable<ModuleReadingMap>>(moduleReadingContext) || writable<ModuleReadingMap>({});
  let readingOpen = $state(false);
  let sendRecord = $state<MxRecord | null>(null);
  let importer = $state<'new' | 'history' | null>(null);
  let importMenuOpen = $state(false);
  let importMenuAnchor = $state<HTMLButtonElement | null>(null);
  let importFocusFrame = 0;
  let rows = $state<MxRecord[]>([]);
  let summaries = $state<Record<string, ColumnSummary>>({});
  let page = $state(1); let pageInput = $state(1); let pages = $state(0); let total = $state(0); let pageSize = $state(50);
  let continuous = $state(false);
  let viewportWidth = $state(0);
  let measuredColumns = $state<RecordColumnSnapshot | null>(null);
  let windowBefore = $state(false); let windowAfter = $state(false);
  let edgeLoading = $state(false); let edgeError = $state(''); let edgeDirection = $state<'before'|'after'>('after');
  let updatesAvailable = $state(false); let baseIndex = $state(0);
  let viewportTop = $state(0); let viewportHeight = $state(600); let rowHeights = $state<Record<string,number>>({});
  let mobileRows = $state(false); let focusedRow = $state('');
  let cursors = new Map<string,string>();
  let requestController: AbortController | undefined;
  let edgeController: AbortController | undefined;
  let restorePosition: RecordViewPosition | undefined;
  let lastAnchor:RecordViewPosition|undefined;let restoringAnchor:RecordViewPosition|undefined;let restoreEpoch=0;
  let anchorScrollTop=0;let anchorScrollLeft=0;
  let layoutAnchor=false;
  let scrollFrame = 0; let positionTimer: ReturnType<typeof setTimeout> | undefined;
  let measureFrame=0;const pendingMeasurements=new Map<string,number>();
  let appliedQuery = {search:'',sortBy:'',sortDir:'asc',match:'contains' as 'contains'|'prefix'|'exact',filters:{} as Record<string,string>,attachments:'' as 'with'|'without'|''};
  const positionKey = $derived(`mx_record_position_v1:${session.uid}:${moduleUid || 'default'}`);
  const preferenceKey = $derived(`mx_record_view_v1:${session.uid}:${moduleUid || 'default'}`);
  let search = $state(''); let sortBy = $state(''); let sortDir = $state('asc');
  let matchMode = $state<'contains' | 'prefix' | 'exact'>('contains');
  let attachmentMode = $state<'with' | 'without' | ''>('');
  let filters = $state<Record<string, string>>({});
  let selectedKeys = $state<string[]>([]);
  let loading = $state(true); let error = $state('');
  let deletingUid = $state('');
  let rowMenuUid = $state('');
  let rowMenuAnchor = $state<HTMLButtonElement | null>(null);
  let tableViewport = $state<HTMLDivElement>();
  const menuRow = $derived(rows.find((row) => row.uid === rowMenuUid));
  let editor = $state<MxRecord | null | undefined>(undefined);
  let editorAnchor:RecordViewPosition|undefined;
  let editorChanges=$state<Record<string,JsonValue>|null>(null);
  let editorWritable = $state(false);
  let recordOpenSequence=0;
  let destroyed=false;let pageLeaving=false;
  onDestroy(()=>{ destroyed=true;refreshSequence++;recordOpenSequence++;rememberPosition(); requestController?.abort(); edgeController?.abort(); cancelAnimationFrame(importFocusFrame); cancelAnimationFrame(scrollFrame);cancelAnimationFrame(measureFrame); clearTimeout(positionTimer);window.clearTimeout(searchTimer); });
  let focusAttachments = $state(false);
  let lastRevision = $state(0);
  let columnOverrides = $state<{ shown: string[]; hidden: string[] }>({ shown: [], hidden: [] });
  let legacyColumns: string[] | null = null;
  let searchTimer: number | undefined;
  let refreshSequence = 0;
  // A response can have been read before a concurrent deletion committed.
  // Keep deletion barriers until an explicit restore, not just another fetch.
  const deletedRecordUids = new Set<string>();
  const lifecycleSequences = new Map<string, number>();
  const deletionGenerations = new Map<string, number>();
  let deletionGeneration = 0;
  let openedDeepLink = '';
  let openedRequestRevision = -1;
  const canCreate = $derived(modulePermission ? modulePermission.can_create : accessLevel <= 2);
  const canUpdate = $derived(modulePermission ? modulePermission.can_update : accessLevel <= 2);
  const canDelete = $derived(modulePermission ? modulePermission.can_delete : accessLevel <= 1);
  const canAttachments = $derived(modulePermission ? modulePermission.can_attachments : true);
  const eligibleFields = $derived(schema?.fields.filter((field) => field.active && (canAttachments || field.field_type !== 'attachments')).sort((a, b) => a.position - b.position) || []);
  const visibleFields = $derived(eligibleFields.filter((field) => selectedKeys.includes(field.key)));
  const summaryFields = $derived(visibleFields.filter(field => !!field.config.table_summary && field.config.table_summary !== 'none'));
  const searchableFields = $derived(schema?.fields.filter((field) => field.active && field.searchable && field.field_type !== 'attachments').sort((a, b) => a.position - b.position) || []);
  const pinnedRows = $derived([focusedRow, rowMenuUid].filter(Boolean));
  const columnWidths=$derived(recordColumnWidths(visibleFields,viewportWidth,measuredColumns));
  const fixedColumns=$derived(!mobileRows&&(continuous||validColumnSnapshot(measuredColumns,visibleFields,viewportWidth)));
  const tableWidth=$derived(columnWidths.reduce((sum,width)=>sum+width,0));
  const renderPieces = $derived(continuous ? windowPieces(rows.map(row=>row.uid),rowHeights,viewportTop,viewportHeight,mobileRows?240:50,pinnedRows)
    : rows.map((row,index)=>({kind:'row' as const,index,key:row.uid})));

  const columnStorageKey = $derived(`mx_record_columns_v3_${moduleUid || 'default'}`);
  onMount(() => {
    try {
      const preferences=JSON.parse(localStorage.getItem(preferenceKey)||'null');
      if(preferences){continuous=preferences.continuous===true;pageSize=[25,50,100,256].includes(preferences.batch)?preferences.batch:50;measuredColumns=preferences.columns||null;}
      const saved=JSON.parse(sessionStorage.getItem(positionKey)||'null') as RecordViewPosition|null;
      if(saved && typeof saved.uid==='string' && Number.isFinite(saved.offset)){
        restorePosition=saved; search=saved.search;sortBy=saved.sortBy;sortDir=saved.sortDir;matchMode=saved.match;
        filters=saved.filters;attachmentMode=saved.attachments;page=saved.page||1;
      }
    }catch{/* Storage can be disabled. Browsing remains available. */}
    try { const saved = JSON.parse(localStorage.getItem(columnStorageKey) || localStorage.getItem('mx_record_columns_v3') || 'null'); if (saved && Array.isArray(saved.shown) && Array.isArray(saved.hidden)) columnOverrides = { shown: saved.shown.filter((key: unknown): key is string => typeof key === 'string'), hidden: saved.hidden.filter((key: unknown): key is string => typeof key === 'string') }; else { const old = JSON.parse(localStorage.getItem('mx_record_columns_v2') || 'null'); if (Array.isArray(old)) legacyColumns = old.filter((key): key is string => typeof key === 'string'); } } catch { /* Use schema defaults. */ }
    const closeRowMenu = (event: PointerEvent) => { const target = event.target instanceof Element ? event.target : null; if (!target?.closest('[data-record-actions]')) rowMenuUid = ''; if (!target?.closest('[data-module-import-actions]')) importMenuOpen = false; };
    const closeImportMenuOnFocus = (event: FocusEvent) => { if (importMenuOpen && !(event.target instanceof Element && event.target.closest('[data-module-import-actions]'))) importMenuOpen = false; };
    const closeRowMenuWithKeyboard = (event: KeyboardEvent) => { if (event.key === 'Escape' && rowMenuUid) { event.preventDefault(); closeRowMenuAndFocus(); } if (event.key === 'Escape' && importMenuOpen) { event.preventDefault(); closeImportMenuAndFocus(); } };
    document.addEventListener('pointerdown', closeRowMenu);
    document.addEventListener('keydown', closeRowMenuWithKeyboard);
    document.addEventListener('focusin', closeImportMenuOnFocus);
    window.addEventListener('pagehide',rememberPosition);
    const leaving=(event:Event)=>{pageLeaving=true;requestController?.abort();edgeController?.abort();window.clearTimeout(searchTimer);
      if(event.type==='beforeunload')queueMicrotask(()=>{if(!destroyed&&event.defaultPrevented)pageLeaving=false;});
    };
    const restored=()=>{if(!pageLeaving||destroyed)return;pageLeaving=false;void refresh();};
    window.addEventListener('beforeunload',leaving);window.addEventListener('pagehide',leaving);window.addEventListener('pageshow',restored);
    void refresh(true);
    return () => { window.clearTimeout(searchTimer);window.removeEventListener('pagehide',rememberPosition);window.removeEventListener('beforeunload',leaving);window.removeEventListener('pagehide',leaving);window.removeEventListener('pageshow',restored); document.removeEventListener('pointerdown', closeRowMenu); document.removeEventListener('keydown', closeRowMenuWithKeyboard); document.removeEventListener('focusin', closeImportMenuOnFocus); };
  });
  $effect(() => {
    const message = liveMessage; const selected = moduleUid || 'mx-default-records';
    untrack(() => {
      if (message?.type === 'sync.required') {
        // Live sequence numbers restart with the server process. A new
        // connection must not reject its lifecycle events using the old epoch.
        lifecycleSequences.clear();
        void reconcileDeletedRecords(); return;
      }
      if (!message?.payload || typeof message.payload !== 'object') return;
      if (!['record.deleted', 'record.restored'].includes(message.type)) return;
      const payload = message.payload as Record<string, unknown>;
      if (payload.module_uid && payload.module_uid !== selected) return;
      const uid = String(payload.record_uid || ''); if (!uid) return;
      const sequence = Number(message.sequence || 0);
      if (sequence && sequence <= (lifecycleSequences.get(uid) || 0)) return;
      if (sequence) lifecycleSequences.set(uid, sequence);
      if (message.type === 'record.deleted') forgetDeletedRecord(uid);
      else { deletedRecordUids.delete(uid); deletionGenerations.delete(uid); }
    });
  });
  $effect(() => { if (revision !== lastRevision && schema) { lastRevision = revision; const accessOrSchema=liveMessage?.type.startsWith('schema.')||liveMessage?.type.startsWith('module.');if(continuous&&!accessOrSchema)updatesAvailable=true;else void refresh(!!accessOrSchema); } });
  $effect(() => {
    if(!openRecordUid){openedDeepLink='';openedRequestRevision=-1;return;}
    if(schema&&(openRecordUid!==openedDeepLink||openRequestRevision!==openedRequestRevision)){
      openedDeepLink=openRecordUid;openedRequestRevision=openRequestRevision;
      void openLinkedRecord(openRecordUid);
    }
  });

  function closeRowMenuAndFocus() { rowMenuUid = ''; rowMenuAnchor?.focus({ preventScroll: true }); }
  function toggleRowMenu(event: MouseEvent, uid: string) {
    event.stopPropagation();
    importMenuOpen = false;
    rowMenuAnchor = event.currentTarget as HTMLButtonElement;
    rowMenuUid = rowMenuUid === uid ? '' : uid;
  }
  function menuKeydown(event: KeyboardEvent) {
    if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
      event.preventDefault();
      const items = [...(event.currentTarget as HTMLElement).querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not([disabled])')];
      const index = items.indexOf(document.activeElement as HTMLButtonElement);
      const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : (index + (event.key === 'ArrowUp' ? -1 : 1) + items.length) % items.length;
      items[next]?.focus();
    } else if (event.key === 'Tab') closeRowMenuAndFocus();
  }

  function closeImportMenuAndFocus() {
    cancelAnimationFrame(importFocusFrame);
    importMenuOpen = false;
    importMenuAnchor?.focus({ preventScroll: true });
  }
  async function openImportMenu(focusLast = false) {
    rowMenuUid = '';
    importMenuOpen = true;
    await tick();
    cancelAnimationFrame(importFocusFrame);
    // Reopened portals may not admit focus until layout/paint finishes (Firefox/WebKit).
    // Retry only a bounded number of frames, and stop on close or user navigation.
    const focusItem = (remaining: number) => {
      importFocusFrame = 0;
      if (!importMenuOpen) return;
      const items = document.getElementById('module-import-menu')?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]');
      const target = items?.[focusLast ? items.length - 1 : 0];
      target?.focus();
      if (target !== document.activeElement && remaining > 0) importFocusFrame = requestAnimationFrame(() => focusItem(remaining - 1));
    };
    importFocusFrame = requestAnimationFrame(() => focusItem(8));
  }
  function toggleImportMenu(event: MouseEvent) {
    importMenuAnchor = event.currentTarget as HTMLButtonElement;
    if (importMenuOpen) closeImportMenuAndFocus();
    else void openImportMenu();
  }
  function importTriggerKeydown(event: KeyboardEvent) {
    if (event.key === 'Tab') { importMenuOpen = false; cancelAnimationFrame(importFocusFrame); return; }
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
    event.preventDefault();
    importMenuAnchor = event.currentTarget as HTMLButtonElement;
    void openImportMenu(event.key === 'ArrowUp');
  }
  function importMenuKeydown(event: KeyboardEvent) {
    // Do not let deferred opening focus overwrite the user's next navigation key.
    cancelAnimationFrame(importFocusFrame);
    if (event.key === 'Tab') { closeImportMenuAndFocus(); return; }
    if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    const items = Array.from((event.currentTarget as HTMLElement).querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)'));
    const active = items.indexOf(document.activeElement as HTMLButtonElement);
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : active < 0 ? (event.key === 'ArrowUp' ? items.length - 1 : 0) : (active + (event.key === 'ArrowUp' ? -1 : 1) + items.length) % items.length;
    items[next]?.focus();
  }
  function importMenuKeyboard(node: HTMLElement) {
    // Native listener keeps keyboard ownership local when this menu is portalled.
    node.addEventListener('keydown', importMenuKeydown);
    return { destroy() { node.removeEventListener('keydown', importMenuKeydown); } };
  }
  function openImporter(mode: 'new' | 'history') { importMenuOpen = false; importer = mode; }

  async function openLinkedRecord(uid: string) {
    const sequence=++recordOpenSequence;
    editorAnchor=currentAnchor();
    rowMenuUid = ''; error = '';
    try { const record = await getRecord(uid, moduleUid || undefined); if (deletedRecordUids.has(uid) || sequence!==recordOpenSequence) return; editor = record; editorWritable = false; focusAttachments = focusLinkedAttachments; }
    catch (reason) { if(sequence===recordOpenSequence)error = reason instanceof Error ? reason.message : `The linked ${recordSingular.toLowerCase()} could not be opened.`; }
  }

  function queryOptions() { return { page, limit:pageSize, q:search.trim(), match:matchMode,
    filters:Object.fromEntries(Object.entries(filters).filter(([,value])=>value.trim())), attachments:attachmentMode,sort_by:sortBy||undefined,sort_dir:sortDir }; }
  function currentAnchor(): RecordViewPosition | undefined {
    if(restoringAnchor)return restoringAnchor;
    if(!tableViewport?.isConnected || !rows.length)return lastAnchor;
    const bounds=tableViewport.getBoundingClientRect();
    if(!bounds.height)return lastAnchor;
    // Layout corrections may be fractional even when a browser rounds scrollTop
    // to whole pixels. Keep the intended offset through all measurement passes,
    // rather than accumulating that rounding (or recapturing shifted content).
    // Any actual wheel/drag or horizontal movement releases it immediately.
    if(layoutAnchor&&lastAnchor&&Math.abs(tableViewport.scrollTop-anchorScrollTop)<.5&&Math.abs(tableViewport.scrollLeft-anchorScrollLeft)<.5&&rows.some(row=>row.uid===lastAnchor!.uid))return lastAnchor;
    layoutAnchor=false;
    const header=tableViewport.querySelector('thead')?.getBoundingClientRect().height||0;
    const visibleTop=continuous?bounds.top+header:Math.max(bounds.top+header,document.querySelector('.workspace-topbar')?.getBoundingClientRect().bottom||0);
    const element=Array.from(tableViewport.querySelectorAll<HTMLElement>('[data-record-uid]')).find(node=>{const rect=node.getBoundingClientRect();return rect.bottom>visibleTop+1&&rect.top<Math.min(bounds.bottom,innerHeight);});
    const fallback=windowAnchor(rows.map(row=>row.uid),rowHeights,tableViewport.scrollTop,mobileRows?240:50);
    const uid=element?.dataset.recordUid||fallback?.uid;
    if(!uid)return lastAnchor;
    anchorScrollTop=tableViewport.scrollTop;anchorScrollLeft=tableViewport.scrollLeft;
    return lastAnchor={uid,cursor:cursors.get(uid),offset:element?element.getBoundingClientRect().top-bounds.top:header+fallback!.offset,horizontal:tableViewport.scrollLeft,
      index:(continuous?baseIndex:(page-1)*pageSize)+Math.max(0,rows.findIndex(row=>row.uid===uid)),...appliedQuery,page};
  }
  function measurementAnchor() {
    // New overscan rows can expand above the visible record before their
    // ResizeObserver fires. Preserve its pre-layout position, unless the user
    // has actually moved the scrollbars since that position was captured.
    if(restoringAnchor)return restoringAnchor;
    if(lastAnchor&&tableViewport&&Math.abs(tableViewport.scrollTop-anchorScrollTop)<1&&Math.abs(tableViewport.scrollLeft-anchorScrollLeft)<1)return lastAnchor;
    return currentAnchor();
  }
  function rememberPosition() { const anchor=currentAnchor();if(anchor)try{sessionStorage.setItem(positionKey,JSON.stringify(anchor));}catch{/* Best effort view state; never record data. */} }
  async function restoreAnchor(anchor?: RecordViewPosition) {
    if(!tableViewport || !anchor)return;
    restoringAnchor=anchor;const epoch=++restoreEpoch;
    // Render the anchor first, even when it lies outside the previous virtual range.
    let offset=0;const index=rows.findIndex(row=>row.uid===anchor.uid);
    if(index>=0){for(let i=0;i<index;i++)offset+=rowHeights[rows[i].uid]||(mobileRows?240:50);viewportTop=offset;}
    await tick();
    if(epoch!==restoreEpoch||!tableViewport)return;
    const node=Array.from(tableViewport.querySelectorAll<HTMLElement>('[data-record-uid]')).find(node=>node.dataset.recordUid===anchor.uid);
    tableViewport.scrollLeft=anchor.horizontal;
    if(node)tableViewport.scrollTop+=node.getBoundingClientRect().top-tableViewport.getBoundingClientRect().top-anchor.offset;
    viewportTop=tableViewport.scrollTop;
    // Updating viewportTop can replace an estimated spacer with real rows.
    // Let that keyed range commit before recording a successfully restored
    // position; otherwise a tall overscan row shifts the saved anchor again.
    await tick();
    if(epoch!==restoreEpoch||!tableViewport)return;
    const settledNode=Array.from(tableViewport.querySelectorAll<HTMLElement>('[data-record-uid]')).find(node=>node.dataset.recordUid===anchor.uid);
    if(settledNode)tableViewport.scrollTop+=settledNode.getBoundingClientRect().top-tableViewport.getBoundingClientRect().top-anchor.offset;
    viewportTop=tableViewport.scrollTop;
    // Do not leave a stale restoration active for another animation frame:
    // the next wheel/drag must be free to choose its own destination.
    restoringAnchor=undefined;lastAnchor=anchor;
    anchorScrollTop=tableViewport.scrollTop;anchorScrollLeft=tableViewport.scrollLeft;
    layoutAnchor=true;
  }
  function measureRow(node: HTMLElement, uid: string) {
    $effect(()=>{
    if(!continuous)return;
    const observer=new ResizeObserver(()=>{
      const height=node.getBoundingClientRect().height+(mobileRows?8.8:0);
      if(height>0 && Math.abs((rowHeights[uid]||0)-height)>1){pendingMeasurements.set(uid,height);
        // Mutating the virtual DOM inside ResizeObserver causes observer loops
        // in WebKit. Batch all row measurements in the next animation frame.
        if(!measureFrame)measureFrame=requestAnimationFrame(()=>{measureFrame=0;if(!pendingMeasurements.size)return;const anchor=measurementAnchor();rowHeights={...rowHeights,...Object.fromEntries(pendingMeasurements)};pendingMeasurements.clear();void restoreAnchor(anchor);});
      }
    });observer.observe(node);return()=>{observer.disconnect();pendingMeasurements.delete(uid);};
    });
  }
  function observeViewport(node: HTMLDivElement) {
    let frame=0;const observer=new ResizeObserver(()=>{cancelAnimationFrame(frame);frame=requestAnimationFrame(()=>{const anchor=measurementAnchor();viewportHeight=node.clientHeight;viewportWidth=node.getBoundingClientRect().width-node.clientLeft*2;mobileRows=innerWidth<=820;if(continuous)void restoreAnchor(anchor);});});observer.observe(node);
    return {destroy(){observer.disconnect();cancelAnimationFrame(frame);}};
  }
  function scrolled() {
    // Capture the user's new destination before a queued measurement can use an
    // old anchor. Fast scrollbar drags may currently point into a virtual gap.
    if(lastAnchor&&tableViewport&&Math.abs(tableViewport.scrollTop-anchorScrollTop)<.5){lastAnchor.horizontal=tableViewport.scrollLeft;anchorScrollLeft=tableViewport.scrollLeft;}
    else currentAnchor();
    if(scrollFrame)return;
    scrollFrame=requestAnimationFrame(()=>{scrollFrame=0;if(!tableViewport)return;viewportTop=tableViewport.scrollTop;viewportHeight=tableViewport.clientHeight;
      clearTimeout(positionTimer);positionTimer=setTimeout(rememberPosition,200);
      if(!continuous||loading||edgeLoading||edgeError)return;
      const threshold=Math.max(150,viewportHeight*.6);
      if(tableViewport.scrollTop+viewportHeight>=tableViewport.scrollHeight-threshold && windowAfter)void loadEdge('after');
      else if(tableViewport.scrollTop<threshold&&windowBefore)void loadEdge('before');
    });
  }
  async function loadEdge(direction: 'before'|'after') {
    if(destroyed||pageLeaving||edgeLoading||loading||!continuous||!rows.length)return;
    edgeDirection=direction;edgeError='';edgeLoading=true;
    const request=refreshSequence;edgeController?.abort();const controller=edgeController=new AbortController();
    const boundary=direction==='before'?rows[0]:rows.at(-1)!;
    try {
      const result=await listRecords({...queryOptions(),window:direction,cursor:cursors.get(boundary.uid),metadata:false,
        limit:Math.min(256,Math.max(pageSize,Math.ceil(viewportHeight/(mobileRows?240:50))*2))},moduleUid||undefined,controller.signal);
      if(request!==refreshSequence||controller.signal.aborted||!result.window)return;
      const anchor=currentAnchor();
      const incoming=result.data.filter(row=>!deletedRecordUids.has(row.uid));
      const merged=mergeWindow(rows,incoming,direction==='before',Math.min(1280,pageSize*5),pinnedRows);
      if(!merged){edgeError='Finish editing or move focus away from the earlier record to load more.';return;}
      result.data.forEach((row,index)=>cursors.set(row.uid,result.window!.cursors[index]));
      rows=merged.rows;
      if(direction==='after'){windowAfter=result.window.has_after;if(merged.removed)windowBefore=true;baseIndex+=merged.removed;}
      else{windowBefore=result.window.has_before;if(merged.removed)windowAfter=true;baseIndex=Math.max(0,baseIndex-merged.added);}
      const retained=new Set(rows.map(row=>row.uid));cursors=new Map([...cursors].filter(([uid])=>retained.has(uid)));
      rowHeights=Object.fromEntries(Object.entries(rowHeights).filter(([uid])=>retained.has(uid)));
      await restoreAnchor(anchor);rememberPosition();
    }catch(reason){if(!controller.signal.aborted&&request===refreshSequence){edgeError=reason instanceof Error?reason.message:'More records could not be loaded.';if(reason instanceof ApiError&&[401,403].includes(reason.status)){rows=[];cursors.clear();rowHeights={};}}}
    finally{if(edgeController===controller){edgeLoading=false;scrolled();}}
  }
  function viewChanged(next=continuous){
    const anchor=currentAnchor();rememberPosition();
    // A loading placeholder has no representative content widths. A quick
    // first switch should use datatype widths, not freeze that empty layout.
    if(!loading&&rows.length&&!mobileRows&&tableViewport){
      const widths=Array.from(tableViewport.querySelectorAll<HTMLElement>('thead th')).map(node=>node.getBoundingClientRect().width);
      if(widths.length===visibleFields.length+2)measuredColumns=snapshotRecordColumns(visibleFields,tableViewport.getBoundingClientRect().width-tableViewport.clientLeft*2,widths);
    }
    const previous=continuous;
    if(previous&&!next&&rows.length>pageSize){
      const start=Math.max(0,Math.floor((anchor?.index||0)/pageSize)*pageSize-baseIndex);
      rows=rows.slice(start,start+pageSize);
    }
    continuous=next;cancelAnimationFrame(measureFrame);measureFrame=0;pendingMeasurements.clear();++restoreEpoch;restoringAnchor=undefined;
    if(previous!==next){
      page=anchor?Math.floor(anchor.index/pageSize)+1:1;
      restorePosition=anchor?{...anchor,offset:Math.min(Math.max(40,anchor.offset),viewportHeight/3)}:undefined;
      if(next)window.scrollTo({top:0,behavior:'instant'});
    }
    try{localStorage.setItem(preferenceKey,JSON.stringify({continuous,batch:pageSize,columns:measuredColumns}));}catch{}
    void refresh();
  }
  function rowNumber(index:number){return (continuous?baseIndex:(page-1)*pageSize)+index+1;}
  async function refresh(withSchema = false) {
    if(destroyed||pageLeaving)return;
    const request = ++refreshSequence;
    requestController?.abort();edgeController?.abort();edgeLoading=false;edgeError='';
    const controller=requestController=new AbortController();
    const anchor=restorePosition || currentAnchor();restorePosition=undefined;
    loading = true; error = '';
    try {
      if (withSchema || !schema) {
        const nextSchema = moduleUid ? await loadModuleSchema(moduleUid,controller.signal) : await loadSchema(false,controller.signal);
        if (destroyed||pageLeaving||controller.signal.aborted||request !== refreshSequence) return;
        schema = nextSchema;
        if (sortBy && !nextSchema.fields.some(field => field.key === sortBy && field.active && field.sortable)
          && !nextSchema.system_fields.some(field => field.key === sortBy)) sortBy = '';
        const allowed = new Set(schema.fields.filter((field) => field.active).map((field) => field.key));
        const defaults = schema.fields.filter((field) => field.active && field.table_visible).sort((a, b) => a.position - b.position).map((field) => field.key);
        columnOverrides = { shown: columnOverrides.shown.filter((key) => allowed.has(key)), hidden: columnOverrides.hidden.filter((key) => allowed.has(key)) };
        if (legacyColumns) {
          const selected = legacyColumns.filter((key) => allowed.has(key));
          columnOverrides = { shown: selected.filter((key) => !defaults.includes(key)), hidden: defaults.filter((key) => !selected.includes(key)) };
          legacyColumns = null; localStorage.removeItem('mx_record_columns_v2'); saveColumnOverrides();
        }
        selectedKeys = [...defaults.filter((key) => !columnOverrides.hidden.includes(key)), ...columnOverrides.shown.filter((key) => !defaults.includes(key))];
      }
      const sameQuery=anchor && JSON.stringify([anchor.search,anchor.sortBy,anchor.sortDir,anchor.match,anchor.filters,anchor.attachments])===JSON.stringify([search,sortBy,sortDir,matchMode,$state.snapshot(filters),attachmentMode]);
      if(destroyed||pageLeaving||controller.signal.aborted)return;
      const result = await listRecords({...queryOptions(),...(continuous?{window:sameQuery?'anchor' as const:'start' as const,anchor_uid:sameQuery?anchor.uid:undefined,cursor:sameQuery?anchor.cursor:undefined}: {})},moduleUid || undefined,controller.signal);
      if (destroyed||pageLeaving||controller.signal.aborted||request !== refreshSequence) return;
      rows = result.data.filter((row) => !deletedRecordUids.has(row.uid)); page = result.page; pageInput = result.page; pages = result.total_pages; total = Math.max(0, result.total - (result.data.length - rows.length));
      summaries = result.summaries || {};
      appliedQuery={search,sortBy,sortDir,match:matchMode,filters:$state.snapshot(filters),attachments:attachmentMode};
      if(result.window){cursors=new Map(result.data.map((row,index)=>[row.uid,result.window!.cursors[index]]));windowBefore=result.window.has_before;windowAfter=result.window.has_after;baseIndex=sameQuery?anchor.index:0;}
      rowHeights={};updatesAvailable=false;
      if(sameQuery)await restoreAnchor(anchor);else if(tableViewport){layoutAnchor=false;tableViewport.scrollTop=0;viewportTop=0;}
    } catch (reason) { if (request === refreshSequence && !controller.signal.aborted){error = reason instanceof Error ? reason.message : 'Records could not be loaded.';if(reason instanceof ApiError&&[401,403].includes(reason.status)){rows=[];cursors.clear();rowHeights={};}} }
    finally { if (!destroyed&&request === refreshSequence){loading=false;if(!pageLeaving)scrolled();} }
  }

  function forgetDeletedRecord(uid: string) {
    const anchor=currentAnchor();
    if(anchor?.uid===uid){const index=rows.findIndex(row=>row.uid===uid);anchor.uid=rows[index+1]?.uid||rows[index-1]?.uid||uid;}
    ++refreshSequence; loading=false; summaries={};
    requestController?.abort();edgeController?.abort();edgeLoading=false;updatesAvailable=true;
    deletedRecordUids.add(uid);
    deletionGenerations.set(uid, ++deletionGeneration);
    if (rows.some((row) => row.uid === uid)) total = Math.max(0, total - 1);
    rows = rows.filter((row) => row.uid !== uid);
    cursors.delete(uid);const {[uid]:removedHeight,...remainingHeights}=rowHeights;rowHeights=remainingHeights;
    if (rowMenuUid === uid) rowMenuUid = '';
    if (editor?.uid === uid) closeEditor();
    void restoreAnchor(anchor);
  }

  async function reconcileDeletedRecords() {
    // A restore event may have been missed during an outage. Only an
    // authoritative GET can lift a deletion barrier during resynchronization.
    const pending = [...deletionGenerations]; let restored = false;
    for (let offset = 0; offset < pending.length; offset += 8) {
      await Promise.all(pending.slice(offset, offset + 8).map(async ([uid, generation]) => {
        try {
          await getRecord(uid, moduleUid || undefined);
          if (deletionGenerations.get(uid) !== generation) return;
          deletedRecordUids.delete(uid); deletionGenerations.delete(uid); restored = true;
        } catch { /* Still deleted, forbidden, or offline: keep the barrier. */ }
      }));
    }
    if (restored) await refresh();
  }

  function display(row: MxRecord, key: string) {
    if (row.restricted_fields?.includes(key)) return 'Restricted';
    const field = schema?.fields.find((item) => item.key === key);
    if(field&&!fieldState(field,row.values).visible)return 'Not applicable';
    if (field?.field_type === 'attachments') {
      const files = row.attached_files.filter((item) => item.attachment_field_uid === field.uid);
      if (!files.length) return '—';
      return files.length === 1 ? files[0].file_name : `${files.length} files`;
    }
    const value = row.values[key];
    if (field?.field_type === 'relationship' && row.relationships?.[key]?.restricted) return 'Access restricted';
    if (value == null || value === '') return '—';
    if (typeof value === 'boolean') return value ? 'Yes' : 'No';
    if (field?.field_type === 'relationship') return row.relationships?.[key]?.restricted ? 'Access restricted' : row.relationships?.[key]?.items.map(item=>item.label).join(', ') || '—';
    if (Array.isArray(value)) return value.map(item=>String(item ?? '—')).join(', ');
    if(field?.field_type==='select')return optionLabel(field,value);
    if (field?.field_type === 'auto_number') {
      const prefix = String(field.config.prefix || ''); const padding = Number(field.config.padding || 0);
      return `${prefix}${String(value).padStart(padding, '0')}`;
    }
    return String(value);
  }
  function recordTitle(row:MxRecord){const field=eligibleFields.find(field=>field.field_type!=='attachments'&&fieldState(field,row.values).visible&&row.values[field.key]!=null&&row.values[field.key]!=='');return `${recordSingular} · ${field?display(row,field.key).slice(0,200):row.uid}`;}

  async function searchSubmit(event: SubmitEvent) { event.preventDefault(); page = 1; await refresh(); }
  function scheduleSearch() { window.clearTimeout(searchTimer); searchTimer = window.setTimeout(() => { page = 1; void refresh(); }, 350); }
  async function sort(key: string) { if (sortBy === key) sortDir = sortDir === 'asc' ? 'desc' : 'asc'; else { sortBy = key; sortDir = 'asc'; } page = 1; await refresh(); }
  async function movePage(next: number) { page = next; await refresh(); }
  async function goToPage(event: SubmitEvent) { event.preventDefault(); page = Math.min(Math.max(1, Number(pageInput) || 1), Math.max(1, pages)); await refresh(); }
  function saveColumnOverrides() { localStorage.setItem(columnStorageKey, JSON.stringify(columnOverrides)); }
  function toggleColumn(key: string, checked: boolean) {
    const defaultVisible = schema?.fields.find((field) => field.key === key)?.table_visible === true;
    selectedKeys = checked ? [...selectedKeys.filter((item) => item !== key), key] : selectedKeys.filter((item) => item !== key);
    if (defaultVisible) columnOverrides = { shown: columnOverrides.shown.filter((item) => item !== key), hidden: checked ? columnOverrides.hidden.filter((item) => item !== key) : [...columnOverrides.hidden.filter((item) => item !== key), key] };
    else columnOverrides = { hidden: columnOverrides.hidden.filter((item) => item !== key), shown: checked ? [...columnOverrides.shown.filter((item) => item !== key), key] : columnOverrides.shown.filter((item) => item !== key) };
    saveColumnOverrides();
  }
  function resetColumns() { columnOverrides = { shown: [], hidden: [] }; selectedKeys = eligibleFields.filter((field) => field.table_visible).map((field) => field.key); saveColumnOverrides(); }
  async function changePageSize() { page = 1; await refresh(); }
  async function clearFilters() { filters = {}; search = ''; attachmentMode = ''; sortBy = ''; sortDir = 'asc'; page = 1; await refresh(); }
  function openRecord(row: MxRecord, attachments = false, edit = false) { if (deletedRecordUids.has(row.uid)) return;recordOpenSequence++;editorAnchor=currentAnchor();rememberPosition(); editorChanges=null;rowMenuUid = ''; focusAttachments = attachments; editorWritable = edit && canUpdate; editor = row; }
  function closeEditor() { recordOpenSequence++;const anchor=editorAnchor;editorChanges=null;focusAttachments = false; editorWritable = false; editor = undefined; if (openRecordUid) location.hash = `module/${encodeURIComponent(moduleUid || 'mx-default-records')}`;void tick().then(()=>restoreAnchor(anchor)); }
  async function removeRecord(row: MxRecord, event: MouseEvent) {
    event.stopPropagation();
    closeRowMenuAndFocus();
    if (!await requestConfirmation({
      title: `Move this ${recordSingular.toLowerCase()} to trash?`,
      description: 'Its fields, attachments, and version history will be preserved so an administrator can inspect or restore it.',
      confirmLabel: 'Move to trash'
    })) return;
    deletingUid = row.uid; error = '';
    try { await deleteRecord(row.uid, moduleUid || undefined); if (rows.length === 1 && page > 1) page -= 1; forgetDeletedRecord(row.uid); await refresh(); }
    catch (reason) { error = reason instanceof Error ? reason.message : `The ${recordSingular.toLowerCase()} could not be deleted.`; }
    finally { deletingUid = ''; }
  }
</script>

<section class="workspace-page records-page" class:continuous-page={continuous}>
  <div class="toolbar records-toolbar">
    {#if canCreate}<button class="button primary records-new-button" onclick={() => { recordOpenSequence++;editorChanges=null;focusAttachments = false; editorWritable = true; editor = null; }}>+ New {recordSingular.toLowerCase()}</button>{/if}
    <form class="search-form" onsubmit={searchSubmit}><label class="records-search-input"><Search size={16} aria-hidden="true" /><input bind:value={search} oninput={scheduleSearch} type="search" placeholder={canAttachments ? 'Search configured fields and attachment filenames…' : 'Search configured fields…'} aria-label={`Search ${recordPlural.toLowerCase()}`} /></label><select bind:value={matchMode} onchange={() => { page = 1; void refresh(); }} aria-label="Search matching"><option value="contains">Contains</option><option value="prefix">Starts with</option><option value="exact">Exact</option></select><button class="button" type="submit">Search</button></form>
    <details class="toolbar-menu"><summary class="button">Columns</summary><div class="toolbar-popover">{#each eligibleFields as field}<label class="checkbox"><input type="checkbox" checked={selectedKeys.includes(field.key)} onchange={(event) => toggleColumn(field.key, event.currentTarget.checked)} /> {field.label}</label>{/each}<button class="button small" type="button" onclick={resetColumns}>Use administrator defaults</button><small>Only your overrides are saved, so new fields can follow the deployment defaults.</small></div></details>
    <button class="button reading-settings-button" type="button" onclick={() => readingOpen = true} disabled={!schema}><Bold size={15} aria-hidden="true"/>Reading settings</button>
    <button class="button records-refresh-button" onclick={() => refresh(true)}>Refresh</button>
    {#if moduleUid}<ModuleWorkQueue {moduleUid} moduleName={recordPlural} {revision} onOpen={uid=>void openLinkedRecord(uid)}/>{/if}
    {#if moduleUid}{#key moduleUid}<ModuleBooks {moduleUid} moduleName={recordPlural} {revision}/>{/key}{/if}
    {#if canCreate && schema}<button class="button module-import-trigger" type="button" data-module-import-actions aria-label="Module actions" title="Module actions" aria-haspopup="menu" aria-expanded={importMenuOpen} aria-controls={importMenuOpen ? 'module-import-menu' : undefined} onclick={toggleImportMenu} onkeydown={importTriggerKeydown}><Ellipsis size={18} aria-hidden="true" /></button>{/if}
  </div>
    <details class="advanced-search"><summary>Advanced filters and sorting</summary><form class="filter-grid" onsubmit={searchSubmit}>{#each searchableFields as field}<label>{field.label}{#if field.field_type === 'select'}<select value={filters[field.key] || ''} onchange={(event) => filters[field.key] = event.currentTarget.value}><option value="">Any value</option>{#each (field.config.options as (string | number | boolean)[] || []) as option}<option value={String(option)}>{String(option)}</option>{/each}</select>{:else if field.field_type === 'boolean'}<select value={filters[field.key] || ''} onchange={(event) => filters[field.key] = event.currentTarget.value}><option value="">Either</option><option value="true">Yes</option><option value="false">No</option></select>{:else}<input type={field.field_type === 'date' ? 'date' : ['integer', 'decimal', 'formula'].includes(field.field_type) ? 'number' : 'text'} value={filters[field.key] || ''} oninput={(event) => filters[field.key] = event.currentTarget.value} placeholder={field.field_type === 'date' ? undefined : 'Field contains…'} />{/if}</label>{/each}{#if canAttachments}<label>Attachments<select bind:value={attachmentMode}><option value="">With or without files</option><option value="with">With attachments</option><option value="without">Without attachments</option></select></label>{/if}<label>Sort field<select bind:value={sortBy}><option value="">Default order</option>{#each eligibleFields.filter((field) => field.sortable) as field}<option value={field.key}>{field.label}</option>{/each}</select></label><label>Sort direction<select bind:value={sortDir}><option value="asc">Ascending</option><option value="desc">Descending</option></select></label><div class="button-row"><button class="button primary">Apply filters</button><button class="button" type="button" onclick={clearFilters}>Clear all</button></div></form></details>
  {#if error}<div class="notice error">{error}</div>{/if}
  {#if continuous && updatesAvailable}<div class="record-updates" role="status"><span>Records changed. Your browsing position is kept.</span><button class="button small" disabled={loading} onclick={()=>void refresh(true)}>Refresh this view</button></div>{/if}
  <div class="records-view-controls">
    <div class="records-result-summary" aria-live="polite" aria-label={`${total.toLocaleString()} ${total === 1 ? recordSingular.toLowerCase() : recordPlural.toLowerCase()}`}>
      <span><strong>{total.toLocaleString()}</strong> {total === 1 ? 'result' : 'results'}</span>
      <small>{loading?'Loading…':continuous?`${rows.length.toLocaleString()} cached${edgeLoading?` · Loading ${edgeDirection==='before'?'earlier':'more'}…`:!windowAfter?' · End of results':''}`:rows.length ? `Showing ${((page - 1) * pageSize + 1).toLocaleString()}–${Math.min(page * pageSize, total).toLocaleString()}` : 'No rows in this view'}</small>
    </div>
    {#if continuous}<div class="record-window-controls" role="group" aria-label="Load record windows">
      <button class="button" aria-label="Load earlier" title="Load earlier records" disabled={loading||edgeLoading||!windowBefore} onclick={()=>void loadEdge('before')}><ChevronUp size={16} aria-hidden="true" /></button>
      <button class="button" aria-label="Load more" title="Load more records" disabled={loading||edgeLoading||!windowAfter} onclick={()=>void loadEdge('after')}><ChevronDown size={16} aria-hidden="true" /></button>
    </div>{/if}
    <label>View<select aria-label="Record navigation" value={String(continuous)} onchange={event=>viewChanged(event.currentTarget.value==='true')}><option value="false">Pages</option><option value="true">Continuous scroll</option></select></label>
    {#if continuous}<label>Batch<select aria-label="Record batch size" bind:value={pageSize} onchange={()=>viewChanged()}><option value={25}>25</option><option value={50}>50</option><option value={100}>100</option><option value={256}>256</option></select></label>{/if}
  </div>
  <div class="table-wrap records-table" class:continuous-records={continuous} aria-busy={loading} id={`records-table-${moduleUid || 'default'}`} bind:this={tableViewport} use:observeViewport onscroll={scrolled} onfocusin={event=>focusedRow=(event.target as Element)?.closest<HTMLElement>('[data-record-uid]')?.dataset.recordUid||''} onfocusout={event=>focusedRow=(event.relatedTarget as Element)?.closest?.<HTMLElement>('[data-record-uid]')?.dataset.recordUid||''}>
    <table aria-label={recordPlural} aria-rowcount={continuous?-1:undefined} style:table-layout={fixedColumns?'fixed':undefined} style:width={fixedColumns?`${tableWidth}px`:undefined}>
      {#if fixedColumns}<colgroup>{#each columnWidths as width}<col style:width={`${width}px`}/>{/each}</colgroup>{/if}
      <thead><tr aria-rowindex={continuous?1:undefined}><th scope="col" class="row-index-column">#</th>{#each visibleFields as field}<th scope="col" aria-sort={sortBy===field.key?(sortDir==='asc'?'ascending':'descending'):undefined} data-field-uid={field.uid} class:personal-field-bold={fieldIsBold($readingPreferences,moduleUid,field.uid)}><button class="table-sort" class:sortable={field.sortable} onclick={() => field.sortable && sort(field.key)}>{field.label}{#if sortBy === field.key}{#if sortDir === 'asc'}<ChevronUp size={14} />{:else}<ChevronDown size={14} />{/if}{/if}</button></th>{/each}<th scope="col" class="record-actions-column">Actions</th></tr></thead>
      <tbody>
        {#if loading && !rows.length}
          <tr class="records-state-row"><td class="table-message" colspan={visibleFields.length + 2}>Loading {recordPlural.toLowerCase()}…</td></tr>
        {:else if !rows.length}
          <tr class="records-state-row"><td class="table-message" colspan={visibleFields.length + 2}>No {recordPlural.toLowerCase()} match this view.</td></tr>
        {:else}
          {#each renderPieces as piece (piece.key)}
            {#if piece.kind==='gap'}<tr class="record-window-gap" aria-hidden="true"><td colspan={visibleFields.length+2} style:height={`${piece.height}px`}></td></tr>
            {:else}
            {@const rowIndex=piece.index}
            {@const row=rows[rowIndex]}
            <tr data-record-uid={row.uid} aria-rowindex={continuous?baseIndex+rowIndex+2:undefined} use:measureRow={row.uid} onclick={(event) => { if (event.target instanceof Element && event.target.closest('button,a,input,textarea,select')) return; if (window.getSelection()?.toString()) return; openRecord(row); }} class="clickable-row">
              <td class="row-index-column" data-label="Row"><span title={`${recordSingular} ${row.uid}${continuous?' · position in this browsing session':''}`}>{rowNumber(rowIndex)}</span></td>
              {#each visibleFields as field}
                <td class:personal-field-bold={fieldIsBold($readingPreferences,moduleUid,field.uid)} class={field.field_type === 'long_text' ? 'long-text-cell' : ''} data-field-uid={field.uid} data-label={field.label} data-field-type={field.field_type} title={field.field_type === 'long_text' ? undefined : display(row, field.key)}>
                  {#if row.restricted_fields?.includes(field.key)}<span class="muted">Restricted</span>
                  {:else if !fieldState(field,row.values).visible}<span class="muted">Not applicable</span>
                  {:else if field.field_type === 'attachments'}
                    {@const fieldFiles = row.attached_files.filter((file) => file.attachment_field_uid === field.uid)}
                    <button class="attachment-cell-button" class:empty={!fieldFiles.length} onclick={(event) => { event.stopPropagation(); openRecord(row, true); }}>{fieldFiles.length ? `${fieldFiles.length} file${fieldFiles.length === 1 ? '' : 's'}` : 'No files'}</button>
                  {:else}
                    {@const presentation = columnPresentation($readingPreferences,moduleUid,field)}
                    {@const value = row.values[field.key]}
                    {@const hasValue = value != null && value !== '' && (!Array.isArray(value) || value.length > 0) && !(field.field_type==='relationship' && row.relationships?.[field.key]?.restricted)}
                    <span class="record-cell-value column-value" class:column-badge={hasValue && presentation.style==='badge'} data-tone={hasValue?columnTone(presentation,row.values[field.key],field.field_type):'none'}>{display(row, field.key)}</span>
                  {/if}
                </td>
              {/each}
              <td class="record-actions-column">
                <div class="record-row-actions" data-record-actions={row.uid}>
                  <button class="button record-primary-action" type="button" title={canUpdate ? `Edit ${recordSingular.toLowerCase()}` : `View ${recordSingular.toLowerCase()}`} aria-label={`${canUpdate ? 'Edit' : 'View'} ${recordSingular} ${rowNumber(rowIndex)}`} onclick={(event) => { event.stopPropagation(); rowMenuUid = ''; openRecord(row, false, canUpdate); }}>{#if canUpdate}<Pencil size={14} /><span>Edit</span>{:else}<Eye size={14} /><span>View</span>{/if}</button>
                  <button class="record-actions-trigger" type="button" aria-label={`More actions for ${recordSingular} ${rowNumber(rowIndex)}`} aria-haspopup="menu" aria-expanded={rowMenuUid === row.uid} onclick={(event) => toggleRowMenu(event, row.uid)}><Ellipsis size={17} /></button>
                </div>
              </td>
            </tr>
            {/if}
          {/each}
        {/if}
      </tbody>
      {#if summaryFields.length}<tfoot aria-label="Filtered column summaries"><tr><th scope="row" class="summary-caption">Totals</th>{#each visibleFields as field}<td data-label={field.label}>{#if field.config.table_summary && field.config.table_summary !== 'none'}<span class="column-summary"><small>{summaryNames[field.config.table_summary as keyof typeof summaryNames]}</small><strong aria-label={`${field.label} summary`}>{loading ? 'Calculating…' : summaryValue(summaries[field.key])}</strong></span>{/if}</td>{/each}<td></td></tr></tfoot>{/if}
    </table>
  </div>
  {#if continuous&&edgeError}<div class="record-window-error" role="alert"><span>{edgeError}</span><button class="button small" onclick={()=>void loadEdge(edgeDirection)}>Retry</button></div>{/if}
  {#if summaryFields.length}<p class="summary-scope" role="status">Summaries cover all {total.toLocaleString()} matching records, not only the rows shown. Search and filters apply.{updatesAvailable?' Refresh to update totals.':''}</p>{/if}
  {#if continuous&&summaryFields.length}<details class="record-mobile-totals"><summary>Totals for matching records</summary><dl>{#each summaryFields as field}<div><dt>{field.label} · {summaryNames[field.config.table_summary as keyof typeof summaryNames]}</dt><dd>{loading?'Calculating…':summaryValue(summaries[field.key])}</dd></div>{/each}</dl></details>{/if}
  {#if !continuous}
  <div class="pagination records-pagination"><span>{rows.length ? `${((page - 1) * pageSize + 1).toLocaleString()}–${Math.min(page * pageSize, total).toLocaleString()} of ${total.toLocaleString()}` : 'No records'}</span><label>Rows<select bind:value={pageSize} onchange={changePageSize}><option value={25}>25</option><option value={50}>50</option><option value={100}>100</option><option value={256}>256</option></select></label><button class="button" disabled={page <= 1 || loading} onclick={() => movePage(1)}>First</button><button class="button" disabled={page <= 1 || loading} onclick={() => movePage(page - 1)}>Previous</button><form onsubmit={goToPage}><label>Page<input type="number" min="1" max={Math.max(1, pages)} bind:value={pageInput} /></label><button class="button" aria-label="Go to page">Go</button></form><span>of {pages}</span><button class="button" disabled={page >= pages || loading} onclick={() => movePage(page + 1)}>Next</button><button class="button" disabled={page >= pages || loading} onclick={() => movePage(pages)}>Last</button></div>
  {/if}
</section>

<TableScrollbar target={tableViewport} enabled={editor === undefined} alwaysVisible={continuous} />

{#if menuRow && rowMenuAnchor}
  <div class="record-row-menu" data-record-actions={menuRow.uid} role="menu" aria-label={`${recordSingular} actions`} tabindex="-1" onkeydown={menuKeydown} use:floatingMenu={{ anchor: rowMenuAnchor, close: () => rowMenuUid = '' }}>
    <button role="menuitem" type="button" onclick={() => { sendRecord = menuRow!; closeRowMenuAndFocus(); }}><Send size={14}/>Send record</button>
    {#if canDelete}<button class="danger-text" role="menuitem" type="button" disabled={deletingUid === menuRow.uid} onclick={(event) => removeRecord(menuRow, event)}><Trash2 size={14} />{deletingUid === menuRow.uid ? 'Moving…' : 'Move to trash'}</button>{/if}
  </div>
{/if}

{#if canCreate && schema && importMenuOpen && importMenuAnchor}
  <div class="module-import-menu" id="module-import-menu" data-module-import-actions role="menu" aria-label="Module actions" tabindex="-1" use:importMenuKeyboard use:floatingMenu={{ anchor: importMenuAnchor, close: () => importMenuOpen = false }}>
    <button role="menuitem" tabindex="-1" type="button" onclick={() => openImporter('new')}><Upload size={16} aria-hidden="true" /><span>Import records</span></button>
    <button role="menuitem" tabindex="-1" type="button" onclick={() => openImporter('history')}><History size={16} aria-hidden="true" /><span>Import history</span></button>
  </div>
{/if}

{#if editor !== undefined && schema}<RecordEditor {schema} {moduleUid} {recordSections} record={editor} initialChanges={editorChanges} canWrite={editor === null ? canCreate : editorWritable && canUpdate} canDelete={editorWritable && canDelete} {canAttachments} allowEdit={editor !== null && canUpdate} {liveMessage} {focusAttachments} recordLabel={recordSingular} onEdit={() => editorWritable = true} onClose={closeEditor} onSaved={() => refresh(true)} />{/if}

{#if readingOpen && schema}<ModuleReadingDialog {moduleUid} moduleName={recordPlural} fields={eligibleFields} onSaved={preferences => $readingPreferences = preferences} onClose={() => readingOpen = false}/>{/if}
{#if sendRecord}<SendRecordDialog recordUid={sendRecord.uid} recordLabel={recordTitle(sendRecord)} onClose={() => sendRecord = null}/>{/if}

{#if importer && schema}{#key moduleUid}<ImportRecords {schema} {moduleUid} history={importer === 'history'} onClose={() => { importer = null; importMenuAnchor?.focus({ preventScroll: true }); }} onChanged={() => refresh(true)} />{/key}{/if}

<style>
  .personal-field-bold,.personal-field-bold .table-sort,.personal-field-bold .attachment-cell-button{font-weight:750;}
  .long-text-cell .record-cell-value{white-space:pre-wrap;overflow-wrap:anywhere;}
  .reading-settings-button{order:3;}
  .records-page.continuous-page{display:flex;flex-direction:column;width:100%;max-width:none;min-width:0;height:calc(100dvh - var(--topbar-height));min-height:24rem;padding-bottom:max(.5rem,env(safe-area-inset-bottom));}
  .records-page.continuous-page>*{min-width:0;}
  .continuous-page>:not(.continuous-records){flex-shrink:0;}
  .records-view-controls,.record-window-error,.record-updates{display:flex;align-items:center;gap:.75rem;flex-wrap:wrap;}
  .records-view-controls{margin:.35rem 0 .5rem;}.records-view-controls .records-result-summary{flex:1;margin:0;}.records-view-controls>label{display:flex;align-items:center;gap:.4rem;font-size:.75rem;}
  .record-window-controls{display:flex;gap:.2rem;}.record-window-controls .button{width:2rem;min-height:2rem;padding:0;justify-content:center;}
  .record-updates{padding:.5rem .7rem;border:1px solid var(--line);border-radius:.45rem;background:var(--surface-2);font-size:.8rem;}.record-updates>span{flex:1;}
  .records-table.continuous-records{flex:1 1 0;min-height:9rem;overflow:auto;overflow-anchor:none;scroll-behavior:auto;}
  .continuous-records th{position:sticky;top:0;z-index:4;}
  .record-window-gap,.record-window-gap>td{padding:0!important;border:0!important;min-height:0!important;line-height:0!important;background:transparent!important;}
  .record-window-error{margin-top:.4rem;padding:.4rem .6rem;border:1px solid var(--danger);border-radius:.4rem;background:var(--danger-surface);font-size:.75rem;}.record-window-error>span{flex:1;min-width:0;overflow-wrap:anywhere;}
  .record-mobile-totals{display:none;}
  @media(min-width:821px){.continuous-records tfoot th,.continuous-records tfoot td{position:sticky;bottom:0;z-index:6;}}
  @media(max-width:820px){.records-table.continuous-records{overflow:auto;}.continuous-records tbody{gap:0;}.continuous-records tr.clickable-row{margin-bottom:.55rem;}.record-window-gap{display:block;}.record-window-gap>td{display:block;width:100%;}.records-view-controls{gap:.5rem;}.records-view-controls .records-result-summary{flex-basis:100%;}.continuous-records tfoot{display:none;}.record-mobile-totals{display:block;padding:.5rem;border:1px solid var(--line);border-radius:.45rem;font-size:.8rem;}.record-mobile-totals dl>div{display:flex;justify-content:space-between;gap:.5rem;padding:.3rem;}.record-mobile-totals dd{font-weight:600;}}
  .records-refresh-button { order: 4; }
  .module-import-trigger { order: 5; flex: 0 0 auto; width: 2.2rem; padding-inline: 0; justify-content: center; }
  .module-import-menu { position: fixed; visibility: hidden; min-width: 13rem; z-index: 120; overflow: auto; padding: .35rem; background: var(--surface); border: 1px solid var(--line); border-radius: .55rem; box-shadow: var(--shadow-popover); color: var(--text); font-size: calc(.8rem * var(--font-scale)); }
  .module-import-menu button { display: flex; align-items: center; gap: .65rem; width: 100%; min-height: 2.5rem; padding: .65rem .75rem; border: 0; border-radius: .35rem; text-align: left; font: inherit; line-height: 1.35; color: inherit; background: transparent; cursor: pointer; }
  .module-import-menu button:hover, .module-import-menu button:focus-visible { background: var(--surface-2); }
  .module-import-menu button :global(svg) { flex-shrink: 0; }
  @media (max-width: 760px) {
    .records-toolbar { grid-template-columns: repeat(2,minmax(0,1fr)); }
    .records-toolbar .toolbar-menu{min-width:0;}
    .records-toolbar .toolbar-menu>summary{min-width:0;width:100%;justify-content:center;white-space:normal;}
    .records-toolbar>.reading-settings-button,.records-toolbar>.records-refresh-button{min-width:0;width:100%;justify-content:center;white-space:normal;}
    .module-import-trigger{justify-self:end;}
  }
  @media(max-width:480px){.records-toolbar .search-form{grid-template-columns:minmax(0,1fr);}.records-toolbar .search-form>*{grid-column:1/-1;}.records-toolbar .search-form>select{width:100%;}}
</style>
