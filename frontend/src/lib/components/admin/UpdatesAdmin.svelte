<script lang="ts">
  import {onDestroy,untrack} from 'svelte';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import Download from '@lucide/svelte/icons/download';
  import Upload from '@lucide/svelte/icons/upload';
  import ShieldCheck from '@lucide/svelte/icons/shield-check';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import {ConnectionError} from '../../api/client';
  import {loadUpdateStatus,queueUpdate,saveUpdatePolicy,uploadUpdatePackage,updateActive,offsetLabel,type UpdateStatus,type UpdatePolicy} from '../../api/updates';
  import {requestConfirmation} from '../../confirmation';
  import {operationUid} from '../../api/operation';
  let {revision=0}:{revision?:number}=$props();
  let status=$state<UpdateStatus|null>(null),problem=$state(''),notice=$state(''),connection=$state(''),pending=$state(''),waiting=$state(false),polling=$state(false);
  let policy=$state<UpdatePolicy>({mode:'manual',start_hour:2,end_hour:3,utc_offset_minutes:480}),dirty=$state(false),file=$state<File|null>(null),uploaded=$state(0),uploadTotal=$state(0);
  let feedbackAt=$state<'actions'|'policy'|'upload'>('actions');
  let uncertain:{uid:string;action:'check'|'stage'|'install';version?:string}|undefined;
  let uncertainIdle=$state(0);
  let alive=true,controller:AbortController|undefined,uploadController:AbortController|undefined,timer:ReturnType<typeof setTimeout>|undefined;
  const active=$derived(!!pending||waiting||!!status?.busy||updateActive(status?.operation));
  const offsets=Array.from({length:105},(_,i)=>-720+i*15);
  function when(value?:number|null){return value?new Date(value).toLocaleString():'Not yet';}
  function bytes(value:number){return value>=1024**3?`${(value/1024**3).toFixed(2)} GiB`:value>=1024**2?`${(value/1024**2).toFixed(1)} MiB`:`${Math.round(value/1024)} KiB`;}
  function errorMessage(reason:unknown){return reason instanceof Error?reason.message:'The update request failed.';}
  function schedule(){clearTimeout(timer);if(alive)timer=setTimeout(()=>void refresh(),active?1500:30000);}
  async function refresh(){
    if(!alive||polling)return;polling=true;const request=controller=new AbortController();
    try {const result=await loadUpdateStatus(request.signal);if(!alive||request.signal.aborted)return;status=result;connection='';if(result.policy&&!dirty)policy={...result.policy};if(waiting&&uncertain){if(result.operation?.uid===uncertain.uid||result.history?.some(op=>op.uid===uncertain?.uid)){waiting=false;uncertain=undefined;problem='';notice='The queued request was confirmed from MX’s durable job history.';}else if(!result.busy&&!updateActive(result.operation))uncertainIdle++;}}
    catch(reason){if(alive&&!request.signal.aborted)connection=status?.maintenance||['activating','validating','rolling_back'].includes(status?.operation?.phase||'')?'MX is restarting. This page will reconnect automatically.':errorMessage(reason);}
    finally{if(controller===request){polling=false;controller=undefined;schedule();}}
  }
  async function run(action:'check'|'stage'|'install'){
    if(active)return;const version=action==='stage'?status?.available?.version:action==='install'?status?.staged?.version:undefined;
    if(action==='install'&&!await requestConfirmation({title:`Install MX ${version}?`,description:`MX will warn connected clients, pause new requests, stop the service, verify a final SQLite backup and activate the matching binary/frontend. Failed activation restores the previous application and database before access reopens. The restart interrupts active calls and transfers.`,confirmLabel:'Install and restart',tone:'primary'}))return;
    pending=action;feedbackAt='actions';problem='';notice='';const uid=operationUid();
    try {await queueUpdate(action,version,uid);if(alive){waiting=true;uncertain={uid,action,version};uncertainIdle=0;notice='Job queued. You can leave this page; the independent worker continues.';}}
    catch(reason){if(alive){if(reason instanceof ConnectionError){waiting=true;uncertain={uid,action,version};uncertainIdle=0;problem='MX did not acknowledge the request. Checking its durable status; this page will not submit it again.';}else problem=errorMessage(reason);}}
    finally{if(alive){pending='';await refresh();}}
  }
  async function retryUnconfirmed(){if(!uncertain||uncertainIdle<2)return;const original=uncertain;pending='retry';try{await queueUpdate(original.action,original.version,original.uid);}catch(reason){if(alive)problem=errorMessage(reason);}finally{if(alive){pending='';await refresh();}}}
  async function save(event:SubmitEvent){event.preventDefault();if(active)return;pending='policy';feedbackAt='policy';problem='';notice='';try{await saveUpdatePolicy(policy);if(alive){dirty=false;notice='Installation policy saved.';}}catch(reason){if(alive)problem=errorMessage(reason);}finally{if(alive){pending='';await refresh();}}}
  async function upload(){if(!file||active)return;pending='upload';feedbackAt='upload';uploaded=0;uploadTotal=file.size;problem='';notice='';uploadController=new AbortController();try{await uploadUpdatePackage(file,(done,total)=>{uploaded=done;uploadTotal=total;},uploadController.signal);if(alive){notice='Package received. The worker will verify its signature and every file before it becomes installable.';file=null;}}catch(reason){if(alive)problem=errorMessage(reason);}finally{if(alive){pending='';uploadController=undefined;await refresh();}}}
  // Reconnection/global refresh must not destroy an edited policy, selected
  // package or uncertain job identity. Refresh status in this mounted panel.
  $effect(()=>{revision;untrack(()=>void refresh());});
  onDestroy(()=>{alive=false;clearTimeout(timer);controller?.abort();uploadController?.abort();});
</script>

<section class="panel section-panel updates-panel" aria-label="MX updates">
  <header class="updates-heading"><div><p class="eyebrow">Release management</p><h2>MX updates</h2><p class="muted">Installed <strong>{status?.current_version||'Checking…'}</strong>{#if status?.configured} · {status.channel==='rc'?'Stable + release candidates':'Stable releases'}{/if}</p></div><button class="button" type="button" disabled={polling} onclick={()=>void refresh()}><RefreshCw size={15}/> Refresh status</button></header>
  {#if connection}<div class="notice warning" role="alert">{connection}</div>{/if}
  {#if problem&&feedbackAt==='actions'}<div class="notice error" role="alert">{problem}</div>{/if}
  {#if notice&&feedbackAt==='actions'}<div class="notice success" role="status">{notice}</div>{/if}
  {#if waiting&&uncertainIdle>=2}<div class="notice warning"><span>No matching job is recorded in two fresh status checks. Retrying uses the same idempotent request ID.</span><button class="button" disabled={!!pending} onclick={()=>void retryUnconfirmed()}>Retry the same request</button></div>{/if}
  {#if !status}<p class="muted">{connection?'Status could not be loaded. Use Refresh status to retry.':'Loading release and worker status…'}</p>
  {:else if !status.configured}<div class="update-setup"><ShieldCheck size={28}/><h3>One-time deployment setup required</h3><p>{status.message}</p><p>The operator provisions a release signing public key, managed binary/frontend slots and systemd worker units. See <code>docs/updates.md</code>. Private signing keys must never be installed on the MX server.</p></div>
  {:else}
    {#if !status.managed}<div class="notice warning" role="status">Downloads and verification are available, but installation is disabled until this service runs from the managed <code>current</code> slot. Follow the one-time deployment setup.</div>{/if}
    <div class="release-grid">
      <article class="release-card"><h3>Available release</h3>{#if status.available}<strong class="release-version">{status.available.version}</strong><p>Signed release metadata verified against this deployment’s trusted key.</p>{:else}<p>No newer trusted release found.</p>{/if}<p class="muted">Last check: {when(status.last_checked_at)}</p><div class="release-actions"><button class="button" disabled={active} onclick={()=>void run('check')}><RefreshCw size={15}/> Check for updates</button>{#if status.available&&status.available.version!==status.staged?.version}<button class="button primary" disabled={active} onclick={()=>void run('stage')}><Download size={15}/> Download and verify</button>{/if}</div>{#if status.available?.notes}<details><summary>Release notes</summary><pre class="release-notes">{status.available.notes}</pre></details>{/if}</article>
      <article class="release-card"><h3>Ready to install</h3>{#if status.staged}<strong class="release-version">{status.staged.version}</strong><p>Signature, platform and installed-file checksums verified. The previous release and final SQLite snapshot are retained for rollback.</p><button class="button primary" disabled={active||!status.managed||status.staged.version===status.current_version} onclick={()=>void run('install')}>{status.staged.version===status.current_version?'Already installed':'Install now…'}</button>{:else}<p>Download a release or upload a signed offline package. Verification finishes before installation can begin.</p>{/if}<p class="muted">Worker last seen: {when(status.worker_last_started_at)}</p></article>
    </div>
    {#if status.operation}
      {@const operation=status.operation}
      <section class="update-activity" aria-label="Update activity" aria-live="polite">
        <div class="activity-heading"><strong>{#if updateActive(operation)}<span class="update-spinner"><LoaderCircle size={16}/></span>{/if}{operation.phase.replaceAll('_',' ')}</strong><span>{operation.version||''}</span></div>
        <p>{operation.message}</p>
        {#if updateActive(operation)}
          {#if operation.total>0}<progress aria-label="Update progress" value={Math.min(operation.bytes,operation.total)} max={operation.total}></progress><small>{bytes(operation.bytes)} of {bytes(operation.total)}</small>
          {:else}<progress aria-label="Update activity"></progress>{/if}
        {/if}
        {#if operation.error}<div class="notice error" role="alert">{operation.error}</div>{/if}
        {#if operation.phase==='queued'&&Date.now()-operation.started_at>15000}<div class="notice warning">The job is still queued. Check that the independent systemd worker/path units are running. Do not start a second update.</div>{/if}
      </section>
    {/if}
    <form class="update-policy" onsubmit={save}>
      <div><h3>Installation policy</h3><p class="muted">Operator choice: manual approval, or automatic installation inside your maintenance window. Checks do not interrupt MX.</p></div>
      <div class="policy-fields">
        <label>Install updates<select aria-label="Install updates" bind:value={policy.mode} disabled={active} onchange={()=>dirty=true}><option value="manual">Only when an administrator installs</option><option value="automatic">Automatically during maintenance</option></select></label>
        {#if policy.mode==='automatic'}
          <label>Window starts<select aria-label="Window starts" bind:value={policy.start_hour} disabled={active} onchange={()=>dirty=true}>{#each Array.from({length:24},(_,i)=>i) as hour}<option value={hour}>{String(hour).padStart(2,'0')}:00</option>{/each}</select></label>
          <label>Window ends<select aria-label="Window ends" bind:value={policy.end_hour} disabled={active} onchange={()=>dirty=true}>{#each Array.from({length:24},(_,i)=>i) as hour}<option value={hour}>{String(hour).padStart(2,'0')}:00</option>{/each}</select></label>
          <label>Window time zone<select aria-label="Window time zone" bind:value={policy.utc_offset_minutes} disabled={active} onchange={()=>dirty=true}>{#each offsets as offset}<option value={offset}>{offset===480?'Asia/Manila · ':''}{offsetLabel(offset)}</option>{/each}</select></label>
        {/if}
      </div>
      {#if policy.mode==='automatic'}<p class="muted">A fixed UTC offset is used; daylight-saving changes are not automatic. Active calls defer unattended installation. A failed/rolled-back release requires an explicit retry. Connected users get {status.notice_seconds}s warning before a restart.</p>{/if}
      {#if problem&&feedbackAt==='policy'}<div class="notice error" role="alert">{problem}</div>{/if}
      {#if notice&&feedbackAt==='policy'}<div class="notice success" role="status">{notice}</div>{/if}
      <button class="button primary" disabled={active||!dirty}>{pending==='policy'?'Saving…':'Save policy'}</button>
    </form>
    <section class="offline-update"><h3>Offline update package</h3><p class="muted">For LAN-only deployments, upload a deployment-trusted <code>.mxupdate</code> package. The same signature and rollback rules apply; no internet connection is needed.</p><div class="offline-actions"><label class="package-picker">Signed package<input type="file" accept=".mxupdate" disabled={active} onchange={(event)=>file=event.currentTarget.files?.[0]||null}/></label><button class="button" disabled={!file||active} onclick={()=>void upload()}><Upload size={15}/> {pending==='upload'?'Uploading…':'Upload and verify'}</button></div>{#if problem&&feedbackAt==='upload'}<div class="notice error" role="alert">{problem}</div>{/if}{#if notice&&feedbackAt==='upload'}<div class="notice success" role="status">{notice}</div>{/if}{#if pending==='upload'}<progress aria-label="Package upload progress" value={uploaded} max={uploadTotal||1}></progress><small>{bytes(uploaded)} of {bytes(uploadTotal)}</small>{/if}</section>
    {#if status.history?.length}<details class="update-history"><summary>Update history · {status.history.length} recent jobs</summary><div class="table-wrap"><table><thead><tr><th>Started</th><th>Action</th><th>Version</th><th>Result / details</th></tr></thead><tbody>{#each [...status.history].reverse() as operation}<tr><td>{when(operation.started_at)}</td><td>{operation.action}</td><td>{operation.version||'—'}</td><td><strong>{operation.phase.replaceAll('_',' ')}</strong><p>{operation.message}</p>{#if operation.error}<p class="error-text">{operation.error}</p>{/if}</td></tr>{/each}</tbody></table></div></details>{/if}
  {/if}
</section>

<style>
  .updates-panel{display:grid;gap:1.2rem;min-width:0;}.updates-panel h2,.updates-panel h3,.updates-panel p{margin:0;}.updates-panel p{line-height:1.5;}.updates-panel h3{font-size:.95rem;}.updates-heading,.activity-heading{display:flex;justify-content:space-between;align-items:center;gap:.75rem;flex-wrap:wrap;}.updates-heading>div{display:grid;gap:.4rem;}.release-grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:1rem;}.release-card,.update-setup{display:grid;align-content:start;gap:.8rem;padding:1rem;border:1px solid var(--line);border-radius:.5rem;min-width:0;}.release-version{font-size:1.3rem;}.release-actions,.offline-actions{display:flex;gap:.5rem;align-items:end;flex-wrap:wrap;}.release-notes{font:inherit;font-size:.8rem;white-space:pre-wrap;overflow-wrap:anywhere;max-height:18rem;overflow:auto;margin:.75rem 0 0;}.update-activity{display:grid;gap:.65rem;padding:1rem;border:1px solid var(--line);background:var(--surface-2);border-radius:.5rem;}.activity-heading>strong{display:inline-flex;gap:.5rem;align-items:center;text-transform:capitalize;}.updates-panel progress{width:100%;height:.45rem;accent-color:var(--primary);}.update-policy,.offline-update{display:grid;gap:.8rem;border-top:1px solid var(--line);padding-top:1.2rem;}.update-policy>button{justify-self:start;}.policy-fields{display:grid;grid-template-columns:minmax(14rem,2fr) repeat(2,minmax(7rem,1fr)) minmax(12rem,1.5fr);gap:.75rem;align-items:end;}.policy-fields label{min-width:0;}.policy-fields select{width:100%;}.package-picker{flex:1;min-width:min(100%,16rem);}.package-picker input{width:100%;}.update-history summary{cursor:pointer;font-weight:600;}.update-history .table-wrap{margin-top:.75rem;}.update-history td{vertical-align:top;white-space:normal;}.update-history td:last-child{min-width:14rem;overflow-wrap:anywhere;}.update-history td p{margin-top:.3rem;}.update-spinner{animation:update-spin 1s linear infinite;}.updates-panel .notice{margin:0;overflow-wrap:anywhere;}@keyframes update-spin{to{transform:rotate(360deg);}}@media(prefers-reduced-motion:reduce){.update-spinner{animation:none;}}@media(max-width:1100px){.policy-fields{grid-template-columns:repeat(2,minmax(0,1fr));}}@media(max-width:700px){.release-grid,.policy-fields{grid-template-columns:minmax(0,1fr);}.updates-heading>.button,.offline-actions>.button{width:100%;}}
</style>
