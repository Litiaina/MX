<script lang="ts">
  import {onMount} from 'svelte';
  import FilePreview from './FilePreview.svelte';
  import {driveNeighbors,drivePreview,guestNeighbors,guestPreview,type DriveItem,type DriveNeighbors,type DriveSort,type DriveTab} from '../api/drive';
  let {item,view='mine',parent=null,query='',sort='name_asc',guestToken,onNavigate,onClose}: {item:DriveItem;view?:DriveTab;parent?:string|null;query?:string;sort?:DriveSort;guestToken?:string;onNavigate:(item:DriveItem)=>void;onClose:()=>void}=$props();
  let neighbors=$state<DriveNeighbors>({previous:null,next:null,position:1,total:1});let busy=$state(true);let error=$state('');
  onMount(()=>{const controller=new AbortController();void (guestToken?guestNeighbors(guestToken,item.uid,controller.signal):driveNeighbors(item.uid,view,parent,query,sort,controller.signal)).then(result=>{neighbors=result;}).catch(reason=>{if(!controller.signal.aborted)error=reason instanceof Error?reason.message:'Could not load file navigation.';}).finally(()=>{if(!controller.signal.aborted)busy=false;});return()=>controller.abort();});
  function step(direction:-1|1){const next=direction<0?neighbors.previous:neighbors.next;if(next&&!busy)onNavigate(next);}
</script>
<FilePreview fileName={item.name} load={()=>guestToken?Promise.resolve(guestPreview(guestToken,item)):drivePreview(item)} externalNavigation={{...neighbors,busy,error}} navigationNoun="file" onStep={step} {onClose}/>
