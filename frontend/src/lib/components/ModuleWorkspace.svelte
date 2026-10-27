<script lang="ts">
 import {onDestroy,untrack} from 'svelte';
 import type {ModuleDefinition,ModulePermission,MxRecord,SchemaResponse} from '../api/domain';
 import type {Session} from '../api/types';
 import type {LiveMessage} from '../live/client';
 import {loadModuleWorkspace,type ModuleWorkspaceConfiguration,type ModuleView} from '../api/moduleViews';
 import {getRecord,loadModuleSchema} from '../api/workspace';
 import RecordsView from './RecordsView.svelte';
 import RecordEditor from './RecordEditor.svelte';
 import ModulePipeline from './ModulePipeline.svelte';
 import ModuleOverview from './ModuleOverview.svelte';
 import ModuleCollection from './ModuleCollection.svelte';
 import ModuleBooks from './ModuleBooks.svelte';
 import ImportRecords from './ImportRecords.svelte';
 import ModuleReadingDialog from './ModuleReadingDialog.svelte';
 import {getContext} from 'svelte';
 import {writable,type Writable} from 'svelte/store';
 import type {ModuleReadingMap} from '../api/domain';
 import {moduleReadingContext} from '../util/readingPreferences';
 import ModuleIcon from './ModuleIcon.svelte';
 import Settings2 from '@lucide/svelte/icons/settings-2';
 import Plus from '@lucide/svelte/icons/plus';
 import Info from '@lucide/svelte/icons/info';
 let {session,module,modulePermission=null,openRecordUid='',openRequestRevision=0,focusLinkedAttachments=false,revision=0,liveMessage=null}:{session:Session;module:ModuleDefinition;modulePermission?:ModulePermission|null;openRecordUid?:string;openRequestRevision?:number;focusLinkedAttachments?:boolean;revision?:number;liveMessage?:LiveMessage|null}=$props();
 const reading=getContext<Writable<ModuleReadingMap>>(moduleReadingContext)||writable<ModuleReadingMap>({});
 let configuration=$state<ModuleWorkspaceConfiguration|null>(null);let schema=$state<SchemaResponse|null>(null);let loading=$state(true);let error=$state('');let selected=$state('');let editor=$state<MxRecord|null|undefined>(undefined);let editing=$state(false);let importer=$state(false);let readingOpen=$state(false);let recordRevision=$state(0);
 let descriptionOpen=$state(false);
 let controller:AbortController|undefined;let sequence=0;let dead=false;let editorSequence=0;let linked='';let linkedRequest=-1;let lastLayoutEvent:LiveMessage|null=null;
 const active=$derived(configuration?.layout.views.find(v=>v.id===selected)||configuration?.layout.views.find(v=>v.id===configuration?.layout.default_view)||configuration?.layout.views[0]||null);
 const canCreate=$derived(modulePermission?.can_create??session.access_level<=2);const canUpdate=$derived(modulePermission?.can_update??session.access_level<=2);const canDelete=$derived(modulePermission?.can_delete??session.access_level<=1);const canAttachments=$derived(modulePermission?.can_attachments??true);
 const fields=$derived(schema?.fields.filter(f=>f.active&&(!['administrators'].includes(String(f.config.access_scope))||session.access_level===0))||[]);
 $effect(()=>{module.uid;untrack(()=>void load());});
 $effect(()=>{const message=liveMessage;untrack(()=>{if(message&&message!==lastLayoutEvent&&(message.type.startsWith('module.')||message.type.startsWith('schema.'))){lastLayoutEvent=message;void load();}});});
 $effect(()=>{const uid=openRecordUid;const request=openRequestRevision;if(!uid){linked='';linkedRequest=-1;}else if(configuration?.layout.views.length&&active&&active.kind!=='records'&&(uid!==linked||request!==linkedRequest)){linked=uid;linkedRequest=request;untrack(()=>void open(uid));}});
 onDestroy(()=>{dead=true;sequence++;editorSequence++;controller?.abort();});
 async function load(){controller?.abort();const request=new AbortController();controller=request;const current=++sequence;loading=true;error='';try{const [config,structure]=await Promise.all([loadModuleWorkspace(module.uid,request.signal),loadModuleSchema(module.uid,request.signal)]);if(dead||current!==sequence)return;configuration=config;schema=structure;if(!selected)selected=config.layout.default_view;else if(!config.layout.views.some(v=>v.id===selected))selected=config.layout.default_view;}catch(e){if(!request.signal.aborted&&!dead)error=e instanceof Error?e.message:String(e);}finally{if(current===sequence)loading=false;}}
 async function open(uid:string){const current=++editorSequence;error='';try{const result=await getRecord(uid,module.uid);if(!dead&&current===editorSequence){editor=result;editing=false;}}catch(e){if(!dead&&current===editorSequence)error=e instanceof Error?e.message:String(e);}}
 function close(){editorSequence++;editor=undefined;editing=false;if(openRecordUid)location.hash=`module/${encodeURIComponent(module.uid)}`;}
 function choose(view:ModuleView){selected=view.id;descriptionOpen=false;}
 async function saved(){recordRevision++;}
</script>
{#if error}<div class="workspace-page"><div class="notice error" role="alert">{error}<button class="button" onclick={load}>Retry workspace</button></div></div>{/if}
{#if loading&&!configuration}<div class="workspace-page" role="status">Loading module workspace…</div>
{:else if configuration&&!configuration.layout.views.length}
 <RecordsView {session} accessLevel={session.access_level} moduleUid={module.uid} {modulePermission} {openRecordUid} {openRequestRevision} {focusLinkedAttachments} {revision} {liveMessage} recordSingular={module.singular_name} recordPlural={module.name}/>
{:else if configuration&&active}
<div class="module-workspace" style={`--module-color:${module.color}`}>
 <header class="module-workspace-heading">
  <div class="module-identity"><span class="module-identity-icon" aria-hidden="true"><ModuleIcon name={module.icon} size={18}/></span><h1>{module.name}</h1>
   {#if module.description||active.description}<button class="module-about-button" type="button" title="Module and view information" aria-label={`About ${module.name}`} aria-expanded={descriptionOpen} aria-controls={`module-about-${module.uid}`} onclick={()=>descriptionOpen=!descriptionOpen}><Info size={15}/></button>{/if}
  </div>
  <div class="workspace-actions">{#if session.access_level===0}<a class="button" href={`#admin?module=${encodeURIComponent(module.uid)}&section=views`}><Settings2 size={15}/>Configure module</a>{/if}{#if canCreate}<button class="button primary" onclick={()=>{editorSequence++;editor=null;editing=true;}}><Plus size={16}/>New {module.singular_name.toLocaleLowerCase()}</button>{/if}</div>
 </header>
 {#if module.description||active.description}<div class="module-description" id={`module-about-${module.uid}`} hidden={!descriptionOpen}>{#if module.description}<p>{module.description}</p>{/if}{#if active.description}<p><strong>{active.name}</strong> · {active.description}</p>{/if}</div>{/if}
 <div class="module-view-bar"><nav aria-label="Module views">{#each configuration.layout.views as view}<button type="button" class:active={active.id===view.id} aria-current={active.id===view.id?'page':undefined} onclick={()=>choose(view)}>{view.name}</button>{/each}</nav><details class="view-options"><summary class="button">Options</summary><div><button class="button" onclick={()=>readingOpen=true}>Reading settings</button>{#if canCreate}<button class="button" onclick={()=>importer=true}>Import records</button>{/if}</div></details></div>
 <div class="module-view-body">{#key active.id}
  {#if active.kind==='records'}<RecordsView {session} accessLevel={session.access_level} moduleUid={module.uid} {modulePermission} {openRecordUid} {openRequestRevision} {focusLinkedAttachments} revision={revision+recordRevision} {liveMessage} recordSingular={module.singular_name} recordPlural={module.name} recordSections={configuration.layout.sections}/>
  {:else if active.kind==='board'}<ModulePipeline moduleUid={module.uid} revision={revision+recordRevision} onOpen={uid=>void open(uid)}/>
  {:else if active.kind==='overview'}<ModuleOverview moduleUid={module.uid} view={active} {fields} revision={revision+recordRevision} onOpen={uid=>void open(uid)}/>
  {:else if active.kind==='ledger'||active.kind==='inventory'}<ModuleBooks moduleUid={module.uid} moduleName={module.name} revision={revision+recordRevision} inline title={active.name} initialTab={active.kind==='ledger'?'journals':'items'}/>
  {:else}<ModuleCollection moduleUid={module.uid} view={active} {fields} revision={revision+recordRevision} {canAttachments} onOpen={uid=>void open(uid)}/>{/if}
 {/key}</div>
</div>
{/if}
{#if editor!==undefined&&schema}<RecordEditor {schema} moduleUid={module.uid} record={editor} canWrite={editor===null?canCreate:editing&&canUpdate} canDelete={editing&&canDelete} {canAttachments} allowEdit={editor!==null&&canUpdate} {liveMessage} focusAttachments={focusLinkedAttachments} recordLabel={module.singular_name} recordSections={configuration?.layout.sections||[]} onEdit={()=>editing=true} onClose={close} onSaved={saved}/>{/if}
{#if importer&&schema}<ImportRecords {schema} moduleUid={module.uid} onClose={()=>importer=false} onChanged={saved}/>{/if}
{#if readingOpen&&schema}<ModuleReadingDialog moduleUid={module.uid} moduleName={module.name} {fields} onSaved={preferences=>$reading=preferences} onClose={()=>readingOpen=false}/>{/if}
<style>
 .module-workspace{padding:.5rem var(--shell-gutter) 1.25rem;min-width:0;width:100%;max-width:none;box-sizing:border-box}
 .module-workspace-heading{display:flex;justify-content:space-between;align-items:center;gap:.75rem;margin-bottom:.2rem;min-height:2.35rem}
 .module-identity{display:flex;align-items:center;gap:.55rem;min-width:0}
 .module-identity-icon{width:1.85rem;height:1.85rem;flex:none;display:grid;place-items:center;border-radius:var(--control-radius);color:var(--text-2);background:var(--surface-3)}
 .module-workspace-heading h1{font-size:1.125rem;font-weight:650;margin:0;line-height:1.3;letter-spacing:-.02em;overflow-wrap:anywhere}
 .module-about-button{display:grid;place-items:center;flex:none;width:1.8rem;height:1.8rem;padding:0;border:1px solid transparent;border-radius:var(--control-radius);background:transparent;color:var(--muted)}
 .module-about-button:hover,.module-about-button[aria-expanded=true]{color:var(--text);background:var(--surface-3)}
 .module-description{color:var(--text-2);font-size:.8125rem;line-height:1.6;padding:.65rem .8rem;margin:.4rem 0;max-width:65rem;white-space:pre-wrap;background:var(--surface-2);border:1px solid var(--line);border-radius:var(--card-radius)}
 .module-description p{margin:0}.module-description p+p{margin-top:.5rem}
 .workspace-actions{display:flex;flex-wrap:wrap;gap:.4rem;flex-shrink:0}
 .module-view-bar{display:flex;align-items:center;gap:.65rem;justify-content:space-between;border-bottom:1px solid var(--line);margin-bottom:.5rem}
 .module-view-bar nav{display:flex;gap:.2rem;min-width:0;overflow:auto;scrollbar-width:thin}
 .module-view-bar nav button{font:inherit;font-size:.8125rem;min-height:2.2rem;white-space:nowrap;color:var(--muted);background:none;padding:.4rem .7rem;border:0;border-bottom:2px solid transparent;cursor:pointer}
 .module-view-bar nav button.active{color:var(--primary-text);font-weight:650;border-bottom-color:var(--primary)}
 .module-view-bar nav button:hover{background:var(--surface-2);color:var(--text)}
 .module-view-body{min-width:0}
 .view-options{position:relative;flex-shrink:0}.view-options>summary{min-height:2rem;padding:.35rem .55rem;border-color:transparent;background:transparent;font-size:.75rem;color:var(--muted)}
 .view-options>div{position:absolute;right:0;top:calc(100% + .25rem);z-index:20;padding:.4rem;background:var(--surface);border:1px solid var(--line);border-radius:var(--card-radius);box-shadow:var(--shadow-popover);display:grid;gap:.25rem;min-width:12rem}
 .module-view-body :global(.records-page){padding:0}.module-view-body :global(.records-toolbar .records-new-button){display:none}.module-view-body :global(.records-page.continuous-page){height:calc(100dvh - var(--topbar-height) - 9rem)}
 @media(max-width:700px){.module-workspace{padding:.65rem}.module-workspace-heading{flex-wrap:wrap;gap:.5rem}.module-identity{flex:1 1 12rem}.workspace-actions{flex-wrap:wrap;max-width:100%}.module-view-bar{gap:.25rem}.module-view-bar nav button{padding:.5rem .6rem}.module-workspace-heading h1{font-size:1.05rem}}
</style>
