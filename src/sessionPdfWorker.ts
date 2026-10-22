export interface PdfAssembler {
  append(canvas: HTMLCanvasElement, gapAfter?: boolean): Promise<void>;
  finish(): Promise<ArrayBuffer>;
  dispose(): void;
}

export async function createPdfAssembler(): Promise<PdfAssembler | null> {
  if (typeof Worker === "undefined" || typeof OffscreenCanvas === "undefined" || typeof createImageBitmap !== "function") return null;
  let worker: Worker;
  try {
    worker = new Worker(new URL("./workers/sessionPdf.worker.ts", import.meta.url), { type: "module" });
  } catch {
    return null;
  }
  let sequence = 0;
  const pending = new Map<number, { resolve: (buffer?: ArrayBuffer) => void; reject: (error: Error) => void }>();
  let failed: Error | null = null;
  let readyResolve!: () => void;
  let readyReject!: (error: Error) => void;
  const ready = new Promise<void>((resolve, reject) => { readyResolve = resolve; readyReject = reject; });
  const fail = (error: Error) => {
    failed = error;
    readyReject(error);
    for (const request of pending.values()) request.reject(error);
    pending.clear();
  };
  worker.onerror = () => fail(new Error("PDF worker failed"));
  worker.onmessageerror = () => fail(new Error("PDF worker message failed"));
  worker.onmessage = ({ data }) => {
    if (data.ready) { readyResolve(); return; }
    if (data.id === undefined && data.error) { fail(new Error(data.error)); return; }
    const request = pending.get(data.id);
    if (!request) return;
    pending.delete(data.id);
    if (data.error) request.reject(new Error(data.error));
    else request.resolve(data.buffer);
  };
  const timeout = setTimeout(() => fail(new Error("PDF worker initialization timed out")), 10000);
  try { await ready; } catch { worker.terminate(); return null; } finally { clearTimeout(timeout); }
  const request = (message: object, transfer: Transferable[] = []) => new Promise<ArrayBuffer | undefined>((resolve, reject) => {
    if (failed) { reject(failed); return; }
    const id = ++sequence;
    pending.set(id, { resolve, reject });
    try { worker.postMessage({ ...message, id }, transfer); } catch (error) { pending.delete(id); reject(error); }
  });
  return {
    async append(canvas, gapAfter = true) {
      const bitmap = await createImageBitmap(canvas);
      try { await request({ bitmap, gapAfter }, [bitmap]); } finally { bitmap.close(); }
    },
    async finish() {
      const buffer = await request({ finish: true });
      if (!buffer) throw new Error("PDF worker returned no document");
      return buffer;
    },
    dispose() { worker.terminate(); fail(new Error("PDF worker disposed")); },
  };
}

/** CSS 像素坐标分段，避免单条超长消息突破 WebView 的画布尺寸上限。 */
export function* pdfCaptureSlices(height: number): Generator<{ y: number; height: number; last: boolean }> {
  const total = Math.ceil(height);
  if (!Number.isFinite(total) || total <= 0) throw new Error("Invalid PDF capture height");
  for (let y = 0; y < total; y += 1400) {
    const sliceHeight = Math.min(1400, total - y);
    yield { y, height: sliceHeight, last: y + sliceHeight === total };
  }
}
