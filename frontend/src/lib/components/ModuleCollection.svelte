<script lang="ts">
 import {onDestroy,untrack,getContext} from 'svelte';
 import {writable,type Writable} from 'svelte/store';
 import type {FieldDefinition,FileAttachment,ModuleReadingMap,MxRecord,RecordPage} from '../api/domain';
 import type {ModuleView} from '../api/moduleViews';
 import {listRecords,previewAttachment,download} from '../api/workspace';
 import {recordFieldText} from '../util/moduleViews';
 import {fieldState} from '../util/formRules';
 import {fieldIsBold,moduleReadingContext} from '../util/readingPreferences';
 import FilePreview from './FilePreview.svelte';
 import RecordAttachmentThumbnail from './RecordAttachmentThumbnail.svelte';
 import {officeExtension} from '../office/formats';
 import {openRecordOffice} from '../office/launch';
 let {moduleUid,view,fields,revision=0,canAttachments=true,onOpen}:{moduleUid:string;view:ModuleView;fields:FieldDefinition[];revision?:number;canAttachments?:boolean;onOpen:(uid:string)=>void}=$props();
 const reading=getContext<Writable<ModuleReadingMap>>(moduleReadingContext)||writable<ModuleReadingMap>({});
 let result=$state<RecordPage|null>(null);let search=$state('');let applied=$state('');let page=$state(1);let loading=$state(false);let error=$state('');let preview=$state<{record:MxRecord;file:FileAttachment}|null>(null);
 let controller:AbortController|undefined;let generation=0;let dead=false;
 $effect(()=>{moduleUid;view.id;revision;untrack(()=>void reload());});
 onDestroy(()=>{dead=true;generation++;controller?.abort();});
 async function reload(){controller?.abort();const request=new AbortController();controller=request;const current=++generation;loading=true;error='';try{const next=await listRecords({page,limit:24,q:applied,attachments:view.kind==='documents'?'with':undefined},moduleUid,request.signal);if(!dead&&current===generation){result=next;if(page>Math.max(1,next.total_pages)){page=Math.max(1,next.total_pages);void reload();}}}catch(e){if(!request.signal.aborted&&!dead)error=e instanceof Error?e.message:String(e);}finally{if(current===generation)loading=false;}}
 function title(record:MxRecord){return view.title_field?recordFieldText(record,view.title_field,fields):recordFieldText(record,fields.find(f=>f.field_type!=='attachments')?.key||'',fields);}
 function text(record:MxRecord,key:string){const field=fields.find(f=>f.key===key);return field&&!fieldState(field,record.values).visible?'Not applicable':recordFieldText(record,key,fields);}
 const cardFields=$derived(view.fields.map(key=>fields.find(f=>f.key===key)).filter((f):f is FieldDefinition=>!!f&&f.field_type!=='attachments'));
</script>
<div class="collection-tools"><form role="search" onsubmit={e=>{e.preventDefault();applied=search.trim();page=1;void reload();}}><label class="sr-only" for="collection-search">Search this collection</label><input id="collection-search" type="search" bind:value={search} placeholder={view.kind==='documents'?'Search document names and record fields…':'Search this directory…'}/><button class="button" type="submit">Search</button></form><span role="status">{loading?'Loading…':`${result?.total.toLocaleString()??0} ${view.kind==='documents'?'records with files':'results'}`}</span><button class="button" disabled={loading} onclick={reload}>Refresh</button></div>
{#if error}<p class="notice error" role="alert">{error}</p>{/if}
{#if view.kind==='documents'&&!canAttachments}<p class="notice">Your account does not have attachment access for this module. Ask its administrator for permission.</p>{:else}
<div class="collection-grid" class:document-grid={view.kind==='documents'} aria-busy={loading}>
 {#each result?.data||[] as record (record.uid)}<article data-record-uid={record.uid}>
  {#if view.kind==='documents'}<div class="document-cover">{#if record.attached_files[0]}<RecordAttachmentThumbnail recordUid={record.uid} file={record.attached_files[0]} onOpen={()=>preview={record,file:record.attached_files[0]}}/>{/if}<span>{record.attached_files.length} file{record.attached_files.length===1?'':'s'}</span></div>{/if}
  <div class="collection-card-heading">{#if view.kind!=='documents'}<div class="directory-avatar" aria-hidden="true">{title(record).slice(0,2).toLocaleUpperCase()}</div>{/if}<button class="collection-title" class:bold={fieldIsBold($reading,moduleUid,fields.find(f=>f.key===view.title_field)?.uid||'')} onclick={()=>onOpen(record.uid)}>{title(record)}</button></div>
  <dl>{#each cardFields as field}<div class:bold={fieldIsBold($reading,moduleUid,field.uid)}><dt>{field.label}</dt><dd>{text(record,field.key)}</dd></div>{/each}</dl>
  {#if view.kind==='documents'}<ul class="document-files">{#each record.attached_files as file}<li><button class="file-link" onclick={()=>preview={record,file}}>{file.file_name}</button>{#if officeExtension(file.file_name)}<button class="button small" onclick={()=>{if(!openRecordOffice(record.uid,file.uid,moduleUid))error='Allow pop-ups for MX to open this document.';}}>Open in Office</button>{/if}</li>{/each}</ul>{/if}
  <button class="button card-open" onclick={()=>onOpen(record.uid)}>Open {view.kind==='documents'?'document record':'details'}</button>
 </article>{/each}
</div>
{#if !loading&&!result?.data.length}<div class="empty-state"><h2>{applied?'No matching results':view.kind==='documents'?'No documents yet':'Your directory is empty'}</h2><p>{applied?'Try another search.':view.kind==='documents'?'Create a record and attach files to begin.':'Create your first entry using the New button above.'}</p></div>{/if}
<footer><span>Page {page} of {Math.max(1,result?.total_pages||1)} · 24 records per page</span><div><button class="button" disabled={loading||page<=1} onclick={()=>{page--;void reload();}}>Previous</button><button class="button" disabled={loading||!result?.has_next} onclick={()=>{page++;void reload();}}>Next</button></div></footer>{/if}
{#if preview}{@const selected=preview}{#key selected.file.uid}<FilePreview fileName={selected.file.file_name} load={()=>previewAttachment(selected.record.uid,selected.file.uid,selected.file.file_name,selected.file.mime_type)} onClose={()=>preview=null} onDownloadOriginal={()=>download(`/mx/v1/records/${selected.record.uid}/attachments/${selected.file.uid}/download`,selected.file.file_name)} officeActions={officeExtension(selected.file.file_name)?{open:()=>{if(!openRecordOffice(selected.record.uid,selected.file.uid,moduleUid))error='Allow pop-ups for MX.';},download:()=>void download(`/mx/v1/records/${selected.record.uid}/attachments/${selected.file.uid}/download`,selected.file.file_name),description:'Opens the original attachment in MX Office. Attachment editing permissions remain unchanged.'}:undefined}/>{/key}{/if}
<style>
 .collection-tools{display:flex;gap:.65rem;align-items:center;flex-wrap:wrap;margin-bottom:.75rem}.collection-tools form{display:flex;gap:.4rem;flex:1;min-width:min(100%,14rem);max-width:40rem}.collection-tools input{width:100%;min-width:0;font-size:.8125rem}.collection-tools span{color:var(--muted);font-size:.8125rem}
 .collection-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(min(100%,18rem),1fr));gap:.75rem}.collection-grid article{padding:1rem;border:1px solid var(--line);border-radius:var(--card-radius);background:var(--surface);display:flex;flex-direction:column;min-width:0;box-shadow:0 1px 2px #00000003}.collection-grid article:focus-within{border-color:var(--line-strong)}
 .collection-card-heading{display:flex;align-items:center;gap:.65rem;min-height:2.25rem;margin-bottom:.25rem}.collection-title{font:inherit;font-size:.9375rem;font-weight:650;color:var(--text);text-align:left;border:0;background:none;padding:.15rem 0;cursor:pointer;overflow-wrap:anywhere}.collection-title:hover{text-decoration:underline}.directory-avatar{display:grid;place-items:center;flex:none;width:2.25rem;height:2.25rem;border-radius:var(--control-radius);background:var(--surface-3);color:var(--text-2);font-size:.75rem;font-weight:650}
 .document-cover{display:flex;justify-content:space-between;align-items:center;min-height:4.5rem;background:var(--surface-2);padding:.7rem;border-radius:var(--control-radius);margin-bottom:.7rem}.document-cover span{color:var(--muted);font-size:.8125rem}dl{display:grid;gap:.65rem;margin:.65rem 0 1rem}dt{font-size:.75rem;color:var(--muted);margin-bottom:.15rem}dd{margin:0;font-size:.875rem;white-space:pre-wrap;overflow-wrap:anywhere;line-height:1.5}.card-open{margin-top:auto;align-self:start;min-height:1.85rem;padding:.3rem 0;border-color:transparent;background:transparent;color:var(--primary-text);font-size:.75rem}.bold{font-weight:750}
 .document-files{list-style:none;padding:0;margin:0 0 1rem;max-height:16rem;overflow:auto}.document-files li{display:flex;gap:.4rem;flex-wrap:wrap;padding:.5rem 0;border-top:1px solid var(--line)}.file-link{font:inherit;font-size:.8125rem;color:var(--text);background:none;border:0;padding:0;text-align:left;overflow-wrap:anywhere;cursor:pointer}.file-link:hover{text-decoration:underline}footer{display:flex;justify-content:space-between;align-items:center;gap:.75rem;flex-wrap:wrap;margin-top:.75rem;color:var(--muted);font-size:.75rem}footer>div{display:flex;gap:.4rem}
</style>
