<script lang="ts">
  import { onMount, tick } from 'svelte';
  import UpdateNotice from './UpdateNotice.svelte';
  import Save from '@lucide/svelte/icons/save';
  import Download from '@lucide/svelte/icons/download';
  import FileText from '@lucide/svelte/icons/file-text';
  import WifiOff from '@lucide/svelte/icons/wifi-off';
  import ArrowLeft from '@lucide/svelte/icons/arrow-left';
  import ArrowRight from '@lucide/svelte/icons/arrow-right';
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import CheckCircle from '@lucide/svelte/icons/circle-check';
  import MoreHorizontal from '@lucide/svelte/icons/ellipsis';
  import X from '@lucide/svelte/icons/x';
  import { savedToMx, offlinePreparationSummary } from '../office/feedback';
  import { apiJson, apiFile, ApiError, ConnectionError, AUTH_EXPIRED_EVENT, hasAuthTokens } from '../api/client';
  import { authenticate, loadSession } from '../api/auth';
  import { uploadDriveFile, driveDownload, type DriveItem } from '../api/drive';
  import { OfficeEngine, type OfficeSnapshot } from '../office/engine';
  import { officeFormats, officeExtension, officePresentation } from '../office/formats';
  import { listDrafts, storeDraft, removeDraft, draftKey, type OfficeDraft } from '../office/drafts';
  import { OfficeOperations } from '../office/operations';
  import { recoveredUpload } from '../office/recovery';
  import { checkOfficeGraphics } from '../office/capabilities';
  import OfficeLoading from './OfficeLoading.svelte';
  import { openingDetail, type OfficeLoadingProgress } from '../office/loading';
  import { officeResources, checkOfficeInputSize } from '../office/resources';
  import { officeIdleDelay } from '../office/idle';

  let canvas: HTMLCanvasElement;
  let viewport: HTMLDivElement; let viewportWidth = $state(innerWidth);
  // The bundled native toolbar is desktop-sized. Keep its initialized width
  // and scroll the viewport rather than squeezing a 1280px framebuffer into a
  // phone-sized canvas (which distorts text and native hit targets).
  const canvasWidth = Math.max(960, innerWidth);
  let item = $state<DriveItem | null>(null);
  let accountUid = $state(''); let extension = $state('');
  let ready = $state(false); let working = $state(false); let status = $state('Preparing MX Office…');
  let loading = $state<OfficeLoadingProgress>({ phase: 'access', detail: 'Checking saved copies on this device and preparing your MX session.' });
  let loadingFailed = $state(false);
  let error = $state(''); let notice = $state(''); let offline = $state(!navigator.onLine);
  let saveConfirmation = $state(''); let saving = $state(false);
  let selectedCell = $state('');
  let optionsOpen = $state(false);
  let optionsPanel = $state<HTMLDivElement>(); let optionsButton = $state<HTMLButtonElement>();
  let nativeFocused = $state(false); let focusSequence = 0;
  let generation = $state(0); let acknowledged = $state(0); let persist = $state(false);
  let automaticSave = $state(true); let recoveryError = $state(''); let storedGeneration = $state(-1);
  let saveStage = $state<'idle'|'preparing'|'local'|'uploading'|'confirming'>('idle');
  let savePercent = $state(0); let saveSeconds = $state(0);
  let lastExportMs = 0; let lastActivity = Date.now(); let preparedAt = -Infinity; let saveStartedAt = 0;
  let backgroundSaving = $state(false); let manualSaveQueued = false;
  const heldPointers = new Set<number>(); const heldKeys = new Set<string>(); let composing = false;
  let inputBaseGeneration = 0; let inputCommitSequence = -1;
  let engineBoot:Promise<void>|undefined;
  const editorUid=crypto.randomUUID();
  let activeDraftKey=''; let restoredDraftKey='';
  let recoveryFingerprint='';
  let automaticTimer:ReturnType<typeof setTimeout>|undefined;
  let offlineTimer:ReturnType<typeof setTimeout>|undefined;
  let directSave = $state(false);
  let uncommittedInput = $state(false); let inputSequence = 0;
  let offlineReady = $state(false); let offlineError = $state(''); let offlineUnsupported = $state(false); let storedAt = $state(0);
  let drafts = $state<OfficeDraft[]>([]); let offered = $state<OfficeDraft | null>(null);
  let conflict = $state(false); let signIn = $state(false); let needsSignIn = $state(false); let email = $state(''); let password = $state(''); let factor = $state('');
  let copyName = $state(''); let copying = $state(false);
  let snapshot = $state<OfficeSnapshot | null>(null);
  let pendingFile = $state<File | undefined>(); let pendingGeneration = -1;
  let pendingOperation: string | undefined;
  let pendingCopy = $state<File | undefined>(); let copyOperation: string | undefined; let copyGeneration = -1;
  let engine: OfficeEngine; let engineStarted = false; let disposed = false;
  let openingController: AbortController | undefined;
  let openingCancelled = false;
  let openingFocus = $state(false);
  let largeDocument = false;
  const localWork = new OfficeOperations();
  let draftTimer: ReturnType<typeof setTimeout> | undefined;
  let connectionSequence = 0;
  const dirty = $derived(uncommittedInput || generation !== acknowledged || !!pendingFile || !!pendingCopy);
  const presentation = $derived(officePresentation(extension));
  const readOnly = $derived(presentation || item?.permission === 'viewer' || item?.permission === 'guest');
  const spreadsheet = $derived(['.xls','.xlsx','.ods'].includes(extension));
  const feedback = $derived(saveConfirmation || status);
  const compactStatus = $derived(readOnly ? 'Read-only' : saving ? backgroundSaving ? 'Autosaving…' : `${saveStage==='preparing'?'Preparing':saveStage==='local'?'Saving locally':saveStage==='confirming'?'Confirming':`Uploading ${savePercent}%`}${saveSeconds>=3?` · ${saveSeconds}s`:''}` : working ? 'Working…' : error ? 'Needs attention' : dirty ? storedGeneration===generation&&!uncommittedInput?'Saved locally':'Unsaved' : saveConfirmation ? 'Saved to MX' : ready ? 'Up to date' : 'Loading…');
  const requestedUid = new URLSearchParams(location.hash.slice(1)).get('file');
  type OpenContext = { item: DriveItem; version_uid: string };

  onMount(() => {
    document.documentElement.dataset.theme = sessionStorage.getItem('mx_office_theme') || 'light';
    needsSignIn = !hasAuthTokens();
    engine = new OfficeEngine((value) => { generation = Math.max(generation,value); if(inputCommitSequence===inputSequence&&value>inputBaseGeneration)uncommittedInput=false; saveConfirmation=''; if(!working)status='Changes not yet saved to MX';activity(); }, (message) => { error = message; ready = false; loadingFailed=true; status='Office stopped · your last prepared copy is still downloadable'; }, (copy) => { if(copy)offerCopy();else void save(); }, message=>{notice=message;void focusDocument();}, cell=>{selectedCell=cell;if(ready)activity();}, progress=>{if(!disposed&&!ready&&!loadingFailed&&['engine','runtime'].includes(loading.phase))loading=progress;});
    void initialize();
    const network = () => { void checkConnection(); };
    const expired = () => { requireSignIn(); error = 'Sign in again to save. Your open document and local draft are retained.'; };
    const unload = (event: BeforeUnloadEvent) => { if (dirty || working) { event.preventDefault(); event.returnValue = ''; } };
    const key = (event: KeyboardEvent) => {
      if(ready){heldKeys.add(event.code||event.key);activity();}
      if (event.key === 'Escape' && optionsOpen) { optionsOpen=false; optionsButton?.focus(); return; }
      if ((event.ctrlKey || event.metaKey) && !event.shiftKey && event.key.toLowerCase() === 'o') {
        // Keep Qt's key-down/key-up stream intact. Its Open dispatch is guarded
        // too; suppressing only one half leaves native modifier/input state stale.
        event.preventDefault();
        notice='Open another document from MX Drive in a separate editor tab. This editor stays bound to its original file.';
        return;
      }
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 's') { event.preventDefault(); if(event.shiftKey)offerCopy();else void save(); return; }
      if(ready&&event.target===canvas&&!event.isComposing){
        if(uncommittedInput&&['Enter','Tab'].includes(event.key))inputCommitSequence=inputSequence;
        if(event.key==='Escape'){uncommittedInput=false;inputCommitSequence=-1;}
      }
      if (ready && !readOnly && !event.isComposing && event.target === canvas && ((!event.ctrlKey && !event.metaKey && !event.altKey && (event.key.length === 1 || ['Backspace','Delete'].includes(event.key))) || ((event.ctrlKey || event.metaKey) && ['v','x','z','y'].includes(event.key.toLowerCase())))) {
        const pendingInput=uncommittedInput;
        uncommittedInput = true; inputSequence++; inputBaseGeneration=generation;
        inputCommitSequence=(event.ctrlKey||event.metaKey)&&(['z','y'].includes(event.key.toLowerCase())||(!pendingInput&&event.key.toLowerCase()==='v'))?inputSequence:-1;
        saveConfirmation = ''; if(!working)status='Changes not yet saved to MX';
        activity();
      }
    };
    const keyUp=(event:KeyboardEvent)=>{heldKeys.delete(event.code||event.key);if(ready)activity();};
    const pointerDown=(event:PointerEvent)=>{if(ready){heldPointers.add(event.pointerId);activity();if(event.target===canvas&&uncommittedInput)inputCommitSequence=inputSequence;}};
    const pointerUp=(event:PointerEvent)=>{heldPointers.delete(event.pointerId);if(ready)activity();};
    const pointerMove=(event:PointerEvent)=>{if(ready&&(event.buttons||heldPointers.has(event.pointerId)))activity();};
    const interaction=()=>{if(ready)activity();};
    const compositionStart=(event:CompositionEvent)=>{if(!ready||readOnly)return;composing=true;if(event.target instanceof Node&&viewport?.contains(event.target)){uncommittedInput=true;inputSequence++;inputBaseGeneration=generation;inputCommitSequence=-1;saveConfirmation='';}activity();};
    const compositionEnd=()=>{composing=false;if(ready)activity();};
    const blur=()=>{heldPointers.clear();heldKeys.clear();composing=false;if(ready)activity();};
    const interval = setInterval(() => {if(saving)saveSeconds=Math.floor((Date.now()-saveStartedAt)/1000);if(ready&&dirty&&!working&&!automaticTimer&&!error)scheduleDraft();},1000);
    const outsideOptions = (event: PointerEvent) => { if(optionsOpen && event.target instanceof Node && !optionsPanel?.contains(event.target) && !optionsButton?.contains(event.target))optionsOpen=false; };
    const connectionInterval = setInterval(()=>void checkConnection(),15_000);
    void checkConnection();
    window.addEventListener('online', network); window.addEventListener('offline', network);
    window.addEventListener(AUTH_EXPIRED_EVENT, expired); window.addEventListener('beforeunload', unload); window.addEventListener('keydown', key, true);
    window.addEventListener('pointerdown', outsideOptions);
    window.addEventListener('keyup',keyUp,true);window.addEventListener('pointerdown',pointerDown,true);window.addEventListener('pointerup',pointerUp,true);window.addEventListener('pointercancel',pointerUp,true);window.addEventListener('pointermove',pointerMove,{capture:true,passive:true});window.addEventListener('wheel',interaction,{capture:true,passive:true});window.addEventListener('touchmove',interaction,{capture:true,passive:true});window.addEventListener('compositionstart',compositionStart,true);window.addEventListener('compositionend',compositionEnd,true);window.addEventListener('blur',blur);
    return () => { disposed = true; openingController?.abort(); clearInterval(interval); clearInterval(connectionInterval); clearTimeout(draftTimer);clearTimeout(automaticTimer);clearTimeout(offlineTimer); engine.dispose(); window.removeEventListener('online', network); window.removeEventListener('offline', network); window.removeEventListener(AUTH_EXPIRED_EVENT, expired); window.removeEventListener('beforeunload', unload); window.removeEventListener('keydown', key, true); window.removeEventListener('pointerdown', outsideOptions);window.removeEventListener('keyup',keyUp,true);window.removeEventListener('pointerdown',pointerDown,true);window.removeEventListener('pointerup',pointerUp,true);window.removeEventListener('pointercancel',pointerUp,true);window.removeEventListener('pointermove',pointerMove,true);window.removeEventListener('wheel',interaction,true);window.removeEventListener('touchmove',interaction,true);window.removeEventListener('compositionstart',compositionStart,true);window.removeEventListener('compositionend',compositionEnd,true);window.removeEventListener('blur',blur); };
  });

  async function toggleOptions() {
    optionsOpen=!optionsOpen;
    if(optionsOpen){await tick();optionsPanel?.querySelector<HTMLButtonElement>('.option-action:not(:disabled)')?.focus();}
  }
  function openDrive() { optionsOpen=false; window.open('/#drive','_blank'); }

  function requireSignIn() { needsSignIn = true; signIn = true; }
  async function documentSession() {
    const session = await loadSession();
    if (accountUid && session.uid !== accountUid) {
      requireSignIn();
      throw new Error('Sign in with the account that opened this document. Your edits are retained.');
    }
    needsSignIn = false;
    return session;
  }

  async function checkConnection() {
    const sequence=++connectionSequence;
    let reachable=false;
    if(navigator.onLine){
      const controller=new AbortController();const timeout=setTimeout(()=>controller.abort(),3000);
      try{reachable=(await fetch('/ping',{cache:'no-store',signal:controller.signal})).ok;}catch{/* LAN/server outage, not necessarily internet loss. */}
      finally{clearTimeout(timeout);}
    }
    if(disposed||sequence!==connectionSequence)return;
    if(offline&&reachable){notice=automaticSave?'Connection restored. Pending saves will retry with the same operation.':'Connection restored. Save to MX when ready.';if(ready&&dirty&&!conflict&&!needsSignIn&&automaticSave){error='';scheduleDraft();}}
    offline=!reachable;
  }

  async function prepareOffline() {
    offlineError = '';
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      try { await checkOfficeGraphics(); }
      catch { if(!disposed)offlineUnsupported=true; return; }
      if (!('serviceWorker' in navigator)) throw new Error('This browser cannot cache the editor for offline reopening. An open document can still be edited.');
      const prepare = async () => {
        const existing = await navigator.serviceWorker.getRegistration('/office/');
        try { await navigator.serviceWorker.register('/office/offline-worker.js', { scope: '/office/' }); }
        catch (reason) { if (!existing?.active) throw reason; }
        // Installation verifies every bundled asset; no document or API reply
        // enters this cache. A previously installed worker works disconnected.
        if (!existing?.active) await navigator.serviceWorker.ready;
      };
      await Promise.race([prepare(), new Promise<never>((_,reject)=>{
        timer=setTimeout(()=>reject(new Error('Offline preparation did not finish. Reconnect and retry. The open document is still editable; offline reopening is not ready yet.')),180_000);
      })]);
      if (!disposed) offlineReady = true;
    } catch (reason) { if(!disposed)offlineError = message(reason); }
    finally { clearTimeout(timer); }
  }
  function message(reason: unknown) { return reason instanceof Error ? reason.message : String(reason); }
  async function initialize() {
    if (!requestedUid) { try{drafts=await listDrafts();}catch(reason){recoveryError=message(reason);}status = 'Choose a recovery/offline copy, or open an Office file from MX Drive.';void prepareOffline();return; }
    try {
      loading = { phase: 'access', detail: 'Checking your MX account and looking for an offline copy of this document.' };
      const session = await documentSession(); accountUid = session.uid;
      if(openingCancelled || disposed)return;
      try{drafts=await listDrafts(accountUid,requestedUid);}catch(reason){recoveryError=message(reason);}
      const draft = drafts.find(value => value.accountUid===accountUid && value.item.uid===requestedUid);
      if (draft) { offered = draft; status = 'A recovery/offline copy exists on this device.'; return; }
      await openCurrent(requestedUid);
    } catch (reason) {
      if(openingCancelled || disposed)return;
      try{drafts=await listDrafts();}catch{/* Server error remains actionable. */}
      const draft = drafts.find(value => value.item.uid === requestedUid && (value.retainOffline??true));
      if (draft && !(reason instanceof ApiError && [401,403,404].includes(reason.status))) { offered = draft; status = 'MX is unreachable. You can continue with your offline copy.'; offline = true; }
      else { error = message(reason); loadingFailed=true; status = 'Open the editor from MX Drive after signing in.'; if (reason instanceof ApiError && reason.status === 401) requireSignIn(); }
    }
  }
  async function startEngine() {
    if (engineStarted) return;
    if(engineBoot)return engineBoot;
    status = 'Starting the bundled Office engine on your device…';
    loading = { phase: 'engine', detail: 'Loading the locally bundled editor and preparing WebAssembly. First launches can take longer; later launches can use this browser’s cache.' };
    engineBoot=engine.start(canvas).then(()=>{engineStarted=true;});
    return engineBoot;
  }
  function activity(){lastActivity=Date.now();if(dirty)scheduleDraft();}
  function backgroundDelay(){return officeIdleDelay(Date.now(),lastActivity,preparedAt,lastExportMs,heldPointers.size>0||heldKeys.size>0||composing||optionsOpen||(spreadsheet&&uncommittedInput));}
  async function captureSnapshot(acceptInput = true) {
    const inputAtSnapshot = inputSequence;
    const started=performance.now();
    preparedAt=Date.now();
    const exported = await engine.snapshot(extension,acceptInput);
    if(exported.blob!==snapshot?.blob)lastExportMs=performance.now()-started;
    if ((acceptInput||!spreadsheet) && inputSequence === inputAtSnapshot) uncommittedInput = false;
    return exported;
  }
  async function openCurrent(uid: string) {
    if(openingCancelled || disposed)return;
    openingController=new AbortController();
    const signal=openingController.signal;
    working = true; ready = false; error = ''; loadingFailed=false;
    openingFocus=true;
    loading = { phase: 'access', detail: 'Checking document permissions and the exact saved version to open.' };
    if(viewport)viewport.scrollLeft=0;
    try {
      await localWork.idle();
      const context = await apiJson<OpenContext>(`/mx/v1/drive/items/${encodeURIComponent(uid)}/office`,{signal});
      const ext = officeExtension(context.item.original_file_name || context.item.name);
      if (!ext) throw new Error('This Office format is not supported for editing.');
      item = context.item; extension = ext;
      checkOfficeInputSize(context.item.size);
      // Boot the public engine while the authenticated, exact-version document
      // downloads. Inspection still completes before any document is opened.
      const boot=startEngine();void boot.catch(()=>{});
      status = 'Loading your document from MX…';
      loading = { phase: 'document', detail: `Downloading saved revision ${context.item.revision} from your MX server. Your file is not being saved or modified.` };
      let progressAt = 0;
      let progressTimer: ReturnType<typeof setTimeout> | undefined;
      let received = { loaded: 0, total: null as number | null };
      const displayProgress = () => {
        clearTimeout(progressTimer); progressTimer=undefined; progressAt=performance.now();
        if(!disposed&&loading.phase==='document')loading={...loading,...received};
      };
      // Pin the exact persistent version selected in the metadata snapshot.
      const file = await apiFile(`/mx/v1/drive/items/${encodeURIComponent(uid)}/download?version_uid=${encodeURIComponent(context.version_uid)}`, { signal, onProgress: (loaded,total)=>{
        received={loaded,total}; const delay=100-(performance.now()-progressAt);
        if(loaded===0||loaded===total||delay<=0)displayProgress();
        else if(progressTimer===undefined)progressTimer=setTimeout(displayProgress,delay);
      } }).finally(()=>clearTimeout(progressTimer));
      snapshot = { blob: file.blob, generation: 0 };
      largeDocument=(await officeResources(file.blob,ext)).large;
      loading={phase:'engine',detail:'Document received. Finishing editor startup on your device.'};
      await boot;
      signal.throwIfAborted();
      loading = { phase: 'opening', detail: openingDetail(ext)+(largeDocument?' This is a large workbook: opening may take several minutes on your device. You can cancel without changing the original.':'') };
      await engine.open(file.blob, ext, readOnly);
      generation = 0; acknowledged = 0; snapshot = { blob: file.blob, generation: 0 };
      uncommittedInput = false;
      pendingFile = undefined; pendingOperation = undefined; pendingGeneration = -1; conflict = false; offered = null; ready = true; offline = false;
      pendingCopy = undefined; copyOperation = undefined; copying = false;
      saveConfirmation = '';
      activeDraftKey=draftKey(accountUid,item.uid,editorUid);storedAt=0;storedGeneration=-1;
      status = readOnly ? 'Read-only · you can download this document.' : `No unsaved changes · MX revision ${item.revision}`;
      document.title = `${item.name} · MX Office`;
      await showCanvas();
      if(openingCancelled || disposed)return;
      openingFocus=false;
      scheduleOfflinePreparation();
      if (persist) await checkpoint();
    } catch (reason) { engine.dispose();ready=false;openingFocus=false;error = message(reason); loadingFailed=true; status='Office could not open · download the loaded copy or return to MX Drive'; if (reason instanceof ApiError && reason.status === 401) requireSignIn(); }
    finally { working = false; }
  }
  async function openLocal(draft: OfficeDraft) {
    if(openingCancelled || disposed)return;
    openingController=new AbortController();
    working = true; ready = false; error = ''; loadingFailed=false; offered = null;
    openingFocus=true;
    loading = { phase: 'document', detail: 'Loading the offline copy saved in this browser. MX access and the latest revision will be checked when you save.', loaded: draft.blob.size, total: draft.blob.size };
    if(viewport)viewport.scrollLeft=0;
    try {
      await localWork.idle();
      item = draft.item; accountUid = draft.accountUid; extension = draft.extension; persist = draft.retainOffline??true;
      restoredDraftKey=draft.key;activeDraftKey=draftKey(accountUid,item.uid,editorUid);
      checkOfficeInputSize(draft.blob.size);
      snapshot = { blob: draft.blob, generation: 0 };
      largeDocument=(await officeResources(draft.blob,extension)).large;
      await startEngine();
      openingController.signal.throwIfAborted();
      loading = { phase: 'opening', detail: openingDetail(extension)+(largeDocument?' This large document may take several minutes on your device. Your saved local copy is retained.':'') };
      const openedSnapshot=await engine.open(draft.blob, extension, readOnly,true);
      generation = 0; acknowledged = draft.generation === draft.acknowledgedGeneration && !draft.saveFile && !draft.copyFile ? 0 : -1;
      snapshot = openedSnapshot; pendingFile = await recoveredUpload(draft.saveFile,draft.saveGeneration===draft.generation?openedSnapshot.blob:undefined);
      uncommittedInput = false; saveConfirmation = '';
      pendingGeneration = draft.saveFile && draft.saveGeneration === draft.generation ? 0 : -1;
      pendingOperation = draft.saveOperation; storedAt = draft.updatedAt; ready = true;
      storedGeneration=0;
      pendingCopy = await recoveredUpload(draft.copyFile,draft.copyGeneration===draft.generation?openedSnapshot.blob:undefined); copyOperation = draft.copyOperation;
      copyGeneration = draft.copyFile && draft.copyGeneration === draft.generation ? 0 : -1;
      if (pendingCopy) { copyName = pendingCopy.name; copying = true; }
      notice = 'Opened the local copy. MX will check the latest revision and your access before saving.';
      status = 'Offline copy opened'; document.title = `${item.name} · MX Office`;
      await showCanvas();
      if(openingCancelled || disposed)return;
      openingFocus=false;
      scheduleOfflinePreparation();
      if(dirty)scheduleDraft();
    } catch (reason) { engine.dispose();ready=false;openingFocus=false;error = message(reason); loadingFailed=true; status='Office could not open · your local copy is still downloadable'; }
    finally { working = false; }
  }
  function scheduleOfflinePreparation(){clearTimeout(offlineTimer);offlineTimer=setTimeout(()=>{if(!disposed&&!working&&backgroundDelay()===0)void prepareOffline();else if(!disposed)scheduleOfflinePreparation();},5000);}
  function scheduleDraft() {
    clearTimeout(automaticTimer);
    automaticTimer=undefined;
    if(disposed||readOnly)return;
    const delay=backgroundDelay();
    automaticTimer=setTimeout(()=>{automaticTimer=undefined;if(!ready||!dirty||working||disposed)return;
      if(backgroundDelay()>0){scheduleDraft();return;}
      if(automaticSave&&!offline&&!conflict&&!needsSignIn&&!copying)void save(false,false,true);
      else void checkpoint(false,false,true).catch(reason=>{recoveryError=message(reason);error='Recovery storage failed. Latest edits may exist only in this tab. Download a copy before closing.';status='Recovery not confirmed · latest edits may exist only in this tab';});
    },delay);
  }
  function cancelOpening() {
    openingCancelled=true;
    openingController?.abort(new Error('Opening cancelled. Your original file and saved local copies are unchanged.'));
    engine.dispose();ready=false;openingFocus=false;working=false;loadingFailed=true;error='Opening cancelled. Your original file and saved local copies are unchanged.';status='Opening cancelled';
  }
  async function focusDocument(initializeInput = false) {
    if(!ready || disposed)return;
    const sequence=++focusSequence;nativeFocused=false;
    await tick();await new Promise<void>(resolve=>requestAnimationFrame(()=>requestAnimationFrame(()=>resolve())));
    if(disposed || sequence!==focusSequence)return;
    try{await engine.focus(initializeInput);if(!disposed&&sequence===focusSequence)nativeFocused=true;}
    catch(reason){if(!disposed&&sequence===focusSequence){error=message(reason);}}
  }
  async function showCanvas() {
    await tick(); window.dispatchEvent(new Event('resize'));
    await new Promise<void>(resolve=>requestAnimationFrame(()=>requestAnimationFrame(()=>resolve())));
    // Establish native focus after layout, not before another resize which can
    // steal Calc's input focus while the first characters are being delivered.
    canvas.focus(); await focusDocument(true);
    if(!nativeFocused && !openingCancelled && !disposed)throw new Error('The document opened, but the editor could not establish input focus. No changes were saved. Retry opening or download the loaded copy.');
    // Opening a large file is not user idle time. Give the newly ready editor
    // its own interaction window, including when restoring an unsaved draft.
    activity();
  }
  async function checkpoint(force = false, preparedOnly = false, background = false) {
    return localWork.run(async () => {
      if (!item || !ready || disposed || (readOnly&&!persist)) return;
      if(background&&backgroundDelay()>0){status='Changes not yet saved to MX · background preparation is waiting while you work';return;}
      try {
      if (!readOnly && !preparedOnly && (!snapshot || snapshot.generation !== generation || uncommittedInput || force)) snapshot = await captureSnapshot(!background);
      if (!snapshot) return;
      activeDraftKey ||= draftKey(accountUid,item.uid,editorUid);
      const fingerprint=JSON.stringify([activeDraftKey,snapshot.generation,acknowledged,item.revision,pendingOperation,pendingGeneration,copyOperation,copyGeneration,persist]);
      if(!force&&fingerprint===recoveryFingerprint&&storedAt)return;
      const draft: OfficeDraft = { key: activeDraftKey, accountUid, item: $state.snapshot(item), extension,
        blob: snapshot.blob, generation: snapshot.generation, acknowledgedGeneration: acknowledged, updatedAt: Date.now(), saveFile: pendingFile, saveOperation: pendingOperation, saveGeneration: pendingGeneration,
        copyFile: pendingCopy, copyOperation, copyGeneration, retainOffline:persist,expiresAt:persist?undefined:Date.now()+7*86400000 };
      await storeDraft(draft); storedAt = draft.updatedAt;storedGeneration=snapshot.generation;recoveryError='';recoveryFingerprint=fingerprint;
      if(restoredDraftKey && restoredDraftKey!==activeDraftKey){await removeDraft(restoredDraftKey);restoredDraftKey='';}
      if(!saving&&dirty)status='Recovery copy saved on this device · not yet saved to MX';
      } catch (reason) { recoveryError = message(reason);throw reason; }
    });
  }
  async function toggleOffline(enabled: boolean) {
    if (!item) return;
    if (enabled && !confirm('Keep a copy of this document in this browser profile? Anyone using this profile may read it while offline. Only enable this on a trusted device.')) return;
    if (!enabled && !confirm('Remove this document’s offline copy from this device? Your open document and MX versions are kept.')) return;
    persist = enabled;
    if (enabled) {
      // Firefox can leave a persistence permission prompt pending indefinitely.
      // Durable-storage permission is best effort, never a gate on draft writes.
      try{await checkpoint();}catch(reason){error=`Offline copy was not confirmed: ${message(reason)}`;}
      void navigator.storage?.persist?.().catch(()=>{});
    }
    else { try { await localWork.idle();if(dirty)await checkpoint(false,true);else if(activeDraftKey){await removeDraft(activeDraftKey);storedAt=0;storedGeneration=-1;} } catch (reason) { recoveryError = message(reason); } }
  }
  async function save(useLatest = false, allowVolatile = false, background = false) {
    if (!item || !ready || readOnly) return;
    if(working){if(!background&&saving&&backgroundSaving)manualSaveQueued=true;return;}
    if(background&&backgroundDelay()>0){scheduleDraft();return;}
    if (pendingCopy) { copying = true; notice = 'Finish or cancel the pending separate-file save first.'; return; }
    clearTimeout(automaticTimer);automaticTimer=undefined;
    working = true; saving = true;backgroundSaving=background;directSave=false;saveStage='preparing';savePercent=0;saveSeconds=0;saveStartedAt=Date.now();saveConfirmation = ''; status = 'Preparing the document on your device…';
    let uploadStarted=false;let payloadRecovered=false;
    try {
      await localWork.idle();
      if(background&&backgroundDelay()>0){status='Changes not yet saved to MX · background preparation is waiting while you work';return;}
      let preparedSnapshot: OfficeSnapshot | null = null;
      // A Calc cell can still be in its input editor and not marked modified.
      // Explicit Save accepts it before checking generation. Background saves
      // wait for accepted cell input and never change its selection/input mode.
      if (!pendingFile) {
        const prepared=await localWork.run(() => background&&backgroundDelay()>0?Promise.resolve(null):captureSnapshot(!background));
        if(!prepared){status='Changes not yet saved to MX · background preparation is waiting while you work';return;}
        snapshot = preparedSnapshot = prepared;
        // A newer native modification can arrive while a snapshot is exporting.
        // Never move the live generation backwards to that older snapshot.
        generation = Math.max(generation, snapshot.generation);
        if (snapshot.generation === acknowledged && !useLatest) {
          status = generation !== acknowledged || uncommittedInput ? 'Newer edits still need saving · save again when ready' : 'No unsaved changes to save';
          return;
        }
      }
      error = ''; notice = '';
      if (useLatest) {
        const latest = await apiJson<OpenContext>(`/mx/v1/drive/items/${item.uid}/office`);
        item = { ...item, revision: latest.item.revision, permission: latest.item.permission };
        if (readOnly) throw new Error('You no longer have permission to edit this file.');
        pendingFile = undefined; pendingOperation = undefined; conflict = false;
      }
      if (!pendingFile) {
        // Deliberate conflict resolution discarded the previous retry payload.
        snapshot = preparedSnapshot || await localWork.run(() => captureSnapshot());
        pendingGeneration = snapshot.generation;
        pendingFile = new File([snapshot.blob], item.original_file_name || `document${extension}`, { type: officeFormats[extension].mime });
        pendingOperation = crypto.randomUUID();
      }
      // No second export here: preserve exactly the prepared retry payload,
      // operation ID and original base revision before uncertain network work.
      saveStage='local';status='Saving a recovery copy on this device…';
      try{await checkpoint(false,true);payloadRecovered=true;}catch(reason){if(!allowVolatile){directSave=true;throw new Error('Browser recovery storage failed. Upload has not started. Download a copy, free browser storage, or explicitly save directly to MX.');}notice='Saving without a device recovery copy. Keep this tab open until MX confirms the save.';}
      await documentSession();
      saveStage='uploading';status=payloadRecovered?'Recovery copy saved on this device · uploading to MX…':'Uploading to MX without a device recovery copy…';uploadStarted=true;
      const next = await uploadDriveFile(pendingFile, { accountUid, parentUid: item.parent_uid, versionOf: $state.snapshot(item), operationUid: pendingOperation,
        onProgress: value => { savePercent=Math.round(value.total?value.loaded/value.total*100:0);saveStage=value.phase==='finalizing'?'confirming':'uploading';status = value.phase === 'finalizing' ? 'Confirming the saved MX version…' : `Uploading to MX · ${savePercent}%${storedAt?' · recovery copy on this device':''}`; } });
      item = next; acknowledged = pendingGeneration; engine.committed(pendingGeneration); pendingFile = undefined; pendingOperation = undefined; offline = false;
      const newerEdits = generation !== acknowledged || uncommittedInput;
      conflict = false; status = newerEdits ? 'Saved snapshot · newer edits are not saved yet' : 'All changes saved to MX';
      saveConfirmation = savedToMx(next.revision, Date.now(), newerEdits);
      if(persist||newerEdits)await checkpoint(false,true).catch(()=>{});
      else if(activeDraftKey){await removeDraft(activeDraftKey).catch(reason=>recoveryError=message(reason));storedAt=0;storedGeneration=-1;}
    } catch (reason) {
      error = message(reason);
      if (reason instanceof ApiError && reason.status === 409) conflict = true;
      if (reason instanceof ApiError && reason.status === 401) requireSignIn();
      if (reason instanceof ConnectionError) offline = true;
      status = `${uploadStarted?'Save not confirmed; retry checks the same operation':'Save not started'} · ${payloadRecovered?'recovery copy saved on this device':'latest edits may exist only in this open tab'}`;
    } finally { working = false; saving = false;backgroundSaving=false;saveStage='idle';if(manualSaveQueued&&!disposed){manualSaveQueued=false;void save();}else if(dirty&&!error)scheduleDraft(); }
  }
  async function downloadCopy() {
    if (!item || working || (!ready && !snapshot)) return;
    working = true;
    try {
      await localWork.idle();
      const copy = readOnly || !ready ? snapshot : await localWork.run(() => captureSnapshot());
      if (!copy) return;
      const url = URL.createObjectURL(copy.blob); const link = document.createElement('a');
      link.href = url; link.download = item.name.toLowerCase().endsWith(extension) ? item.name : `${item.name}${extension}`; link.click();
      setTimeout(() => URL.revokeObjectURL(url), 30_000);
    } catch (reason) { error = message(reason); }
    finally { working = false; }
  }
  function offerCopy() {
    if(!ready || !item || readOnly || working || copying)return;
    copyName = pendingCopy?.name || `${item.name.replace(/\.[^.]+$/,'')} (copy)`; copying=true;
  }
  async function saveCopy() {
    if (!item || !ready || working || readOnly || !copyName.trim()) return;
    working = true; saving = true;saveStage='preparing';saveSeconds=0;saveStartedAt=Date.now(); error=''; notice=''; saveConfirmation=''; status='Preparing separate file on your device…';
    try {
      await localWork.idle();
      await documentSession();
      if (!pendingCopy) {
        snapshot = await localWork.run(() => captureSnapshot());
        const name = copyName.trim().toLowerCase().endsWith(extension) ? copyName.trim() : `${copyName.trim()}${extension}`;
        pendingCopy = new File([snapshot.blob],name,{type:officeFormats[extension].mime});
        copyGeneration = snapshot.generation; copyOperation = crypto.randomUUID();
      }
      saveStage='local';await checkpoint(false,true);
      const originalKey = activeDraftKey;
      const copy = await uploadDriveFile(pendingCopy,{accountUid,parentUid:null,operationUid:copyOperation,
        onProgress:value=>{savePercent=Math.round(value.total?value.loaded/value.total*100:0);saveStage=value.phase==='finalizing'?'confirming':'uploading';status=value.phase==='finalizing'?'Confirming separate file…':`Saving copy to MX · ${savePercent}%`;}});
      item = copy; acknowledged = copyGeneration; engine.committed(acknowledged);
      pendingFile=undefined; pendingOperation=undefined; pendingCopy=undefined; copyOperation=undefined; conflict=false; copying=false;
      activeDraftKey=draftKey(accountUid,item.uid,editorUid);
      try { await removeDraft(originalKey);storedAt=0;storedGeneration=-1; } catch(reason){recoveryError=message(reason);}
      const newerEdits=generation!==acknowledged || uncommittedInput;
      offline=false; status=newerEdits?'Saved copy · newer edits are not saved yet':'All changes saved to MX';
      saveConfirmation=savedToMx(copy.revision,Date.now(),newerEdits);
      notice='Saved as a separate file in your My Drive. The original file was not changed.';if(persist||newerEdits)await checkpoint(false,true).catch(()=>{});
    } catch (reason) {
      error=message(reason); if(reason instanceof ApiError && reason.status===401)requireSignIn();
      if(reason instanceof ConnectionError)offline=true;
      status='Copy not confirmed · retry keeps the same file and operation';
    }
    finally { working=false; saving=false;saveStage='idle';if(dirty&&!error)scheduleDraft(); }
  }
  async function login(event: SubmitEvent) {
    event.preventDefault(); working=true; error='';
    try { const auth=await authenticate(email,password,factor); password=''; factor=''; if(accountUid&&auth.uid!==accountUid){requireSignIn();throw new Error('Sign in with the account that opened this document.');} needsSignIn=false; signIn=false; notice='Signed in. Your edits are still open; save when ready.'; if(!ready&&requestedUid)await initialize(); }
    catch(reason){error=message(reason);password='';}
    finally{working=false;}
  }
  async function deleteLocal(draft: OfficeDraft) {
    if(!confirm(`Remove the offline copy of ${draft.item.name} from this device? MX files are not changed.`))return;
    try{await removeDraft(draft.key);drafts=drafts.filter(value=>value.key!==draft.key);}
    catch(reason){offlineError=message(reason);}
  }
</script>

<svelte:window bind:innerWidth={viewportWidth}/>
<main>
  <UpdateNotice compact isAdmin={false} onOpenUpdates={()=>{}}/>
  <header class="toolbar">
    <div class="identity"><FileText size={16}/><strong title={item?.name || 'MX Office'}>{item?.name || 'MX Office'}</strong></div>
    <div class="save-feedback" class:saved={!!saveConfirmation && !saving} class:pending={dirty && !saving} class:failed={!!error} role="status" aria-live="polite" title={feedback}>
      {#if saveConfirmation && !saving && !dirty}<CheckCircle size={14}/>{:else}<Save size={14}/>{/if}
      <span aria-hidden="true">{compactStatus}</span><span class="sr-only">{feedback}</span>
      {#if selectedCell}<small class="sr-only" aria-label="Selected cell">{selectedCell}</small>{/if}
    </div>
    {#if offline}<span class="disconnected" title="MX is unreachable. Your open document is retained." aria-label="MX is offline"><WifiOff size={15}/></span>{/if}
    <button class="primary save-button" aria-label={saving?'Saving…':'Save to MX'} title="Save to MX (Ctrl/Cmd+S)" disabled={!ready || working || readOnly || !dirty || conflict} onclick={()=>void save()}><Save size={15}/><span>{saving?'Saving…':'Save'}</span></button>
    {#if needsSignIn && !signIn}<button class="sign-in-button" onclick={()=>signIn=true}>Sign in to save</button>{/if}
    <button class="options-button" bind:this={optionsButton} aria-label="Document options" title="Document options" aria-expanded={optionsOpen} aria-controls="document-options" onclick={()=>void toggleOptions()}><MoreHorizontal size={19}/></button>
  </header>
  <div id="document-options" class="document-options" hidden={!optionsOpen} bind:this={optionsPanel} role="dialog" aria-label="Document options" tabindex="-1">
    <div class="options-heading"><strong>Document options</strong><button class="icon-button" aria-label="Close document options" onclick={()=>{optionsOpen=false;optionsButton?.focus();}}><X size={16}/></button></div>
    <button class="option-action" onclick={openDrive}><FolderOpen size={17}/>Open from Drive</button>
    <button class="option-action" disabled={(!ready && !snapshot) || working} onclick={()=>{optionsOpen=false;void downloadCopy();}}><Download size={17}/>Download copy</button>
    {#if !readOnly}<button class="option-action" disabled={!ready || working} onclick={()=>{optionsOpen=false;offerCopy();}}><Save size={17}/>Save as a copy</button>{/if}
    <section class="offline-settings" aria-label="Offline settings">
      <label><input type="checkbox" bind:checked={automaticSave} onchange={()=>{if(automaticSave)scheduleDraft();}}/>Automatically save to MX when idle</label>
      <small>Background preparation waits for a pause in typing, scrolling and dragging. Finish a spreadsheet cell with Enter/Tab, or use Ctrl/Cmd+S to save unfinished input. Large exports can still briefly occupy the editor.</small>
      <small>Recovery is automatic and separate from offline retention. Copies expire after 7 days and are cleaned on the next recovery scan; a confirmed save removes its automatic copy. Other people using this browser profile may access them. Changes since the last checkpoint may be lost if the device stops.</small>
      <label><input type="checkbox" checked={persist} disabled={!ready || working} onchange={async event=>{const input=event.currentTarget;await toggleOffline(input.checked);input.checked=persist;}}/>Keep offline copy on this device</label>
      <small>{offlineReady?'Editor available offline':offlineUnsupported?'Offline editor unsupported in this browser':offlineError?'Offline reopening unavailable':'Offline preparation starts after the document opens'}{storedAt?` · Recovery checkpoint ${new Date(storedAt).toLocaleTimeString()}`:''}</small>
      {#if recoveryError}<p class="recovery-warning" role="alert">{recoveryError} Latest edits are not protected on this device.</p>{/if}
      {#if offlineError}<details class="offline-details"><summary>Offline reopening unavailable · details and retry</summary><p>{offlinePreparationSummary(offlineError)}</p><details><summary>Technical details</summary><p>{offlineError}</p></details>{#if !offlineReady}<button onclick={()=>void prepareOffline()}>Retry offline preparation</button>{/if}</details>{/if}
    </section>
    <p class="save-details">{feedback}</p>
    {#if presentation}<p class="read-only-details">Presentations are read-only in this build. Presentation editing has not passed validation; download a copy to edit locally.</p>{/if}
    {#if viewportWidth<canvasWidth}<section class="scroll-controls" aria-label="Editor scrolling"><span>Scroll the desktop-sized editor</span><div><button aria-label="Scroll editor left" onclick={()=>viewport?.scrollBy({left:-240})}><ArrowLeft size={16}/></button><button aria-label="Scroll editor right" onclick={()=>viewport?.scrollBy({left:240})}><ArrowRight size={16}/></button></div></section>{/if}
  </div>
  <div class="messages">
  {#if error}<div class="alert" role="alert"><span>{error}</span>{#if recoveryError&&snapshot}<button disabled={working} onclick={()=>void downloadCopy()}>Download recovery copy</button>{/if}<button class="icon-button" aria-label="Dismiss error" onclick={()=>error=''}><X size={16}/></button></div>{/if}
  {#if directSave}<div class="notice"><span>Device recovery is unavailable. A direct save cannot survive closing this tab before MX confirms it.</span><button disabled={working} onclick={()=>{if(confirm('Save without a recovery copy on this device? Keep this tab open until MX confirms the save.'))void save(false,true);}}>Save directly to MX</button></div>{/if}
  {#if notice}<div class="notice" role="status"><span>{notice}</span><button class="icon-button" aria-label="Dismiss message" onclick={()=>notice=''}><X size={16}/></button></div>{/if}
  {#if conflict}<div class="conflict" role="alert"><strong>This file changed in MX. Your edits have not overwritten it.</strong><p>You can keep both files, or deliberately add your edits as a new version after the latest saved version. Another simultaneous change will be checked again.</p><div><button onclick={()=>{copyName=`${item?.name.replace(/\.[^.]+$/,'')} (my copy)`;copying=true;}}>Save as separate file</button><button disabled={working} onclick={()=>{if(confirm('Add your edited document as the new current version? The other user’s version remains in history.'))void save(true);}}>Save my edits as the next version</button><button disabled={working} onclick={()=>{if(item&&confirm('Discard the open edits and load the current MX version? Download a copy first if you want to keep them.'))void openCurrent(item.uid);}}>Discard edits and load latest</button></div></div>{/if}
  {#if copying}<form class="inline-form" onsubmit={event=>{event.preventDefault();void saveCopy();}}><label>New file name<input required disabled={!!pendingCopy} bind:value={copyName}/></label><button class="primary" disabled={working}>Save copy to My Drive</button><button type="button" disabled={working} onclick={()=>{if(!pendingCopy){copying=false;}else if(confirm('Stop retrying this copy? It may already exist in My Drive if the server response was lost. Your open edits are kept.')){pendingCopy=undefined;copyOperation=undefined;copying=false;void checkpoint().catch(reason=>error=message(reason));}}}>Cancel</button></form>{/if}
  {#if signIn}<form class="inline-form" onsubmit={login}><label>Email<input type="email" autocomplete="username" required bind:value={email}/></label><label>Password<input type="password" autocomplete="current-password" required bind:value={password}/></label><label>OTP or recovery code<input autocomplete="one-time-code" bind:value={factor}/></label><button disabled={working}>Sign in to save</button><button type="button" onclick={()=>signIn=false}>Cancel</button></form>{/if}
  </div>
  {#if offered}<section class="recovery"><h1>Continue your local copy?</h1><p>{offered.item.name} · saved on this device {new Date(offered.updatedAt).toLocaleString()}</p><p>It may contain changes that were not saved to MX. Interrupted saves reuse their original operation to check whether MX already committed them. Your access and the original revision are checked.</p><button class="primary" onclick={()=>void openLocal(offered!)}>Open local copy</button><button disabled={offline} onclick={()=>{if(offered&&confirm('Discard this local draft and open the current MX version?')){const uid=offered.item.uid;void removeDraft(offered.key).then(()=>openCurrent(uid));}}}>Discard local draft and open MX version</button></section>
  {:else if !requestedUid && !ready && !working && !item}<section class="recovery"><h1>Offline documents</h1><p>Only documents you explicitly kept on this device appear here. Open other files from MX Drive while connected.</p>{#each drafts as draft}<div class="draft-row"><button onclick={()=>void openLocal(draft)}>{draft.item.name}<small>{new Date(draft.updatedAt).toLocaleString()}</small></button><button onclick={()=>void deleteLocal(draft)}>Remove local copy</button></div>{:else}<p>No offline copies have been saved in this browser profile.</p>{/each}<a href="/">Open MX</a></section>{/if}
  <div class="editor-stage" bind:this={viewport} class:inactive={!ready} role="region" aria-label="Scrollable Office editor"><canvas id="qtcanvas" bind:this={canvas} style:width={`${canvasWidth}px`} contenteditable="true" aria-label="Office document" data-native-focused={nativeFocused} onfocus={()=>void focusDocument()} oncontextmenu={event=>event.preventDefault()} onkeydown={event=>event.preventDefault()} onwheel={event=>event.preventDefault()}></canvas>{#if (!ready || openingFocus) && !offered}{#if requestedUid || item}<OfficeLoading progress={loading} fileName={item?.name || ''} failed={loadingFailed} onCancel={cancelOpening} onRetry={()=>location.reload()} onDownload={snapshot?()=>void downloadCopy():item?()=>void driveDownload(item!).catch(reason=>error=message(reason)):undefined} downloadLabel={snapshot?'Download loaded copy':'Download original'} onReturn={()=>location.assign('/#drive')} />{:else}<div class="loading" role="status">{status}</div>{/if}{/if}</div>
</main>

<style>
  :global(html),:global(body){margin:0;height:100%;font:14px system-ui,sans-serif;background:#f7f8fa;color:#20242b;}
  :global(html[data-theme='dark']){color-scheme:dark;}
  :global(html[data-theme='dark'] body){background:#16191d;color:#e5e7eb;}
  main{display:flex;flex-direction:column;height:100dvh;min-width:0;position:relative;}
  .toolbar{display:flex;align-items:center;gap:10px;height:38px;flex:0 0 38px;box-sizing:border-box;padding:0 8px;border-bottom:1px solid #d9dee6;background:#fff;font-size:12px;z-index:20;}
  .identity{display:flex;gap:7px;align-items:center;min-width:0;flex:1;}.identity strong{font-weight:500;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;}strong{overflow-wrap:anywhere;}.identity :global(svg){flex-shrink:0;color:#657285;}
  button{display:inline-flex;align-items:center;justify-content:center;gap:6px;border:1px solid #cdd4df;background:#fff;color:inherit;padding:7px 10px;border-radius:6px;font:inherit;cursor:pointer;}button:disabled{opacity:.5;cursor:default;}button:focus-visible,a:focus-visible,input:focus-visible,summary:focus-visible{outline:2px solid #4773c9;outline-offset:2px;}.primary{background:#1d4ed8;color:white;border-color:#1d4ed8;}
  .toolbar button{height:28px;padding:3px 9px;flex-shrink:0;}.toolbar .options-button{width:28px;padding:0;border-color:transparent;background:transparent;}.options-button:hover{background:#eef1f6;}.disconnected{display:flex;color:#926008;}
  .save-feedback{display:flex;align-items:center;gap:5px;color:#657285;font-size:11px;flex-shrink:0;white-space:nowrap;}.save-feedback.saved{color:#24633b;}.save-feedback.pending{color:#926008;}.save-feedback.failed{color:#a32828;}
  .sr-only{position:absolute;width:1px;height:1px;padding:0;margin:-1px;overflow:hidden;clip:rect(0,0,0,0);white-space:nowrap;border:0;}
  .document-options{position:absolute;right:8px;top:44px;width:340px;max-width:calc(100% - 16px);max-height:calc(100dvh - 52px);box-sizing:border-box;overflow:auto;padding:10px;background:#fff;border:1px solid #d9dee6;border-radius:9px;box-shadow:0 8px 30px #0002;z-index:40;font-size:13px;}
  .options-heading{display:flex;align-items:center;justify-content:space-between;padding:0 4px 8px;}.icon-button{padding:4px;flex-shrink:0;border:0;background:transparent;}.option-action{display:flex;width:100%;justify-content:start;border:0;padding:10px;background:transparent;text-align:left;}.option-action:hover:not(:disabled){background:#eef1f6;}
  .offline-settings{margin-top:6px;padding:12px 5px 4px;border-top:1px solid #d9dee6;}.offline-settings label{display:flex;align-items:center;gap:7px;}.offline-settings small{display:block;margin:8px 0;color:#657285;font-size:11px;line-height:1.5;}input[type=checkbox]{accent-color:#1d4ed8;}
  .offline-details{font-size:12px;color:#785917;}.offline-details summary{cursor:pointer;}.offline-details p{line-height:1.5;overflow-wrap:anywhere;}.offline-details details{margin-bottom:10px;}
  .save-details,.read-only-details{margin:10px 5px 3px;font-size:12px;line-height:1.5;overflow-wrap:anywhere;color:#657285;}.scroll-controls{display:flex;align-items:center;justify-content:space-between;gap:8px;margin-top:12px;padding:10px 5px 0;border-top:1px solid #d9dee6;font-size:12px;}.scroll-controls>div{display:flex;gap:5px;}
  .messages{position:absolute;bottom:36px;left:50%;transform:translateX(-50%);width:calc(100% - 24px);max-width:760px;max-height:calc(100% - 88px);overflow:auto;z-index:30;pointer-events:none;}
  .messages>*{pointer-events:auto;border:1px solid #d9dee6;border-radius:8px;box-shadow:0 4px 18px #0002;margin-top:8px;}
  .alert,.notice,.conflict{padding:10px 12px;background:#fff0f0;color:#a32828;overflow-wrap:anywhere;}.alert,.notice{display:flex;align-items:start;gap:12px;}.alert>span,.notice>span{flex:1;}.conflict{background:#fff7e5;color:#785917;}.notice{background:#eef5ef;color:#34634d;}.conflict p{margin:6px 0 10px;}.conflict>div{display:flex;gap:8px;flex-wrap:wrap;}
  .inline-form{display:flex;align-items:end;gap:8px;flex-wrap:wrap;padding:12px;background:#fff;}.inline-form label{display:flex;flex-direction:column;gap:4px;max-width:100%;}input:not([type=checkbox]){min-width:0;background:transparent;color:inherit;border:1px solid #b9c3d2;border-radius:5px;padding:8px;font:inherit;}
  .editor-stage{flex:1;min-height:180px;min-width:0;position:relative;overflow-x:auto;overflow-y:hidden;background:#e7e7e7;}.editor-stage.inactive{min-height:0;}canvas{display:block;box-sizing:border-box;height:100%;border:0;padding:0;outline:0;}canvas:focus{outline:0;}.inactive canvas{visibility:hidden;}.loading{position:absolute;inset:0;display:grid;place-items:center;padding:20px;color:#444;pointer-events:none;text-align:center;}
  .recovery{padding:24px;max-width:800px;margin:auto;}.recovery h1{font-size:22px;}.recovery p{line-height:1.6;}.recovery button{margin-right:6px;}.draft-row{display:flex;gap:8px;margin:10px 0;}.draft-row>button:first-child{flex:1;flex-direction:column;align-items:start;}.draft-row small{font-size:11px;opacity:.7;}
  :global(html[data-theme='dark']) .toolbar,:global(html[data-theme='dark']) .document-options,:global(html[data-theme='dark']) .inline-form,:global(html[data-theme='dark']) button:not(.primary){background:#20242a;}:global(html[data-theme='dark']) .toolbar,:global(html[data-theme='dark']) .document-options,:global(html[data-theme='dark']) .offline-settings,:global(html[data-theme='dark']) .scroll-controls,:global(html[data-theme='dark']) .messages>*,:global(html[data-theme='dark']) button{border-color:#3e4754;}
  :global(html[data-theme='dark']) .icon-button,:global(html[data-theme='dark']) .option-action,:global(html[data-theme='dark']) .options-button{background:transparent;}:global(html[data-theme='dark']) .option-action:hover:not(:disabled),:global(html[data-theme='dark']) .options-button:hover{background:#303741;}
  :global(html[data-theme='dark']) .save-feedback,:global(html[data-theme='dark']) .offline-settings small,:global(html[data-theme='dark']) .save-details,:global(html[data-theme='dark']) .read-only-details{color:#a5b0bf;}:global(html[data-theme='dark']) .save-feedback.saved{color:#a4d4b7;}:global(html[data-theme='dark']) .save-feedback.pending,:global(html[data-theme='dark']) .disconnected,:global(html[data-theme='dark']) .offline-details{color:#e5c982;}:global(html[data-theme='dark']) .save-feedback.failed{color:#ffb6b6;}
  :global(html[data-theme='dark']) .alert{background:#352023;color:#ffb6b6;}:global(html[data-theme='dark']) .conflict{background:#322c20;color:#e5c982;}:global(html[data-theme='dark']) .notice{background:#1d3026;color:#a4d4b7;}
  @media(max-width:700px){.toolbar{gap:6px;}.identity{gap:5px;}button{padding:7px 9px;font-size:12px;}.recovery{padding:16px;}.inline-form{padding:10px;}}
  @media(max-width:430px){.save-button span{display:none;}.toolbar .save-button{width:28px;padding:0;}.identity :global(svg){display:none;}.sign-in-button{font-size:10px;}}
</style>
