import { officeFormats } from './formats';
import { installOfficeCompatibility } from './compatibility';
import { checkOfficeGraphics } from './capabilities';
interface EngineModule {
  canvas: HTMLCanvasElement; uno_scripts: string[]; locateFile: (name: string) => string;
  mainScriptUrlOrBlob: Blob; uno_main: Promise<MessagePort>;
  onAbort: (message: string) => void; printErr: (message: string) => void;
}
interface EngineFS { mkdir: (path: string) => void; writeFile: (path: string, bytes: Uint8Array) => void; readFile: (path: string) => Uint8Array }
declare global { interface Window { Module: EngineModule; FS: EngineFS } }
export interface OfficeSnapshot { blob: Blob; generation: number }
export class OfficeEngine {
  private port?: MessagePort;
  private sequence = 0;
  private restoreCompatibility?: () => void;
  private runtimeError?: (event: ErrorEvent) => void;
  private canvasResize?: ResizeObserver;
  private resizeFrame?: number;
  private pending = new Map<number, { resolve: (message: Record<string, unknown>) => void; reject: (error: Error) => void; timer: ReturnType<typeof setTimeout> }>();
  constructor(private modified: (generation: number) => void, private fatal: (message: string) => void, private saveRequested: (copy: boolean) => void, private announce: (message: string) => void, private selected: (cell: string) => void = () => {}) {}
  async start(canvas: HTMLCanvasElement): Promise<void> {
    if (!isSecureContext || !crossOriginIsolated || typeof SharedArrayBuffer === 'undefined') throw new Error('MX Office requires HTTPS and browser isolation headers. Open it directly from MX; check reverse-proxy headers if needed.');
    await checkOfficeGraphics();
    this.restoreCompatibility = installOfficeCompatibility();
    const vendor = `${location.origin}/office/vendor/`;
    let ready!: () => void;
    let failed!: (reason: Error) => void;
    const boot = new Promise<void>((resolve, reject) => { ready = resolve; failed = reject; });
    const fail = (message: string) => {
      const error = new Error(message); failed(error); this.fatal(message);
      for(const pending of this.pending.values()){clearTimeout(pending.timer);pending.reject(error);}
      this.pending.clear();
    };
    this.runtimeError = (event: ErrorEvent) => fail(`Office runtime error: ${event.message || 'browser runtime failure'}`);
    window.addEventListener('error', this.runtimeError);
    window.Module = {
      canvas, uno_scripts: [`${vendor}zeta.js`, `${location.origin}/office/office-thread.js`],
      locateFile: (name) => vendor + name,
      mainScriptUrlOrBlob: new Blob([`importScripts('${vendor}soffice.js');`], { type: 'text/javascript' }),
      uno_main: undefined as unknown as Promise<MessagePort>,
      onAbort: (message) => fail(`The Office engine stopped: ${message}`),
      printErr: (message) => console.warn('Office:', message)
    };
    const script = document.createElement('script'); script.src = `${vendor}soffice.js`;
    script.onerror = () => fail('The bundled Office engine could not be loaded. Reconnect once to prepare offline editing.');
    script.onload = () => {
      window.Module.uno_main.then((port) => {
        this.port = port;
        console.info('MX Office: engine channel connected');
        port.onmessage = ({ data }) => {
          if (data.cmd === 'ready') { console.info('MX Office: document engine ready'); ready(); return; }
          if (data.cmd === 'modified') { this.modified(Number(data.generation)); return; }
          if (data.cmd === 'selection') { this.selected(String(data.cell || '')); return; }
          if (data.cmd === 'save-request' || data.cmd === 'save-copy-request') { this.saveRequested(data.cmd === 'save-copy-request'); return; }
          if (data.cmd === 'open-request') { this.announce('Open another document from MX Drive in a separate editor tab. This editor stays bound to its original file.'); return; }
          const pending = this.pending.get(data.id);
          if (pending) { clearTimeout(pending.timer); this.pending.delete(data.id); if (data.cmd === 'error') pending.reject(new Error(data.message)); else pending.resolve(data); }
        };
      }).catch((error) => fail(String(error)));
    };
    document.body.append(script);
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      await Promise.race([boot, new Promise<never>((_, reject) => { timer = setTimeout(() => reject(new Error('Office startup took too long. Your saved drafts are retained.')), 120_000); })]);
      // Save/status/recovery bars change canvas height without resizing the
      // browser window. Keep native rendering and pointer hit targets aligned.
      this.canvasResize = new ResizeObserver(() => {
        if(this.resizeFrame !== undefined)cancelAnimationFrame(this.resizeFrame);
        this.resizeFrame = requestAnimationFrame(() => { this.resizeFrame=undefined;window.dispatchEvent(new Event('resize')); });
      });
      this.canvasResize.observe(canvas);
    }
    finally { clearTimeout(timer); }
  }
  private request(cmd: string, extra: Record<string, unknown>): Promise<Record<string, unknown>> {
    if (!this.port) return Promise.reject(new Error('Office is not ready.'));
    const id = ++this.sequence;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { this.pending.delete(id); reject(new Error('Office did not acknowledge this operation. Your document is still open.')); }, 60_000);
      this.pending.set(id, { resolve, reject, timer }); this.port!.postMessage({ cmd, id, ...extra });
    });
  }
  async open(blob: Blob, extension: string, readOnly: boolean) {
    if (!officeFormats[extension]) throw new Error('This file format is not supported for editing.');
    try { window.FS.mkdir('/tmp/mx-office'); } catch { /* Directory already exists. */ }
    window.FS.writeFile(`/tmp/mx-office/document${extension}`, new Uint8Array(await blob.arrayBuffer()));
    await this.request('open', { extension, readOnly });
    window.dispatchEvent(new Event('resize'));
  }
  async snapshot(extension: string): Promise<OfficeSnapshot> {
    const format = officeFormats[extension];
    const message = await this.request('export', { extension, filter: format.filter });
    const bytes = window.FS.readFile(`/tmp/mx-office/export${extension}`);
    return { blob: new Blob([new Uint8Array(bytes)], { type: format.mime }), generation: Number(message.generation) };
  }
  committed(generation: number) { this.port?.postMessage({ cmd: 'committed', generation }); }
  async focus() { if(this.port)await this.request('focus', {}); }
  dispose() { this.canvasResize?.disconnect();if(this.resizeFrame !== undefined)cancelAnimationFrame(this.resizeFrame);for (const pending of this.pending.values()) { clearTimeout(pending.timer); pending.reject(new Error('Editor closed.')); } this.pending.clear(); this.port?.close(); this.restoreCompatibility?.(); if(this.runtimeError)window.removeEventListener('error',this.runtimeError); }
}
