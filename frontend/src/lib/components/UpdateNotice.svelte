<script lang="ts">
  import {onMount,onDestroy} from 'svelte';
  import RefreshCw from '@lucide/svelte/icons/refresh-cw';
  import X from '@lucide/svelte/icons/x';
  import {loadUpdateHealth,loadUpdateStatus} from '../api/updates';
  import {requestConfirmation} from '../confirmation';
  let {isAdmin,onOpenUpdates,compact=false}:{isAdmin:boolean;onOpenUpdates:()=>void;compact?:boolean}=$props();
  let available=$state(''),maintenance=$state(false),scheduled=$state<number|null>(null),reloadVersion=$state(''),dismissed=$state('');
  let initialVersion='',alive=true,timer:ReturnType<typeof setTimeout>|undefined,controller:AbortController|undefined;
  async function check(){
    const request=controller=new AbortController();let delay=20000,keepPolling=true;
    try{
      const health=await loadUpdateHealth(request.signal);if(!alive||request.signal.aborted)return;
      if(!health.configured){keepPolling=false;return;}
      if(!initialVersion)initialVersion=health.version;
      if(initialVersion!==health.version)reloadVersion=health.version;
      maintenance=health.maintenance;scheduled=health.scheduled_at;
      if(isAdmin&&!health.maintenance){const status=await loadUpdateStatus(request.signal);if(alive)available=status.available?.version!==status.current_version?status.available?.version||'':'';}
    }catch{delay=5000;/* A restart is expected; retain its last honest state. */}
    finally{if(controller===request)controller=undefined;if(alive&&keepPolling)timer=setTimeout(()=>void check(),delay);}
  }
  async function reload(){const accepted=compact?confirm('Save to MX or download your current document first. Reload this editor now?'):await requestConfirmation({title:'Reload the updated MX workspace?',description:'Save or preserve your open form/document changes first. Reloading replaces the current workspace tab.',confirmLabel:'Reload workspace',tone:'primary'});if(accepted)location.reload();}
  onMount(()=>{void check();});onDestroy(()=>{alive=false;clearTimeout(timer);controller?.abort();});
</script>

{#if maintenance||scheduled||reloadVersion||(available&&available!==dismissed)}<aside class="update-notice" class:compact class:urgent={maintenance||!!scheduled} role="status" aria-live="polite"><RefreshCw size={16}/><span>{#if maintenance}<strong>MX is updating.</strong> New requests are paused; keep this tab open. The server will reconnect.{:else if scheduled}<strong>MX restart scheduled for {new Date(scheduled).toLocaleTimeString()}.</strong> Save your work now. Calls and transfers will be interrupted.{:else if reloadVersion}<strong>MX updated to {reloadVersion}.</strong> Reload after saving your open work.{:else}<strong>MX {available} is available.</strong> Review its installation policy in Updates.{/if}</span>{#if reloadVersion&&!maintenance&&!scheduled}<button class="button small" onclick={()=>void reload()}>Reload when ready</button>{:else if isAdmin}<button class="button small" onclick={onOpenUpdates}>View updates</button>{/if}{#if !maintenance&&!scheduled&&!reloadVersion}<button class="dismiss-update" aria-label="Dismiss update notice" onclick={()=>dismissed=available}><X size={15}/></button>{/if}</aside>{/if}

<style>
  .update-notice{display:flex;align-items:center;gap:.65rem;padding:.55rem 1rem;border-bottom:1px solid var(--line,#d9dee6);background:var(--surface-2,#f5f7fa);color:var(--text,#20242a);font:13px/1.5 system-ui,sans-serif;flex-wrap:wrap;}
  .update-notice.compact,.update-notice.urgent{position:fixed;bottom:3rem;left:50%;transform:translateX(-50%);width:calc(100% - 24px);max-width:760px;box-sizing:border-box;border:1px solid var(--line,#d9dee6);border-radius:8px;box-shadow:0 4px 20px #0003;z-index:10000;}
  :global(html[data-theme='dark']) .update-notice{background:var(--surface-2,#20242a);color:var(--text,#e5e7eb);border-color:var(--line,#3e4754);}
  .update-notice>:global(svg){flex-shrink:0;}
  .update-notice>span{flex:1;min-width:min(100%,14rem);line-height:1.5;}
  .dismiss-update{display:inline-flex;border:0;background:transparent;color:var(--muted);padding:.4rem;}
  .update-notice .button{flex-shrink:0;}
  .update-notice.compact .button{border:1px solid var(--line,#b9c3d2);border-radius:5px;background:transparent;color:inherit;padding:6px 8px;font:inherit;}
  @media(max-width:500px){.update-notice{padding:.5rem .7rem;}.update-notice>.button{margin-left:1.5rem;}}
</style>
