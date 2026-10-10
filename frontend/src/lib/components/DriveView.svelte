<script lang="ts">
  import { onMount, onDestroy, untrack } from 'svelte';
  import Upload from '@lucide/svelte/icons/upload';
  import FolderPlus from '@lucide/svelte/icons/folder-plus';
  import FolderInput from '@lucide/svelte/icons/folder-input';
  import HardDrive from '@lucide/svelte/icons/hard-drive';
  import Search from '@lucide/svelte/icons/search';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import LayoutGrid from '@lucide/svelte/icons/layout-grid';
  import List from '@lucide/svelte/icons/list';
  import Star from '@lucide/svelte/icons/star';
  import Share2 from '@lucide/svelte/icons/share-2';
  import Download from '@lucide/svelte/icons/download';
  import Pencil from '@lucide/svelte/icons/pencil';
  import History from '@lucide/svelte/icons/history';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import X from '@lucide/svelte/icons/x';
  import Link from '@lucide/svelte/icons/link';
  import MoreHorizontal from '@lucide/svelte/icons/ellipsis';
  import Copy from '@lucide/svelte/icons/copy';
  import Scissors from '@lucide/svelte/icons/scissors';
  import ClipboardPaste from '@lucide/svelte/icons/clipboard-paste';
  import FilePreview from './FilePreview.svelte';
  import DrivePreview from './DrivePreview.svelte';
  import DrivePagination from './DrivePagination.svelte';
  import Users from '@lucide/svelte/icons/users';
  import ShieldCheck from '@lucide/svelte/icons/shield-check';
  import DriveFileIcon from './DriveFileIcon.svelte';
  import DrivePicker from './DrivePicker.svelte';
  import { requestConfirmation } from '../confirmation';
  import { focusDialog } from '../util/focusDialog';
  import { driveList, driveFolder, driveChange, driveStar, drivePurge, drivePreview, driveDownload, driveSharing, driveGrant, drivePeople, driveLink, driveRevokeLink, driveHistory, driveSize, type DrivePage, type DriveItem, type DriveTab, type DriveSharing, type DriveHistory } from '../api/drive';
  import type { DriveUploads } from '../drive/uploads.svelte';
  import { driveTransfer, driveRestoreVersion } from '../api/drive';
  import {driveSpaces,driveSpaceGrant,type DriveSpace,type DriveSort} from '../api/drive';
  import {ApiError} from '../api/client';
  import { droppedFiles, selection, intersects, isEditing, activityLabel, type UploadEntry } from '../drive/fileManager';
  import { driveMenu } from '../drive/contextMenu';
  import { canOpenOffice, canEditOffice, openOffice } from '../office/launch';
  let { accountUid, revision = 0, uploads }: { accountUid: string; revision?: number; uploads: DriveUploads } = $props();
  const tabs: { id: DriveTab; label: string }[] = [{ id: 'mine', label: 'My Drive' }, { id: 'shared', label: 'Shared with me' }, { id: 'recent', label: 'Recent' }, { id: 'starred', label: 'Starred' }, { id: 'trash', label: 'Trash' }];
  let tab = $state<DriveTab>('mine'); let parent = $state<string | null>(null); let query = $state(''); let offset = $state(0); let grid = $state(false); let page = $state<DrivePage | null>(null); let busy = $state(false); let error = $state(''); let notice = $state(''); let loading = $state(false); let mounted = false; let generation = 0;
  let pageSize=$state(50);let sort=$state<DriveSort>('modified_desc');
  let shareTarget=$state<'people'|'spaces'>('people');let spaces=$state<DriveSpace[]>([]);let finding=$state(false);let found=$state(false);let searchGeneration=0;
  let fileInput: HTMLInputElement; let folderInput: HTMLInputElement; let versionInput: HTMLInputElement;
  let versionTarget = $state<DriveItem | null>(null); let preview = $state<DriveItem | null>(null); let previewVersion = $state<string | undefined>(undefined);
  let dialog = $state<'folder' | 'rename' | 'share' | 'history' | null>(null); let selected = $state<DriveItem | null>(null); let label = $state(''); let moving = $state<DriveItem[] | null>(null);
  let chosen = $state(new Set<string>()); let anchor = $state<string | null>(null); let historyTab = $state<'versions'|'activity'>('versions');
  let root: HTMLElement; let fileArea: HTMLDivElement; let menu = $state<{x:number;y:number;anchor?:HTMLElement;items:DriveItem[]}|null>(null);
  let dragDepth = 0; let dropping = $state(false); let marquee = $state<{left:number;top:number;width:number;height:number}|null>(null); let endMarquee: (()=>void)|undefined;
  const chosenItems = $derived((page?.items || []).filter(item => chosen.has(item.uid)));
  const allChosen = $derived(!!page?.items.length && chosenItems.length === page.items.length);
  let sharing = $state<DriveSharing | null>(null); let history = $state<DriveHistory | null>(null); let peopleQuery = $state(''); let people = $state<{ uid: string; name: string; email: string }[]>([]); let shareRole = $state<'viewer' | 'editor'>('viewer'); let linkDays = $state('7'); let newLink = $state(''); let dialogError = $state('');
  const preparing = $derived(uploads.preparing);
  const canUpload = $derived(tab === 'mine');
  const storagePercent = $derived(page ? Math.min(100, page.quota_bytes ? page.used_bytes / page.quota_bytes * 100 : page.used_bytes ? 100 : 0) : 0);
  onMount(() => { mounted = true; void load(); const timer = setInterval(() => { if (document.visibilityState === 'visible' && !loading && !busy && !marquee && !menu && !preview && !dialog) void load(true); }, 15_000); const online = () => void load(true); window.addEventListener('online', online); document.addEventListener('keydown', keyboard); document.addEventListener('copy',copyEvent); document.addEventListener('cut',copyEvent); document.addEventListener('paste',pasteEvent); return () => { clearInterval(timer); window.removeEventListener('online', online); document.removeEventListener('keydown',keyboard); document.removeEventListener('copy',copyEvent); document.removeEventListener('cut',copyEvent); document.removeEventListener('paste',pasteEvent); }; });
  onDestroy(() => { mounted = false; ++generation; endMarquee?.(); });
  $effect(() => { void revision; void uploads.revision; if (mounted) untrack(() => { void load(true); if (dialog === 'history' && selected) void uploaded(selected); }); });
  async function load(quiet = false) {
    const token = ++generation; if (!quiet) loading = true;
    try { const result = await driveList(tab, parent, query, offset,{limit:pageSize,sort}); if (token === generation) { page = result;offset=result.offset; chosen = new Set([...chosen].filter(uid => result.items.some(item => item.uid === uid))); if (selected) selected = result.items.find((item) => item.uid === selected?.uid) || selected; if (!quiet) error = ''; } }
    catch (reason) { if(token===generation){if(parent&&reason instanceof ApiError&&[403,404].includes(reason.status)){parent=null;offset=0;query='';page=null;chosen=new Set();anchor=null;menu=null;notice='This folder is no longer available. Returned to '+(tab==='shared'?'Shared with me.':'My Drive.');void load();}else if(!quiet)error=reason instanceof Error?reason.message:'Could not load Drive.';} }
    finally { if (token === generation) loading = false; }
  }
  function navigate(uid: string | null, next = tab) { parent = uid; tab = next; query = ''; offset = 0; error = ''; chosen = new Set(); anchor = null; menu = null; void load(); }
  function pageTo(next:number){offset=next;chosen=new Set();anchor=null;menu=null;void load();fileArea?.scrollTo({top:0});}
  function open(item: DriveItem) { if (tab === 'trash') return; if (item.kind === 'folder') navigate(item.uid, item.permission === 'owner' ? 'mine' : 'shared'); else { previewVersion = undefined; preview = item; } }
  function launchOffice(item: DriveItem) { sessionStorage.setItem('mx_office_theme',document.documentElement.dataset.theme || 'light'); if(!openOffice(item)) error='The browser blocked the editor tab. Allow popups for MX and try again.'; }
  async function action(operation: () => Promise<unknown>, message = '') { busy = true; error = ''; try { await operation(); if (message) notice = message; await load(true); } catch (reason) { error = reason instanceof Error ? reason.message : 'This operation failed.'; } finally { busy = false; } }
  function edit(item: DriveItem) { selected = item; label = item.name; dialogError = ''; dialog = 'rename'; }
  async function saveName() { busy = true; dialogError = ''; try { if (dialog === 'folder') await driveFolder(label, parent); else if (selected) await driveChange(selected, 'rename', label); dialog = null; await load(true); } catch (reason) { dialogError = reason instanceof Error ? reason.message : 'Could not save.'; } finally { busy = false; } }
  async function share(item: DriveItem) { selected = item; sharing = null; people = [];spaces=[];shareTarget='people';found=false;peopleQuery = ''; newLink = ''; dialogError = ''; dialog = 'share'; try { const result=await driveSharing(item.uid);if(selected?.uid===item.uid&&dialog==='share')sharing=result; } catch (reason) { dialogError = String(reason); } }
  async function updateGrant(user: string, role: 'viewer' | 'editor' | null) { if (!selected) return; busy = true; dialogError = ''; try { await driveGrant(selected.uid, user, role); sharing = await driveSharing(selected.uid); people=people.filter(p=>p.uid!==user); } catch (reason) { dialogError = reason instanceof Error ? reason.message : 'Could not update sharing.'; } finally { busy = false; } }
  async function findPeople() { const request=++searchGeneration;const target=shareTarget;finding=true;found=false;dialogError='';try {if(target==='people'){const result=await drivePeople(peopleQuery);if(request===searchGeneration)people=result.people.filter(item=>item.uid!==accountUid&&!sharing?.users.some(u=>u.user_uid===item.uid));}else{const result=await driveSpaces(peopleQuery);if(request===searchGeneration)spaces=result.spaces.filter(item=>!sharing?.spaces.some(s=>s.space_uid===item.uid));}if(request===searchGeneration)found=true;}catch(reason){if(request===searchGeneration)dialogError=reason instanceof Error?reason.message:'Search unavailable.';}finally{if(request===searchGeneration)finding=false;} }
  function switchShareTarget(target:'people'|'spaces'){shareTarget=target;people=[];spaces=[];peopleQuery='';found=false;finding=false;++searchGeneration;}
  async function updateSpace(space:string,role:'viewer'|'editor'|null){if(!selected)return;busy=true;dialogError='';try{await driveSpaceGrant(selected.uid,space,role);sharing=await driveSharing(selected.uid);spaces=spaces.filter(s=>s.uid!==space);}catch(reason){dialogError=reason instanceof Error?reason.message:'Could not update space access.';}finally{busy=false;}}
  async function createLink() { if (!selected) return; busy = true; dialogError = ''; try { const result = await driveLink(selected.uid, linkDays === '0' ? null : Date.now() + Number(linkDays) * 86400_000); newLink = `${location.origin}${location.pathname}#drive-share/${result.token}`; sharing = await driveSharing(selected.uid); } catch (reason) { dialogError = reason instanceof Error ? reason.message : 'Could not create link.'; } finally { busy = false; } }
  async function revoke(link: string) { if (!selected) return; busy = true; try { await driveRevokeLink(selected.uid, link); sharing = await driveSharing(selected.uid); newLink = ''; } catch (reason) { dialogError = String(reason); } finally { busy = false; } }
  async function showHistory(item: DriveItem) { selected = item; history = null; historyTab = item.kind === 'folder' ? 'activity' : 'versions'; dialogError = ''; dialog = 'history'; try { const result = await driveHistory(item.uid); if(dialog === 'history' && selected?.uid === item.uid) history = result; } catch (reason) { dialogError = String(reason); } }
  async function uploaded(item: DriveItem) { if (dialog !== 'history' || selected?.uid !== item.uid) return; selected = item; try { const result = await driveHistory(item.uid); if (dialog === 'history' && selected?.uid === item.uid) history = result; } catch (reason) { dialogError = String(reason); } }
  async function purge(item?: DriveItem) { if (await requestConfirmation({ title: item ? 'Delete permanently?' : 'Empty trash?', description: 'MX metadata and every file version will be removed. This cannot be undone in MX. Storage cleanup runs in the background under N1 retention.', confirmLabel: item ? 'Delete permanently' : 'Empty trash', tone: 'danger' })) await action(() => drivePurge(item?.uid), 'Removed from MX. N1 storage cleanup is queued in the background.'); }
  async function restoreVersion(version: string) { if (!selected || !await requestConfirmation({ title: 'Restore this version?', description: 'This creates a new current version; the existing history is preserved.', confirmLabel: 'Restore version' })) return; const target = selected; await action(async () => { await driveRestoreVersion(target,version); dialog = null; },'Version restored. Existing history was preserved.'); }
  async function queueFiles(list: FileList | File[] | UploadEntry[], version?: DriveItem, folders: string[] = [], destination = parent) {
    if (!canUpload && !version) return;
    await uploads.enqueue(list, destination, version, folders);
    if (mounted) await load(true);
  }
  function date(value: number) { return new Date(value).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' }); }
  function choose(item: DriveItem, event: {ctrlKey?:boolean;metaKey?:boolean;shiftKey?:boolean}, checkbox = false) {
    chosen = selection(chosen,(page?.items||[]).map(i=>i.uid),item.uid,anchor,checkbox || !!(event.ctrlKey || event.metaKey),!!event.shiftKey);
    if (!event.shiftKey) anchor = item.uid;
  }
  function selectAll() { chosen = allChosen ? new Set() : new Set((page?.items||[]).map(i=>i.uid)); }
  function context(event: MouseEvent, item?: DriveItem, button = false) {
    if ((event.target as Element).closest('input,textarea,[contenteditable="true"]')) return;
    event.preventDefault(); event.stopPropagation();
    if (item && !chosen.has(item.uid)) { chosen = new Set([item.uid]); anchor = item.uid; }
    menu = {x:event.clientX,y:event.clientY,anchor:button?event.currentTarget as HTMLElement:undefined,items:item?chosenItems:[]};
  }
  function stage(items: DriveItem[], action: 'copy'|'move', writeClipboard = true) {
    if (!items.length || tab === 'trash' || (action === 'move' && items.some(i=>i.permission!=='owner'))) return;
    uploads.stage(items,action); notice = `${items.length} ${items.length === 1 ? 'item' : 'items'} ready to ${action === 'move' ? 'move' : 'copy'}. Open the destination folder and paste.`; menu = null;
    if(writeClipboard) void navigator.clipboard?.writeText(items.map(i=>i.name).join('\n')).catch(()=>{});
  }
  async function paste(destination = parent) {
    const clipboard = uploads.clipboard; if(!clipboard || !canUpload || busy) return;
    const items = clipboard.items; const actionName = clipboard.action;
    await action(async () => {
      await driveTransfer(items,actionName,destination,clipboard.operation);
      if (actionName === 'move') uploads.clearClipboard();
      else uploads.stage(items,'copy');
      chosen = new Set();
    },`${items.length} ${items.length === 1 ? 'item' : 'items'} ${actionName === 'move' ? 'moved' : 'copied'}.`);
  }
  function shortcutsAllowed(target: EventTarget | null, allowBusy = false) { return !dialog && !preview && !moving && (allowBusy || !busy) && !isEditing(target) && !document.querySelector('[role="dialog"],[role="alertdialog"]'); }
  function copyEvent(event: ClipboardEvent) {
    if (!shortcutsAllowed(event.target,true) || !chosenItems.length || tab==='trash' || window.getSelection()?.toString()) return;
    const actionName=event.type==='cut'?'move':'copy';
    if(actionName==='move' && chosenItems.some(i=>i.permission!=='owner')) return;
    event.preventDefault(); stage(chosenItems,actionName,false);
    event.clipboardData?.setData('application/x-mx-drive',JSON.stringify({account:accountUid,items:chosenItems.map(i=>i.uid)}));
    event.clipboardData?.setData('text/plain',chosenItems.map(i=>i.name).join('\n'));
  }
  function pasteEvent(event: ClipboardEvent) {
    if (!shortcutsAllowed(event.target,true) || !canUpload || !event.clipboardData) return;
    const external = Array.from(event.clipboardData.files);
    if (!external.length) for(const item of Array.from(event.clipboardData.items)) { if(item.kind==='file') { const file=item.getAsFile(); if(file) external.push(file); } }
    if(external.length) { event.preventDefault(); void queueFiles(external); return; }
    if(busy) return;
    // The account-scoped clipboard is authoritative for internal files. OS
    // clipboard writes can lag behind a toolbar Copy; stale native markers must
    // not suppress a paste of the newer selection. External files win above.
    if(uploads.clipboard) { event.preventDefault(); void paste(); }
  }
  function keyboard(event: KeyboardEvent) {
    if (!shortcutsAllowed(event.target) || menu) return;
    const modifier = event.ctrlKey || event.metaKey;
    if (modifier && event.key.toLowerCase()==='a') { event.preventDefault(); chosen=new Set((page?.items||[]).map(i=>i.uid)); }
    else if(event.key==='Escape') { chosen=new Set(); uploads.clearClipboard(); }
    else if(event.key==='F2' && chosenItems.length===1 && chosenItems[0].permission!=='viewer' && tab!=='trash') { event.preventDefault(); edit(chosenItems[0]); }
    else if(event.key==='Delete' && chosenItems.length && tab!=='trash') { event.preventDefault(); void trashSelection(chosenItems); }
    else if(event.target instanceof Element) {
      const row=event.target.closest<HTMLElement>('[data-drive-uid]');if(!row)return;
      const ordered=page?.items||[];const index=ordered.findIndex(i=>i.uid===row.dataset.driveUid);
      if(['ArrowDown','ArrowUp','Home','End'].includes(event.key)) {
        event.preventDefault();const next=ordered[event.key==='Home'?0:event.key==='End'?ordered.length-1:Math.max(0,Math.min(ordered.length-1,index+(event.key==='ArrowDown'?1:-1)))];
        if(next){choose(next,event);fileArea.querySelector<HTMLElement>(`[data-drive-uid="${CSS.escape(next.uid)}"]`)?.focus({preventScroll:false});}
      } else if(event.key==='ContextMenu'||(event.shiftKey&&event.key==='F10')) {
        event.preventDefault();const item=ordered[index];if(!item)return;if(!chosen.has(item.uid)){chosen=new Set([item.uid]);anchor=item.uid;}
        const rect=row.getBoundingClientRect();menu={x:rect.left+24,y:rect.top+24,items:chosenItems};
      }
    }
  }
  async function trashSelection(items: DriveItem[]) {
    const owned=items.filter(i=>i.permission==='owner'); if(owned.length!==items.length || !items.length) return;
    if(!await requestConfirmation({title:`Move ${items.length} ${items.length===1?'item':'items'} to trash?`,description:'Their contents will be hidden from shared users and guest links revoked. You can restore them later.',confirmLabel:'Move to trash',tone:'danger'})) return;
    await action(async()=>{const failures:string[]=[]; for(const item of items) { try { await driveChange(item,'trash'); } catch(reason) { failures.push(`${item.name}: ${reason instanceof Error?reason.message:'Could not trash item'}`); } } chosen=new Set(); if(failures.length) { await load(true); throw new Error(failures.join('\n')); } });
  }
  function startSelection(event: PointerEvent) {
    if(event.button!==0 || event.pointerType==='touch' || (event.target as Element).closest('.file-row,button,input,.file-head') || menu) return;
    event.preventDefault(); endMarquee?.(); fileArea.focus({preventScroll:true});
    const x=event.clientX,y=event.clientY; const original=event.ctrlKey||event.metaKey?new Set(chosen):new Set<string>(); chosen=new Set(original);
    const move=(e:PointerEvent)=>{if(e.pointerId!==event.pointerId)return; const box={left:Math.min(x,e.clientX),top:Math.min(y,e.clientY),right:Math.max(x,e.clientX),bottom:Math.max(y,e.clientY)};
      marquee={left:box.left,top:box.top,width:box.right-box.left,height:box.bottom-box.top}; const next=new Set(original);
      for(const row of fileArea.querySelectorAll<HTMLElement>('[data-drive-uid]')) if(intersects(box,row.getBoundingClientRect())) next.add(row.dataset.driveUid!); chosen=next;
    };
    const finish=()=>{document.removeEventListener('pointermove',move);document.removeEventListener('pointerup',finish);document.removeEventListener('pointercancel',finish);window.removeEventListener('blur',finish);marquee=null;endMarquee=undefined;};
    endMarquee=finish; document.addEventListener('pointermove',move);document.addEventListener('pointerup',finish);document.addEventListener('pointercancel',finish);window.addEventListener('blur',finish);
  }
  function dragOver(event: DragEvent) { if(!canUpload || dialog || !event.dataTransfer?.types.includes('Files')) return; event.preventDefault(); event.dataTransfer.dropEffect='copy'; dropping=true; }
  async function drop(event: DragEvent, destination = parent) {
    if(!canUpload || !event.dataTransfer?.types.includes('Files'))return;event.preventDefault();event.stopPropagation();dropping=false;dragDepth=0;
    try { const result=await droppedFiles(event.dataTransfer); await queueFiles(result.files,undefined,result.folders,destination); } catch(reason) { error=reason instanceof Error?reason.message:'Could not read dropped files.'; }
  }
</script>

<section class="drive" bind:this={root} aria-label="MX Drive" ondragover={dragOver} ondragenter={(event)=>{if(canUpload&&event.dataTransfer?.types.includes('Files')){dragDepth++;dropping=true;}}} ondragleave={()=>{if(--dragDepth<=0){dragDepth=0;dropping=false;}}} ondrop={(event)=>void drop(event)}>
  <header class="drive-heading"><div class="title"><HardDrive size={27} /><div><h1>MX Drive</h1><p>Your files, organized and shared within MX.</p></div></div><div class="top-actions"><button disabled={!canUpload || preparing} onclick={() => { label = ''; dialogError = ''; dialog = 'folder'; }}><FolderPlus size={17} />New folder</button><button disabled={!canUpload || preparing} onclick={() => folderInput.click()}><FolderInput size={17} />Upload folder</button><button class="emphasis" disabled={!canUpload || preparing} onclick={() => fileInput.click()}><Upload size={17} />Upload files</button></div></header>
  <input class="hidden" type="file" multiple bind:this={fileInput} onchange={(event) => { void queueFiles(event.currentTarget.files || []); event.currentTarget.value = ''; }} />
  <input class="hidden" type="file" multiple webkitdirectory bind:this={folderInput} onchange={(event) => { void queueFiles(event.currentTarget.files || []); event.currentTarget.value = ''; }} />
  <input class="hidden" type="file" bind:this={versionInput} onchange={(event) => { const file = event.currentTarget.files?.[0]; if (file && versionTarget) void queueFiles([file], versionTarget); event.currentTarget.value = ''; }} />
  <div class="drive-shell"><aside class="drive-sidebar"><nav aria-label="Drive views">{#each tabs as entry}<button class:active={tab === entry.id} aria-current={tab === entry.id ? 'page' : undefined} onclick={() => navigate(null, entry.id)}>{#if entry.id === 'mine'}<HardDrive size={18} />{:else if entry.id === 'shared'}<Share2 size={18} />{:else if entry.id === 'recent'}<History size={18} />{:else if entry.id === 'starred'}<Star size={18} />{:else}<Trash2 size={18} />{/if}{entry.label}</button>{/each}</nav>
    <section class="storage-summary" aria-label="Drive storage"><div><HardDrive size={17} /><strong>Storage</strong></div>{#if page}<progress aria-label="Storage used" value={storagePercent} max="100"></progress><p><strong>{driveSize(page.used_bytes)}</strong> of {driveSize(page.quota_bytes)} used</p><small>{page.quota_assigned ? 'Assigned by your administrator' : 'Default account allowance'}{page.reserved_bytes ? ` · ${driveSize(page.reserved_bytes)} uploading` : ''}</small>{#if page.used_bytes >= page.quota_bytes}<p class="storage-warning">Storage is full. Empty trash or ask your administrator for more space.</p>{/if}<small>Includes versions and trash</small>{:else}<small>Loading storage…</small>{/if}</section>
  </aside><div class="drive-content"><div class="drive-toolbar"><form onsubmit={(event) => { event.preventDefault(); offset = 0; void load(); }}><Search size={18} /><input aria-label="Search MX Drive" placeholder="Search files and folders…" bind:value={query} /><button type="submit" title="Search Drive" aria-label="Search Drive"><Search size={16} /></button></form></div>
  <div class="drive-location"><div class="breadcrumbs"><button onclick={() => navigate(null)}>{tabs.find((entry) => entry.id === tab)?.label}</button>{#each page?.breadcrumbs || [] as crumb}<ChevronRight size={13} /><button onclick={() => navigate(crumb.uid)}>{crumb.name}</button>{/each}</div><div class="view-actions"><button title="Refresh Drive" aria-label="Refresh Drive" disabled={loading} onclick={() => void load()}><RefreshCw size={16} /></button><button title={grid ? 'Switch to list' : 'Switch to grid'} aria-label={grid ? 'Switch to list' : 'Switch to grid'} onclick={() => grid = !grid}>{#if grid}<List size={16} />{:else}<LayoutGrid size={16} />{/if}</button>{#if tab === 'trash'}<button class="danger" disabled={busy || !page?.items.length} onclick={() => void purge()}>Empty trash</button>{/if}</div></div>
  {#if notice}<div class="message" role="status">{notice}<button aria-label="Dismiss notice" onclick={() => notice = ''}><X size={15} /></button></div>{/if}
  {#if error}<div class="message error" role="alert">{error}<button onclick={() => void load()}>Retry</button></div>{/if}
  {#if preparing}<p role="status">Preparing folder structure… Files already queued will continue uploading.</p>{/if}
  <div class="selection-toolbar" aria-label="File selection actions">
    <label><input type="checkbox" aria-label="Select all files on this page" checked={allChosen} indeterminate={chosenItems.length>0&&!allChosen} onchange={selectAll} /><span>{chosenItems.length ? `${chosenItems.length} selected` : 'Select files'}</span></label>
    {#if chosenItems.length}
      <button title="Copy (Ctrl/Cmd+C)" disabled={tab==='trash'} onclick={()=>stage(chosenItems,'copy')}><Copy size={16}/>Copy</button>
      <button title="Cut (Ctrl/Cmd+X)" disabled={tab==='trash'||chosenItems.some(i=>i.permission!=='owner')} onclick={()=>stage(chosenItems,'move')}><Scissors size={16}/>Cut</button>
      <button disabled={tab==='trash'||busy||chosenItems.some(i=>i.permission!=='owner')} onclick={()=>moving=chosenItems}><FolderInput size={16}/>Move</button>
      <button class="danger" disabled={tab==='trash'||busy||chosenItems.some(i=>i.permission!=='owner')} onclick={()=>void trashSelection(chosenItems)}><Trash2 size={16}/>Trash</button>
      <button aria-label="Clear selection" onclick={()=>chosen=new Set()}><X size={16}/></button>
    {/if}
    {#if uploads.clipboard && canUpload}<button disabled={busy} title="Paste (Ctrl/Cmd+V)" onclick={()=>void paste()}><ClipboardPaste size={16}/>Paste {uploads.clipboard.items.length} {uploads.clipboard.action==='move'?'to move':'to copy'}</button>{/if}
  </div>
  <div class="listing-summary"><span aria-live="polite">{page?.total.toLocaleString()||0} {page?.total===1?'item':'items'}{chosenItems.length?` · ${chosenItems.length} selected on this page`:''}</span><label>Sort<select aria-label="Sort Drive files" bind:value={sort} onchange={()=>pageTo(0)}><option value="modified_desc">Modified · newest</option><option value="modified_asc">Modified · oldest</option><option value="name_asc">Name · A–Z</option><option value="name_desc">Name · Z–A</option></select></label></div>
  <p class="selection-hint">Ctrl/Cmd-click to add · Shift-click for a range · Double-click to open · Drag empty space to select</p>
  <div class="files" class:grid bind:this={fileArea} role="listbox" tabindex="0" aria-multiselectable="true" aria-label="Drive files" aria-busy={loading} onpointerdown={startSelection} oncontextmenu={(event)=>context(event)}>
    {#if !grid}<div class="file-head" aria-hidden="true"><span></span><span>Name</span><span class="modified">Modified</span><span class="size">Size</span><span>Actions</span></div>{/if}
    {#each page?.items || [] as item (item.uid)}
      <div class="file-row" class:chosen={chosen.has(item.uid)} class:cut={uploads.clipboard?.action==='move'&&uploads.clipboard.items.some(i=>i.uid===item.uid)} role="option" aria-selected={chosen.has(item.uid)} tabindex="0" data-drive-uid={item.uid} onclick={(event)=>{if(!(event.target as Element).closest('.file-actions,input'))choose(item,event);}} ondblclick={(event)=>{if(!(event.target as Element).closest('.file-actions,input'))open(item);}} oncontextmenu={(event)=>context(event,item)} onkeydown={(event)=>{if(event.target!==event.currentTarget)return;if(event.key==='Enter'){event.preventDefault();open(item);}else if(event.key===' '){event.preventDefault();choose(item,event,true);}}} ondrop={(event)=>{if(item.kind==='folder'&&item.permission==='owner')void drop(event,item.uid);}}><input class="row-check" type="checkbox" aria-label={`Select ${item.name}`} checked={chosen.has(item.uid)} onclick={(event)=>{event.stopPropagation();choose(item,event,true);}} /><button class="file-name" title="Click to select; double-click to open" onclick={(event)=>{if(event.detail===0)open(item);}}><DriveFileIcon {item} large={grid && item.kind === 'file'} /><span><strong>{item.name}</strong><small>{item.kind === 'folder' ? 'Folder' : item.mime_type || 'File'}{item.permission !== 'owner' ? ` · ${item.permission}` : ''}</small></span></button><time class="modified" datetime={new Date(item.updated_at).toISOString()}>{date(item.updated_at)}</time><span class="size">{item.kind === 'folder' ? '—' : driveSize(item.size)}</span>
        <div class="file-actions">{#if tab === 'trash'}<button aria-label={`Restore ${item.name}`} title="Restore" disabled={busy} onclick={() => void action(() => driveChange(item, 'restore'))}><RotateCcw size={16} /></button><button class="danger" aria-label={`Delete permanently ${item.name}`} title="Delete permanently" disabled={busy} onclick={() => void purge(item)}><Trash2 size={16} /></button>
        {:else}<button aria-label={`${item.starred ? 'Unstar' : 'Star'} ${item.name}`} title={item.starred ? 'Unstar' : 'Star'} class:starred={item.starred} disabled={busy} onclick={() => void action(() => driveStar(item))}><Star size={15} fill={item.starred ? 'currentColor' : 'none'} /></button>
        {#if item.kind === 'file'}{#if canOpenOffice(item)}<button aria-label={`${canEditOffice(item)?'Edit':'Open'} ${item.name} in MX Office`} title={canEditOffice(item)?'Edit in MX Office':'Open in MX Office'} onclick={()=>launchOffice(item)}><Pencil size={16}/></button>{/if}<button aria-label={`Download ${item.name}`} title="Download" onclick={() => void action(() => driveDownload(item))}><Download size={16} /></button>{/if}
        <button aria-label={`More actions for ${item.name}`} title="More actions" disabled={busy} onclick={(event)=>context(event,item,true)}><MoreHorizontal size={18}/></button>{/if}</div>
      </div>
    {:else}<div class="empty"><HardDrive size={36} /><h2>{loading ? 'Loading Drive…' : tab === 'trash' ? 'Trash is empty' : 'No files here yet'}</h2><p>{tab === 'trash' ? 'Trashed files stay in your quota until permanently removed.' : tab === 'shared' ? 'Files shared with your MX account will appear here.' : 'Upload files or a folder to get started. You can also attach Drive files to records and conversations.'}</p>{#if canUpload}<button onclick={() => fileInput.click()}><Upload size={17} />Upload files</button>{/if}</div>{/each}
    <div class="selection-space" aria-hidden="true"></div>
  </div>
  {#if dropping}<div class="drop-overlay" role="status"><Upload size={32}/><strong>Drop to upload to {page?.breadcrumbs.at(-1)?.name || 'My Drive'}</strong><span>Files and folder structure are kept</span></div>{/if}
  <footer class="drive-page-footer"><DrivePagination total={page?.total||0} {offset} limit={pageSize} {loading} onPage={pageTo} onLimit={(value)=>{pageSize=value;pageTo(0);}}/></footer></div></div>
</section>

{#if marquee}<div class="selection-marquee" style:left={`${marquee.left}px`} style:top={`${marquee.top}px`} style:width={`${marquee.width}px`} style:height={`${marquee.height}px`}></div>{/if}
{#if menu}
  {@const targets=menu.items}
  {@const item=targets.length===1?targets[0]:null}
  <div class="drive-menu" use:driveMenu={{...menu,close:()=>menu=null}} role="menu" aria-label="Drive actions">
    {#if targets.length}<div class="menu-caption">{targets.length===1?targets[0].name:`${targets.length} items selected`}</div>
      {#if tab!=='trash'}
        {#if item}<button role="menuitem" onclick={()=>{open(item);menu=null;}}><Search size={16}/>{item.kind==='folder'?'Open folder':'Preview'}</button>{#if canOpenOffice(item)}<button role="menuitem" onclick={()=>{launchOffice(item);menu=null;}}><Pencil size={16}/>{canEditOffice(item)?'Edit in MX Office':'Open in MX Office'}</button>{/if}{/if}
        <button role="menuitem" onclick={()=>stage(targets,'copy')}><Copy size={16}/>Copy<kbd>Ctrl/Cmd+C</kbd></button>
        <button role="menuitem" disabled={targets.some(i=>i.permission!=='owner')} onclick={()=>stage(targets,'move')}><Scissors size={16}/>Cut<kbd>Ctrl/Cmd+X</kbd></button>
        <button role="menuitem" disabled={targets.some(i=>i.permission!=='owner')} onclick={()=>{moving=targets;menu=null;}}><FolderInput size={16}/>Move to folder</button>
        {#if item}<hr/>{#if item.kind==='file'}<button role="menuitem" onclick={()=>{void action(()=>driveDownload(item));menu=null;}}><Download size={16}/>Download</button>{/if}
          {#if item.permission!=='viewer'}<button role="menuitem" onclick={()=>{edit(item);menu=null;}}><Pencil size={16}/>Rename</button>{/if}
          {#if item.permission==='owner'}<button role="menuitem" onclick={()=>{void share(item);menu=null;}}><Share2 size={16}/>Share</button>{/if}
          <button role="menuitem" onclick={()=>{void showHistory(item);menu=null;}}><History size={16}/>{item.kind==='file'?'Versions and activity':'Activity'}</button>
        {/if}<hr/><button role="menuitem" class="danger" disabled={targets.some(i=>i.permission!=='owner')} onclick={()=>{void trashSelection(targets);menu=null;}}><Trash2 size={16}/>Move to trash</button>
      {:else}{#if item}<button role="menuitem" onclick={()=>{void action(()=>driveChange(item,'restore'));menu=null;}}><RotateCcw size={16}/>Restore</button><button role="menuitem" class="danger" onclick={()=>{void purge(item);menu=null;}}><Trash2 size={16}/>Delete permanently</button>{/if}{/if}
    {:else}<div class="menu-caption">{page?.breadcrumbs.at(-1)?.name||'My Drive'}</div><button role="menuitem" disabled={!canUpload} onclick={()=>{label='';dialogError='';dialog='folder';menu=null;}}><FolderPlus size={16}/>New folder</button><button role="menuitem" disabled={!canUpload} onclick={()=>{fileInput.click();menu=null;}}><Upload size={16}/>Upload files</button><button role="menuitem" disabled={!canUpload} onclick={()=>{folderInput.click();menu=null;}}><FolderInput size={16}/>Upload folder</button>{/if}
    {#if canUpload}<hr/><button role="menuitem" disabled={!uploads.clipboard||busy} onclick={()=>{void paste(item?.kind==='folder'?item.uid:parent);menu=null;}}><ClipboardPaste size={16}/>{item?.kind==='folder'?'Paste into folder':'Paste here'}<kbd>Ctrl/Cmd+V</kbd></button>{/if}
  </div>
{/if}

{#if dialog}
  <div class="drive-modal-backdrop" inert={!!preview} aria-hidden={preview ? 'true' : undefined}><div class="drive-modal" use:focusDialog role="dialog" aria-modal="true" aria-label={dialog === 'share' ? 'Share file or folder' : dialog === 'history' ? 'Versions and activity' : dialog === 'folder' ? 'New folder' : 'Rename item'} tabindex="-1" onkeydown={(event) => { if (event.key === 'Escape' && !busy && !(event.target instanceof HTMLSelectElement)) dialog = null; }}>
    <header><div><h2>{dialog === 'share' ? 'Share' : dialog === 'history' ? 'Versions and activity' : dialog === 'folder' ? 'New folder' : 'Rename'}</h2>{#if selected && dialog !== 'folder'}<p>{selected.name}</p>{/if}</div><button aria-label="Close dialog" disabled={busy} onclick={() => dialog = null}><X size={19} /></button></header>
    {#if dialogError}<p class="dialog-error" role="alert">{dialogError}</p>{/if}
    {#if dialog === 'folder' || dialog === 'rename'}<form class="name-form" onsubmit={(event) => { event.preventDefault(); void saveName(); }}><label>Name<input required maxlength="240" bind:value={label} /></label><p class="hint">{dialog === 'rename' ? 'Only the MX display name changes. The N1 object key keeps its original extension.' : 'Folder structure is managed by MX; N1 stores file objects by unique key.'}</p><footer><button type="button" disabled={busy} onclick={() => dialog = null}>Cancel</button><button class="emphasis" disabled={busy || !label.trim()}>{busy ? 'Saving…' : 'Save'}</button></footer></form>
    {:else if dialog === 'share'}
      <div class="dialog-body sharing-body"><h3>Who has access</h3><p class="hint">Viewer: preview and download. Editor: rename and add file versions. Only you can move, delete or manage sharing.</p>
      <div class="owner-row"><ShieldCheck size={20}/><span><strong>You</strong><small>File owner · full control</small></span><span>Owner</span></div>
      {#if !sharing}<p role="status">Loading access…</p>{/if}
      {#each sharing?.users || [] as user}<div class="access-row"><span class="access-identity"><strong>{user.name}</strong><small title={user.email}>{user.email}</small></span><select aria-label={`Access for ${user.name}`} value={user.role} disabled={busy} onchange={(event) => void updateGrant(user.user_uid, event.currentTarget.value as 'viewer' | 'editor')}><option value="viewer">Viewer</option><option value="editor">Editor</option></select><button aria-label={`Remove access for ${user.name}`} title="Remove access" disabled={busy} onclick={() => void updateGrant(user.user_uid, null)}><X size={16} /></button></div>{/each}
      {#each sharing?.spaces || [] as space}<div class="access-row"><span class="access-identity"><strong><Users size={15}/>{space.name}</strong><small>{space.archived?'Archived · no access':`${space.members} members · collaboration space`}</small></span><select aria-label={`Space access for ${space.name}`} value={space.role} disabled={busy||space.archived} onchange={(event)=>void updateSpace(space.space_uid,event.currentTarget.value as 'viewer'|'editor')}><option value="viewer">Viewer</option><option value="editor">Editor</option></select><button aria-label={`Remove space access for ${space.name}`} title="Remove space access" disabled={busy} onclick={()=>void updateSpace(space.space_uid,null)}><X size={16}/></button></div>{/each}
      {#if sharing&&!sharing.users.length&&!sharing.spaces.length}<p class="sharing-empty">Private · only you have direct access.</p>{/if}
      {#if sharing?.inherited.length}<h3>Inherited from folders</h3>{#each sharing.inherited as entry}<div class="inherited-row"><span><strong>{entry.name}</strong><small>{entry.detail} · from {entry.from}</small></span><span>{entry.role==='editor'?'Editor':'Viewer'}</span></div>{/each}<p class="hint">Change inherited access on the parent folder.</p>{/if}
      <h3>Add people or spaces</h3><div class="share-targets" role="group" aria-label="Share target"><button aria-pressed={shareTarget==='people'} class:active={shareTarget==='people'} onclick={()=>switchShareTarget('people')}>People</button><button aria-pressed={shareTarget==='spaces'} class:active={shareTarget==='spaces'} onclick={()=>switchShareTarget('spaces')}><Users size={16}/>Spaces & groups</button></div>
      <p class="hint">{shareTarget==='spaces'?'Share with a space you belong to. Current and future members inherit access; leaving removes that access.':'Find an MX account by name or email.'}</p>
      <form class="sharing-search" onsubmit={(event) => { event.preventDefault(); void findPeople(); }}><input aria-label={shareTarget==='people'?'Find an MX account':'Find a collaboration space'} placeholder={shareTarget==='people'?'Name or email…':'Space or group name…'} bind:value={peopleQuery}/><button disabled={finding||busy}>{finding?'Finding…':'Find'}</button></form>
      <label class="role">Access to grant<select aria-label="Access to grant" bind:value={shareRole}><option value="viewer">Viewer</option><option value="editor">Editor</option></select></label>
      {#each people as user}<div class="share-result"><span class="access-identity"><strong>{user.name}</strong><small title={user.email}>{user.email}</small></span><button aria-label={`Share with ${user.name}`} disabled={busy} onclick={() => void updateGrant(user.uid, shareRole)}>Share</button></div>{/each}
      {#each spaces as space}<div class="share-result"><span class="access-identity"><strong><Users size={15}/>{space.name}</strong><small>{space.members} members · {space.kind==='group'?'Group':'Space'}</small></span><button aria-label={`Share with space ${space.name}`} disabled={busy} onclick={()=>void updateSpace(space.uid,shareRole)}>Share</button></div>{/each}
      {#if found&&!people.length&&!spaces.length}<p class="sharing-empty">No matching {shareTarget==='people'?'accounts':'spaces'} without existing access. Try another name.</p>{/if}
      <h3>Public guest links</h3><p class="hint">Anyone holding a link can preview and download this item, including its folder contents. No MX account is required. Links do not grant edit access.</p>
      {#if page?.public_links}<div class="link-create"><select aria-label="Guest link expiry" bind:value={linkDays}><option value="1">Expires in 1 day</option><option value="7">Expires in 7 days</option><option value="30">Expires in 30 days</option><option value="0">No expiration</option></select><button disabled={busy} onclick={() => void createLink()}><Link size={15} />Create link</button></div>{:else}<p>Public links are disabled by the administrator.</p>{/if}
      {#if newLink}<label>New guest link<input readonly value={newLink} onclick={(event) => event.currentTarget.select()} /></label><button onclick={() => { void navigator.clipboard.writeText(newLink).then(() => notice = 'Guest link copied.').catch(() => dialogError = 'Select and copy the link above.'); }}>Copy link</button>{/if}
      {#each sharing?.links || [] as link}<div class="person"><span>Guest link<small>{link.expires_at ? `Expires ${date(link.expires_at)}` : 'No expiration'} · created {date(link.created_at)}</small></span><button disabled={busy} onclick={() => void revoke(link.uid)}>Revoke</button></div>{/each}
      <p class="hint">The secret URL is shown only when created. Create a new link if you did not save it. Moving to trash revokes public links permanently.</p></div>
    {:else if dialog === 'history' && selected}
      <div class="history-tabs" role="tablist" aria-label="File details">{#if selected.kind==='file'}<button id="drive-versions-tab" role="tab" aria-selected={historyTab==='versions'} aria-controls="drive-history-panel" class:active={historyTab==='versions'} onclick={()=>historyTab='versions'}>Versions <span>{history?.versions.length ?? '…'}</span></button>{/if}<button id="drive-activity-tab" role="tab" aria-selected={historyTab==='activity'} aria-controls="drive-history-panel" class:active={historyTab==='activity'} onclick={()=>historyTab='activity'}>Activity</button></div>
      <div class="dialog-body history-body" id="drive-history-panel" role="tabpanel" aria-labelledby={historyTab==='versions'?'drive-versions-tab':'drive-activity-tab'}>
        {#if !history && !dialogError}<p class="history-empty" role="status">Loading {historyTab}…</p>{:else if historyTab==='versions'}
          <div class="history-intro"><div><h3>File versions</h3><p class="hint">Preview an older version or restore it as the latest. Nothing in your existing history is overwritten.</p></div>{#if selected.permission!=='viewer'}<button class="emphasis" onclick={()=>{versionTarget=selected;versionInput.click();}}><Upload size={16}/>New version</button>{/if}</div>
          {#each history?.versions||[] as version,index}<div class="version" class:current={index===0}><div class="version-icon"><History size={20}/></div><span><strong>{index===0?'Current version':`Earlier version · ${date(version.created_at)}`}</strong><small>{index===0?date(version.created_at):version.file_name}</small><small>{driveSize(version.size)} · {version.mime_type || 'File'}</small></span><div class="version-actions"><button title="Preview version" aria-label={`Preview ${index===0?'current':'earlier'} version`} onclick={()=>{preview=selected;previewVersion=version.uid;}}><Search size={15}/>Preview</button><button title="Download version" aria-label="Download version" onclick={()=>selected&&void action(()=>driveDownload(selected!,version.uid))}><Download size={15}/></button>{#if index>0&&selected.permission!=='viewer'}<button title="Restore as a new version" onclick={()=>void restoreVersion(version.uid)}><RotateCcw size={15}/>Restore</button>{/if}</div></div>{:else}<p class="history-empty">No saved file versions yet.</p>{/each}
        {:else}<h3>Recent activity</h3><p class="hint">Changes to this item, newest first.</p><ol class="activity-list">{#each history?.activity||[] as activity}<li class="activity"><span class="activity-dot"></span><div><strong>{activity.actor}</strong><p>{activityLabel(activity.action)}</p><time datetime={new Date(activity.created_at).toISOString()}>{date(activity.created_at)}</time></div></li>{:else}<li class="history-empty">No activity yet.</li>{/each}</ol>{/if}
      </div>
    {/if}
  </div></div>
{/if}
{#if moving}<DrivePicker foldersOnly excludeUids={moving.map(i=>i.uid)} onChooseFolder={(uid) => { const items = moving!; moving = null; void action(() => driveTransfer(items, 'move', uid),`${items.length} items moved.`); }} onClose={() => moving = null} />{/if}
{#if preview}{#key `${preview.uid}:${previewVersion || ''}`}{#if previewVersion}<FilePreview fileName={preview.name} load={()=>drivePreview(preview!,previewVersion)} onClose={()=>{preview=null;previewVersion=undefined;}}/>{:else}<DrivePreview item={preview} view={tab} {parent} {query} {sort} onNavigate={(item)=>preview=item} onClose={()=>preview=null}/>{/if}{/key}{/if}

<style>
  .drive { padding: 1.2rem var(--shell-gutter, 1rem); position:relative; }
  .drive, .drive-modal, .drive-menu { --surface: var(--collab-bg); --surface-2: var(--collab-surface); --text: var(--collab-text); --muted: var(--collab-muted); --line: var(--collab-line); --line-strong: var(--collab-selected-border); }
  .drive-shell { display: grid; grid-template-columns: 12.5rem minmax(0,1fr); gap: 1.5rem; }
  .drive-sidebar { min-width: 0; } .drive-sidebar nav { display: grid; gap: .3rem; } .drive-sidebar nav button { justify-content: flex-start; gap: .8rem; min-height: 2.75rem; border-radius: .7rem; padding: .6rem .8rem; }
  .drive-content { min-width: 0; border: 1px solid var(--line); border-radius: .9rem; padding: 1rem 1.2rem 0; background: var(--surface); }
  .storage-summary { margin-top: 1.4rem; padding: 1.2rem .8rem; border-top: 1px solid var(--line); } .storage-summary > div { display: flex; align-items: center; gap: .6rem; font-size: .85rem; } .storage-summary progress { margin: 1rem 0 .4rem; height: .35rem; } .storage-summary p { font-size: .78rem; line-height: 1.5; } .storage-summary small { font-size: .7rem; margin-top: .3rem; } .storage-summary .storage-warning { color: var(--danger); }
  @media(max-width:900px) { .drive-shell { grid-template-columns: minmax(0,1fr); gap: .8rem; } .drive-sidebar { display: flex; flex-direction: column; } .drive-sidebar nav { display: flex; flex-wrap: wrap; gap: .3rem; } .drive-sidebar nav button { flex: 1 1 auto; font-size: .8rem; min-height: 2.4rem; gap: .45rem; } .storage-summary { margin: .7rem 0 0; padding: .7rem; border: 1px solid var(--line); border-radius: .7rem; } .storage-summary > div { display: none; } .storage-summary progress { margin: 0 0 .3rem; } .storage-summary small { display: inline; margin-right: .4rem; } .drive-content { padding: .8rem .7rem 0; } }
  .drive .file-row { border-radius: 0; background: transparent; }
  .drive .grid .file-name { flex-direction: column; align-items: stretch; gap: .7rem; width: 100%; } .drive .grid .modified { margin-left: 0; }
  .drive { color: var(--text); min-width: 0; width: 100%; } h1 { margin: 0; font-size: 1.5rem; } h2 { margin: 0; font-size: 1.15rem; } h3 { margin: 1.2rem 0 .6rem; font-size: .95rem; } p { color: var(--muted); margin: .3rem 0; font-size: .9rem; }
  .drive-heading, .title, .top-actions, .drive-toolbar, .drive-location, .view-actions, .breadcrumbs { display: flex; align-items: center; gap: .6rem; } .drive-heading { justify-content: space-between; flex-wrap: wrap; margin-bottom: 1.4rem; } .title { gap: .9rem; } .title > :global(svg) { color: var(--collab-icon); } .top-actions { flex-wrap: wrap; }
  button { display: inline-flex; align-items: center; justify-content: center; gap: .4rem; padding: .5rem .65rem; border: 1px solid var(--line); border-radius: .4rem; background: var(--surface); color: var(--text); font: inherit; font-size: .85rem; cursor: pointer; min-height: 2.2rem; } button:hover:not(:disabled) { background: var(--surface-2); border-color: var(--line-strong); } button:disabled { opacity: .45; cursor: default; } button:focus-visible, input:focus-visible, select:focus-visible { outline: 2px solid var(--collab-selected-border); outline-offset: 2px; } button.emphasis, button.active { background: var(--collab-selected); color: var(--collab-selected-text); border-color: var(--collab-selected-border); } button.danger { color: var(--danger); } button.starred { color: var(--warning); } .hidden { display: none; }
  .drive-toolbar { justify-content: space-between; flex-wrap: wrap; padding-bottom: .8rem; border-bottom: 1px solid var(--line); } nav { display: flex; flex-wrap: wrap; gap: .3rem; } nav button { border-color: transparent; background: transparent; } .drive-toolbar form { display: flex; align-items: center; gap: .5rem; min-width: 0; width: min(23rem, 100%); padding-left: .6rem; border: 1px solid var(--line); background: var(--surface); border-radius: .4rem; } .drive-toolbar input { border: 0; padding: .5rem 0; background: transparent; } .drive-toolbar form button { border: 0; }
  input, select { font: inherit; font-size: .9rem; background: var(--surface); color: var(--text); border: 1px solid var(--line); border-radius: .4rem; padding: .5rem .6rem; min-width: 0; } input { width: 100%; } .drive-location { justify-content: space-between; padding: .8rem 0; } .breadcrumbs { flex-wrap: wrap; min-width: 0; } .breadcrumbs button { border: 0; background: transparent; padding: .2rem .4rem; text-align: left; overflow-wrap: anywhere; } .view-actions { flex-shrink: 0; }
  .files { border: 1px solid var(--line); border-radius: .6rem; overflow: auto; background: var(--surface); max-height:clamp(18rem,calc(100dvh - 22rem),48rem); overscroll-behavior:contain; user-select:none; } .files:focus-visible { outline:2px solid var(--collab-selected-border); outline-offset:2px; }
  .file-head, .file-row { display: grid; grid-template-columns: 1.25rem minmax(0,1fr) 12rem 5rem 7.5rem; align-items: center; gap: .8rem; padding: .7rem 1rem; } .file-head { font-size: .75rem; color: var(--muted); background: var(--surface-2); font-weight: 600; } .file-head span:last-child { text-align:right; } .file-row + .file-row { border-top: 1px solid var(--line); } .drive .file-row:hover { background: var(--surface-2); } .drive .file-row.chosen { background:var(--collab-selected); box-shadow:inset 3px 0 var(--collab-selected-border); } .file-row.cut .file-name { opacity:.55; } .row-check { width:1rem; height:1rem; padding:0; margin:0; accent-color:var(--collab-icon); cursor:pointer; }
  .file-name { justify-content: flex-start; text-align: left; border: 0; background: transparent; padding: 0; min-width: 0; } .file-name span { min-width: 0; } .file-name strong { display: block; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; font-weight: 600; } small { display: block; color: var(--muted); font-size: .75rem; line-height: 1.5; } .modified, .size { color: var(--muted); font-size: .8rem; } .file-actions { display: flex; gap: .2rem; flex-wrap: nowrap; justify-content: flex-end; } .file-actions button { padding: .4rem; width:2.2rem; border-color: transparent; background: transparent; }
  .files.grid { display: grid; grid-template-columns: repeat(auto-fill,minmax(min(17rem,100%),1fr)); gap: .75rem; padding: .75rem; background: transparent; border: 0; align-content:start; } .grid .file-row { display: flex; flex-direction: column; align-items: stretch; border: 1px solid var(--line); border-radius: .6rem; background: var(--surface); gap: .6rem; padding: .9rem; } .grid .file-name { align-items: flex-start; } .grid .modified { margin-left: 3rem; } .grid .size { display: none; } .grid .file-actions { justify-content: flex-end; border-top: 1px solid var(--line); padding-top: .5rem; margin-top: auto; }
  .selection-toolbar { display:flex; align-items:center; flex-wrap:wrap; gap:.4rem; min-height:2.6rem; } .selection-toolbar label { display:flex; align-items:center; gap:.6rem; font-size:.8rem; margin-right:auto; } .selection-toolbar input { width:1rem; height:1rem; accent-color:var(--collab-icon); } .selection-toolbar button { font-size:.78rem; min-height:2rem; padding:.35rem .5rem; } .selection-hint { font-size:.72rem; margin:.35rem 0 .7rem; line-height:1.6; }
  .selection-marquee { position:fixed; pointer-events:none; z-index:90; border:1px solid var(--collab-selected-border); background:color-mix(in srgb,var(--collab-selected-border) 15%,transparent); }
  .selection-space { height:1.5rem; grid-column:1/-1; }
  .drop-overlay { position:absolute; inset:.5rem; border:2px dashed var(--collab-selected-border); border-radius:1rem; background:color-mix(in srgb,var(--collab-bg) 92%,transparent); pointer-events:none; z-index:20; display:flex; flex-direction:column; align-items:center; justify-content:center; gap:1rem; color:var(--collab-text); text-align:center; }
  .drive-menu { position:fixed; z-index:120; visibility:hidden; overflow:auto; padding:.35rem; min-width:15rem; border:1px solid var(--line); border-radius:.65rem; background:var(--surface); color:var(--text); box-shadow:0 .5rem 2rem #0003; } .drive-menu button { width:100%; justify-content:flex-start; border:0; background:transparent; text-align:left; gap:.65rem; font-size:.8rem; } .drive-menu kbd { margin-left:auto; padding-left:1rem; font:inherit; color:var(--muted); font-size:.65rem; } .drive-menu hr { border:0; border-top:1px solid var(--line); margin:.3rem; } .menu-caption { max-width:22rem; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; padding:.5rem .65rem; font-size:.75rem; color:var(--muted); }
  .empty { grid-column: 1 / -1; text-align: center; padding: 2rem 1rem; color: var(--muted); display: flex; flex-direction: column; align-items: center; gap: .7rem; } .empty p { max-width: 32rem; }
  .message { display: flex; align-items: center; justify-content: space-between; gap: .6rem; background: var(--surface-2); border: 1px solid var(--line); padding: .7rem; border-radius: .4rem; margin: .5rem 0; font-size: .85rem; } .message.error, .dialog-error { color: var(--danger); } progress { width: 100%; height: .3rem; accent-color: var(--collab-icon); }
  .drive-modal-backdrop { position: fixed; inset: 0; background: #0008; z-index: 110; display: grid; place-items: center; padding: 1rem; } .drive-modal { width: min(40rem,100%); max-height: 90dvh; display:flex; flex-direction:column; overflow:hidden; background: var(--surface); color: var(--text); border: 1px solid var(--line); border-radius: .8rem; box-shadow: 0 1rem 4rem #0004; } .drive-modal header, .drive-modal footer { display: flex; align-items: center; justify-content: space-between; gap: .5rem; padding: 1rem; border-bottom: 1px solid var(--line); flex-shrink:0; } .drive-modal footer { justify-content: flex-end; padding: 1rem 0 0; border: 0; } .drive-modal header>div {min-width:0;} .drive-modal header p {overflow:hidden;text-overflow:ellipsis;white-space:nowrap;} .drive-modal header button {flex-shrink:0;} .name-form, .dialog-body { padding: 0 1rem 1rem; overflow:auto;min-height:0;overscroll-behavior:contain; } .name-form { padding-top: 1rem; } .drive-modal label { display: grid; gap: .4rem; margin: .6rem 0; font-size: .85rem; font-weight: 500; } .hint { font-size: .8rem; line-height: 1.6; } .dialog-error { padding: .7rem 1rem; } .person, .version { display: flex; align-items: center; gap: .5rem; padding: .7rem 0; border-bottom: 1px solid var(--line); } .person > span, .version > span { flex: 1; min-width: 0; overflow-wrap: anywhere; } .person button {flex-shrink:0;} .version strong { font-size: .85rem; } .link-create { display: flex; gap: .5rem; margin-top: .8rem; flex-wrap:wrap; } .link-create select {width:auto;max-width:100%;} .role { max-width: 11rem; } .activity { padding: .5rem 0; border-bottom: 1px solid var(--line); font-size: .8rem; }
  .access-row {display:grid;grid-template-columns:minmax(0,1fr) 8rem 2.25rem;gap:.65rem;align-items:center;padding:.85rem 0;border-bottom:1px solid var(--line);} .access-row select {width:100%;min-width:0;} .access-row button {padding:.4rem;} .access-identity {min-width:0;} .access-identity strong {display:flex;align-items:center;gap:.4rem;overflow-wrap:break-word;font-size:.85rem;} .access-identity strong :global(svg) {flex-shrink:0;} .access-identity small {overflow:hidden;text-overflow:ellipsis;white-space:nowrap;} .owner-row,.inherited-row {display:flex;align-items:center;gap:.75rem;padding:.8rem 0;border-bottom:1px solid var(--line);font-size:.85rem;} .owner-row>span:first-of-type,.inherited-row>span:first-child {flex:1;min-width:0;overflow-wrap:break-word;} .owner-row>span:last-child,.inherited-row>span:last-child {flex-shrink:0;color:var(--muted);} .owner-row> :global(svg) {color:var(--collab-icon);flex-shrink:0;} .sharing-empty {padding:.6rem 0;font-size:.8rem;} .share-targets,.sharing-search {display:flex;align-items:center;gap:.5rem;} .sharing-search {margin-top:.7rem;} .sharing-search input {flex:1;min-width:0;} .sharing-search button {flex-shrink:0;} .share-result {display:grid;grid-template-columns:minmax(0,1fr) auto;gap:.75rem;align-items:center;padding:.75rem 0;border-bottom:1px solid var(--line);} .share-result button {flex-shrink:0;}
  .listing-summary {display:flex;align-items:center;justify-content:space-between;gap:.75rem;flex-wrap:wrap;padding:.2rem 0 .35rem;font-size:.78rem;color:var(--muted);} .listing-summary label {display:flex;align-items:center;gap:.4rem;} .listing-summary select {width:auto;font-size:.78rem;padding:.35rem .4rem;} .file-head {position:sticky;top:0;z-index:2;} .drive-page-footer {border-top:1px solid var(--line);margin-top:.5rem;}
  .history-tabs { display:flex; gap:.4rem; padding:.75rem 1rem; border-bottom:1px solid var(--line); } .history-tabs span { font-size:.7rem; padding:.1rem .4rem; border-radius:1rem; background:var(--surface-2); } .history-body { padding-top:.5rem; } .history-intro { display:flex; align-items:center; gap:1rem; margin-bottom:1rem; } .history-intro>div { flex:1; } .history-intro h3 { margin:.5rem 0; } .history-intro button { flex-shrink:0; } .version { padding:1rem .6rem; gap:.7rem; border:1px solid var(--line); border-radius:.6rem; margin:.6rem 0; } .version.current { background:var(--surface-2); } .version-icon { color:var(--muted); } .version-actions { display:flex; flex-wrap:wrap; gap:.3rem; } .version-actions button { font-size:.75rem; min-height:2rem; } .activity-list { padding:0; list-style:none; } .activity { display:flex; gap:.8rem; padding:1rem 0; } .activity p { margin:.3rem 0; font-size:.85rem; color:var(--text); } .activity time { color:var(--muted); font-size:.75rem; } .activity-dot { width:.5rem; height:.5rem; border-radius:50%; margin-top:.35rem; flex-shrink:0; background:var(--collab-icon); } .history-empty { text-align:center; padding:2rem 1rem; color:var(--muted); }
  @media(max-width:1200px) { .file-head, .file-row { grid-template-columns:1.25rem minmax(0,1fr) 5rem 7.5rem; } .modified { display: none; } }
  @media(max-width:760px) { .drive-heading { gap: 1rem; } .top-actions { width: 100%; } .top-actions button { flex: 1; font-size: .75rem; padding: .5rem .3rem; } .drive-toolbar form { width: 100%; } nav { width: 100%; } nav button { font-size: .75rem; flex: 1 1 auto; } .file-head { display: none; } .file-row { grid-template-columns:1rem minmax(0,1fr) 2.2rem; gap:.5rem; padding:.75rem .5rem; } .size { display:none; } .file-actions button:not(:last-child) { display:none; } .file-actions { padding:0; } .grid .file-actions button { display:inline-flex; } .file-name strong { font-size:.8rem; } .file-name small { font-size:.68rem; } .drive-modal-backdrop { padding: .5rem; } .person, .version, .history-intro { flex-wrap:wrap; } .version-actions { width:100%; } .drive-location { align-items: flex-start; } .selection-hint { display:none; } .selection-toolbar { margin-bottom:.5rem; } .access-row {grid-template-columns:minmax(0,1fr) 6.2rem 2.1rem;gap:.4rem;} .access-row select {font-size:.78rem;padding:.4rem .3rem;} .files {max-height:55dvh;} }
</style>
