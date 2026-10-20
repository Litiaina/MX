/** Desktop delivery is best-effort; durable MX notifications remain authoritative.
 * Focus belongs to the account across tabs, not to whichever tab wins a timer.
 */
export type DesktopStatus = 'ready'|'unsupported'|'insecure'|'denied'|'permission-needed'|'focused'|'disabled'|'duplicate'|'submitted'|'failed';
export type DesktopAttempt = {status:DesktopStatus;at:number};
const lastAttempts=new Map<string,DesktopAttempt>();
export function lastDesktopAttempt(accountUid:string):DesktopAttempt|undefined { return lastAttempts.get(accountUid); }
export function desktopCapability(): DesktopStatus {
  if (!window.isSecureContext) return 'insecure';
  if (!('Notification' in window)) return 'unsupported';
  return Notification.permission === 'granted' ? 'ready' : Notification.permission === 'denied' ? 'denied' : 'permission-needed';
}
export function desktopExplanation(status: DesktopStatus): string {
  return ({ready:'Ready while MX is unfocused or minimized. Keep MX open; operating-system Do Not Disturb and browser background suspension can prevent alerts.',unsupported:'This browser does not provide desktop notifications. MX notifications remain available in the notification center.',insecure:'Desktop notifications require a trusted HTTPS connection. Install/trust your organization’s certificate; bypassing a certificate warning may not be sufficient.',denied:'Notifications are blocked for this site. Allow them in your browser’s site settings and check operating-system notification settings.', 'permission-needed':'Allow notifications for this site using Enable permission below.',focused:'MX is in focus. Notifications stay inside MX; no operating-system popup is sent.',disabled:'Desktop notifications are disabled in your MX preferences.',duplicate:'This notification was already handled by another MX tab.',submitted:'Submitted to the browser. The operating system controls whether a banner is displayed.',failed:'The browser rejected desktop delivery. Check site permission, the HTTPS certificate and operating-system notification settings.'})[status];
}
type Alert = { uid:string; title:string; body:string; icon?:string; onOpen:()=>void };
export class DesktopNotifications {
  readonly #tab = crypto.randomUUID();
  readonly #focusKey: string;
  readonly #deliveredKey: string;
  readonly #open = new Map<string, Notification>();
  readonly #memory = new Set<string>();
  #database:Promise<IDBDatabase|null>|undefined;
  #heartbeat: ReturnType<typeof setInterval>|undefined;
  #stopped = false;
  constructor(private readonly accountUid:string) {
    this.#focusKey = `mx-focus:${accountUid}`;
    this.#deliveredKey = `mx-desktop-delivered:${accountUid}`;
  }
  focused = () => document.visibilityState === 'visible' && document.hasFocus();
  #read<T>(key:string, fallback:T):T { try { return JSON.parse(localStorage.getItem(key)||'null') ?? fallback; } catch { return fallback; } }
  #publishFocus = () => {
    if (this.focused()) {
      try { localStorage.setItem(this.#focusKey,JSON.stringify({tab:this.#tab,at:Date.now()})); } catch { /* Per-tab gating still applies. */ }
      this.close();
    } else this.#releaseFocus();
  };
  #releaseFocus() {
    try { if(this.#read<{tab?:string}>(this.#focusKey,{}).tab===this.#tab)localStorage.removeItem(this.#focusKey); } catch { /* Storage is optional. */ }
  }
  #otherFocused():boolean {
    const active=this.#read<{tab?:string;at?:number}>(this.#focusKey,{});
    return !!active.tab && active.tab!==this.#tab && Number(active.at)>Date.now()-8000;
  }
  start() {
    this.#stopped=false;
    this.#publishFocus();
    window.addEventListener('focus',this.#publishFocus);
    window.addEventListener('blur',this.#publishFocus);
    document.addEventListener('visibilitychange',this.#publishFocus);
    this.#heartbeat=setInterval(this.#publishFocus,2000);
  }
  stop() {
    this.#stopped=true; clearInterval(this.#heartbeat);
    window.removeEventListener('focus',this.#publishFocus);
    window.removeEventListener('blur',this.#publishFocus);
    document.removeEventListener('visibilitychange',this.#publishFocus);
    this.#releaseFocus();this.close();void this.#database?.then(database=>database?.close());
  }
  async #claimDurable(uid:string):Promise<boolean|null> {
    if(!('indexedDB' in window))return null;
    this.#database??=new Promise(resolve=>{
      try {
        const request=indexedDB.open('mx-desktop-delivery-v1',1);
        let abandoned=false;
        request.onupgradeneeded=()=>{const store=request.result.createObjectStore('receipts',{keyPath:'key'});store.createIndex('at','at');};
        request.onsuccess=()=>{
          if(abandoned||this.#stopped){request.result.close();resolve(null);return;}
          request.result.onversionchange=()=>request.result.close();resolve(request.result);
        };
        request.onerror=()=>resolve(null);request.onblocked=()=>{abandoned=true;resolve(null);};
      }catch{resolve(null);}
    });
    const database=await this.#database;if(!database||this.#stopped)return null;
    // Unique insertion in a read/write transaction is authoritative across
    // content processes. A Web Lock alone does not flush another tab's buffered
    // localStorage snapshot (notably during Firefox reconnects).
    return new Promise(resolve=>{
      try{
        const transaction=database.transaction('receipts','readwrite'),store=transaction.objectStore('receipts');let claimed=false;
        const receipt=store.add({key:`${this.accountUid}:${uid}`,at:Date.now()});
        receipt.onsuccess=()=>{claimed=true;};
        receipt.onerror=event=>{if(receipt.error?.name==='ConstraintError'){event.preventDefault();event.stopPropagation();}};
        const count=store.count();count.onsuccess=()=>{let excess=count.result-4096;if(excess>0){const oldest=store.index('at').openCursor();oldest.onsuccess=()=>{const cursor=oldest.result;if(cursor&&excess-->0){cursor.delete();cursor.continue();}};}};
        transaction.oncomplete=()=>resolve(claimed);transaction.onabort=()=>resolve(null);transaction.onerror=()=>resolve(null);
      }catch{resolve(null);}
    });
  }
  close(prefix='') { for(const [uid,notification] of this.#open)if(uid.startsWith(prefix)){notification.close();this.#open.delete(uid);} }
  async deliver(alert:Alert, enabled:boolean, sound?:()=>void):Promise<DesktopStatus> {
    if(this.#stopped)return this.#result('disabled');
    const focusedAtArrival=this.focused();
    if(!this.focused()&&this.#otherFocused())return this.#result('focused');
    const execute=async():Promise<DesktopStatus>=>{
      if(this.#stopped)return 'disabled';
      // Recheck after acquiring the lock: focus can change while queued.
      if(!this.focused()&&this.#otherFocused())return 'focused';
      const delivered=this.#read<Record<string,number>>(this.#deliveredKey,{});
      if(this.#memory.has(alert.uid)||delivered[alert.uid])return 'duplicate';
      const durable=await this.#claimDurable(alert.uid);
      if(this.#stopped)return 'disabled';if(durable===false)return 'duplicate';
      this.#memory.add(alert.uid);
      if(this.#memory.size>512)this.#memory.delete(this.#memory.values().next().value!);
      const entries=Object.entries({...delivered,[alert.uid]:Date.now()}).filter(([,at])=>at>Date.now()-86400000).sort((a,b)=>b[1]-a[1]).slice(0,512);
      try{localStorage.setItem(this.#deliveredKey,JSON.stringify(Object.fromEntries(entries)));}catch{/* In-memory de-duplication remains active. */}
      try { sound?.(); } catch { /* A failed optional sound must not block OS delivery. */ }
      // Receipt persistence can yield while the user switches applications.
      // An alert received in focused MX must not become a late OS popup.
      if(focusedAtArrival||this.focused()||this.#otherFocused())return 'focused';
      if(!enabled)return 'disabled';
      const capability=desktopCapability();
      if(capability!=='ready')return capability;
      try{
        const notification=new Notification(alert.title,{body:alert.body,icon:alert.icon,tag:`mx:${this.accountUid}:${alert.uid}`,silent:true});
        this.#open.set(alert.uid,notification);
        notification.onclick=()=>{if(!this.#stopped){window.focus();alert.onOpen();}notification.close();this.#open.delete(alert.uid);};
        notification.onclose=()=>this.#open.delete(alert.uid);
        notification.onerror=()=>{this.#status('failed');this.#open.delete(alert.uid);};
        return 'submitted';
      }catch{return 'failed';}
    };
    // Locks serialize account delivery across tabs without delayed timers that
    // are unreliable in minimized windows. Storage fallback is best-effort.
    let status:DesktopStatus;
    try { status=navigator.locks ? await navigator.locks.request(`mx-desktop:${this.accountUid}`,execute) : await execute(); }
    catch { status=await execute(); } // Some private/embedded contexts reject locks.
    return this.#result(status);
  }
  #result(status:DesktopStatus):DesktopStatus {this.#status(status);return status;}
  #status(status:DesktopStatus) {
    lastAttempts.set(this.accountUid,{status,at:Date.now()});
    if(lastAttempts.size>16)lastAttempts.delete(lastAttempts.keys().next().value!);
    window.dispatchEvent(new CustomEvent('mx-desktop-status',{detail:status}));
  }
}
