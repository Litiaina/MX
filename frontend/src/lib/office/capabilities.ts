// The threaded Qt engine needs WebGL inside a worker, not merely WebGL on the
// main page or the existence of OffscreenCanvas. Probe the real capability on
// a disposable canvas before downloading/starting the large Office runtime.
let graphics: Promise<void> | undefined;
export function checkOfficeGraphics(): Promise<void> {
  return graphics ??= probeGraphics();
}
async function probeGraphics(): Promise<void> {
  if (typeof Worker === 'undefined' || typeof OffscreenCanvas === 'undefined' ||
      !HTMLCanvasElement.prototype.transferControlToOffscreen) {
    throw new Error('This browser does not support the graphics workers required by MX Office. Download a copy to edit locally, or use a compatible browser.');
  }
  const source = new Blob([`onmessage = ({data: canvas}) => {
    try {
      const gl = canvas.getContext('webgl');
      const supported = !!gl && !!gl.getParameter(gl.VERSION);
      postMessage(supported);
    } catch { postMessage(false); }
  };`], { type: 'text/javascript' });
  const url = URL.createObjectURL(source);
  let worker: Worker | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    const supported = await new Promise<boolean>((resolve) => {
      worker = new Worker(url);
      worker.onmessage = event => resolve(event.data === true);
      worker.onerror = event => { event.preventDefault(); resolve(false); };
      timer = setTimeout(() => resolve(false), 5000);
      const canvas = document.createElement('canvas').transferControlToOffscreen();
      worker.postMessage(canvas, [canvas]);
    });
    if (!supported) throw new Error('This browser or graphics driver cannot provide WebGL in an Office worker. Your file is unchanged. Download a copy to edit locally, or use a compatible browser.');
  } finally {
    clearTimeout(timer); worker?.terminate(); URL.revokeObjectURL(url);
  }
}
