<script lang="ts">
  import {onDestroy,onMount} from 'svelte';
  import X from '@lucide/svelte/icons/x';
  import ShieldAlert from '@lucide/svelte/icons/shield-alert';
  import {checkFieldArchive,archiveFailure,type ArchiveBlocker,type FieldArchiveCheck} from '../../api/fieldArchive';
  import {updateField} from '../../api/workspace';
  import {focusDialog} from '../../util/focusDialog';
  let {fieldUid,fieldLabel,onClose,onArchived,onOpen}:{fieldUid:string;fieldLabel:string;onClose:()=>void;onArchived:()=>Promise<void>;onOpen:(blocker:ArchiveBlocker)=>void}=$props();
  let check=$state<FieldArchiveCheck|null>(null);let checking=$state(true);let busy=$state(false);let error=$state('');
  let controller:AbortController|undefined;
  onMount(()=>void reload());onDestroy(()=>controller?.abort());
  async function reload(){
    controller?.abort();const request=new AbortController();controller=request;checking=true;error='';
    try{const result=await checkFieldArchive(fieldUid,request.signal);if(!request.signal.aborted)check=result;}
    catch(reason){if(!request.signal.aborted)error=reason instanceof Error?reason.message:'Could not check dependencies. Try again.';}
    finally{if(controller===request)checking=false;}
  }
  async function archive(){
    if(checking||busy||!check?.can_archive)return;
    busy=true;error='';
    try{await updateField(fieldUid,{active:false});await onArchived();}
    catch(reason){const latest=archiveFailure(reason,fieldUid);if(latest){check=latest;error='Dependencies changed since the check. Review the updated list below.';}else{error=reason instanceof Error?reason.message:'The archive could not be confirmed. Check the field status before retrying.';check=null;}}
    finally{busy=false;}
  }
  async function close(){if(busy)return;if(check&&!check.field.active){busy=true;try{await onArchived();}catch(reason){error=reason instanceof Error?reason.message:'Could not refresh the settings. Try again.';}finally{busy=false;}}else onClose();}
  const title=$derived(checking?'Check before archiving':error&&!check?'Could not check this field':check&&!check.field.active?'Field already archived':check?.blockers.length?'Cannot archive this field':'Archive this field?');
  const names:Record<string,string>={relationship:'Relationship',lookup:'Lookup',rollup:'Rollup',formula:'Formula'};
</script>
<svelte:window onkeydown={event=>{if(event.key==='Escape'&&!busy){event.preventDefault();close();}}} />
<div class="overlay" role="presentation" onclick={event=>{if(event.target===event.currentTarget)close();}}>
  <div class="dialog field-archive-dialog" role="dialog" aria-modal="true" aria-labelledby="field-archive-title" aria-describedby="field-archive-description" aria-busy={checking||busy} tabindex="-1" use:focusDialog>
    <header class="dialog-head"><div class="archive-heading"><span class="archive-icon"><ShieldAlert size={21}/></span><div><h2 id="field-archive-title">{title}</h2><p>{check?.module_name||'Module field'} · <strong>{check?.field.label||fieldLabel}</strong></p></div></div><button class="icon-button" type="button" aria-label="Close field archive dialog" disabled={busy} onclick={close}><X size={18}/></button></header>
    <div class="archive-body">
      <p id="field-archive-description">{check?.blockers.length?'Other settings still use this field. Resolve the dependencies below, then check again. No field or record has been changed.':'Archiving hides the field from new entries. Historical values remain stored; no records or files are deleted.'}</p>
      {#if error}<div class="notice error" role="alert">{error}</div>{/if}
      {#if checking}<div class="archive-loading" role="status"><span class="loader"></span>Checking relationships, calculations, record links, files and storage settings…</div>
      {:else if check?.blockers.length}<h3>Used by {check.blockers.length} {check.blockers.length===1?'dependency':'dependencies'}</h3><ul class="archive-blockers">{#each check.blockers as blocker}<li><div class="blocker-title"><strong>{blocker.module_name} → {blocker.field_label|| (blocker.section==='storage'?'N1 storage layout':'Stored records')}</strong>{#if blocker.field_type}<span>{names[blocker.field_type]||blocker.field_type}</span>{/if}</div><p>{blocker.reason}</p><p class="resolution"><strong>How to fix:</strong> {blocker.resolution}</p><button class="button small" type="button" onclick={()=>onOpen(blocker)}>{blocker.section==='field'?'Open field settings':blocker.section==='storage'?'Open storage layout':'Open module records'}</button></li>{/each}</ul>
      {:else if check&&!check.field.active}<p role="status">This field is already archived. Close this dialog to return to its settings.</p>
      {:else if check}<p class="archive-ready" role="status">No blocking dependencies found. The server will check again when you archive.</p>{/if}
    </div>
    <footer class="dialog-actions"><button class="button" type="button" disabled={busy} onclick={close}>{check&&!check.field.active?'Done':'Cancel'}</button><span class="spacer"></span><button class="button" type="button" disabled={checking||busy} onclick={()=>void reload()}>Check again</button><button class="button danger" type="button" disabled={checking||busy||!check?.can_archive} onclick={()=>void archive()}>{busy?'Archiving…':'Archive field'}</button></footer>
  </div>
</div>
<style>
  .field-archive-dialog{width:min(44rem,100%);max-height:calc(100dvh - 2rem);padding:0;display:flex;flex-direction:column;overflow:hidden}.dialog-head,.dialog-actions{padding:1rem;flex-shrink:0}.dialog-head{border-bottom:1px solid var(--line);margin:0}.dialog-actions{border-top:1px solid var(--line);margin:0;gap:.5rem}.archive-heading{display:flex;gap:.65rem;align-items:flex-start;min-width:0}.archive-icon{flex-shrink:0;color:var(--muted)}h2{font-size:1.15rem;margin:0}.archive-heading p{margin:.35rem 0 0;color:var(--muted);overflow-wrap:anywhere}.archive-body{padding:1rem;overflow:auto;min-height:0}.archive-body>p{margin-top:0;color:var(--muted)}h3{font-size:.9rem;margin:1rem 0 .6rem}.archive-loading{display:flex;align-items:center;gap:.8rem;padding:1rem 0}.loader{flex-shrink:0}.archive-blockers{list-style:none;margin:0;padding:0;display:grid;gap:.7rem}.archive-blockers li{padding:.85rem;border:1px solid var(--line);border-radius:.55rem;background:var(--surface-2);min-width:0}.blocker-title{display:flex;align-items:start;justify-content:space-between;gap:.5rem;overflow-wrap:anywhere}.blocker-title>strong{min-width:0}.blocker-title>span{font-size:.7rem;color:var(--muted);flex-shrink:0}.archive-blockers p{font-size:.85rem;margin:.5rem 0;overflow-wrap:anywhere}.resolution{color:var(--muted)}.archive-ready{padding:.75rem;border:1px solid var(--line);border-radius:.4rem}@media(max-width:540px){.overlay{padding:.5rem}.field-archive-dialog{width:100%;max-height:calc(100dvh - 1rem)}.dialog-actions{flex-wrap:wrap;padding:.75rem}.dialog-actions .spacer{display:none}.dialog-actions>button{flex:1}.blocker-title{flex-direction:column}.archive-heading p{font-size:.85rem}}
</style>
