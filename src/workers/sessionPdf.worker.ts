import { jsPDF } from "jspdf";

// DOM 截图在主线程执行；分页、JPEG 编码及 PDF 序列化在此线程执行。
const scope = self as unknown as {
  onmessage: ((event: MessageEvent) => void) | null;
  postMessage: (message: unknown, transfer?: Transferable[]) => void;
};
const pdf = new jsPDF({ orientation: "portrait", unit: "pt", format: "a4", compress: true });
let y = 28;
let queue = Promise.resolve();

async function append(bitmap: ImageBitmap, gapAfter: boolean): Promise<void> {
  const margin = 28;
  const width = pdf.internal.pageSize.getWidth() - margin * 2;
  const height = pdf.internal.pageSize.getHeight();
  const scale = width / bitmap.width;
  try {
    for (let sourceY = 0; sourceY < bitmap.height;) {
      if (height - margin - y < 80) {
        pdf.addPage();
        y = margin;
      }
      const sliceHeight = Math.min(bitmap.height - sourceY, Math.max(1, Math.floor((height - margin - y) / scale)));
      const slice = new OffscreenCanvas(bitmap.width, sliceHeight);
      const context = slice.getContext("2d");
      if (!context) throw new Error("PDF canvas context unavailable");
      context.drawImage(bitmap, 0, sourceY, bitmap.width, sliceHeight, 0, 0, bitmap.width, sliceHeight);
      const jpeg = await slice.convertToBlob({ type: "image/jpeg", quality: 0.9 });
      pdf.addImage(new Uint8Array(await jpeg.arrayBuffer()), "JPEG", margin, y, width, sliceHeight * scale);
      slice.width = slice.height = 0;
      y += sliceHeight * scale;
      sourceY += sliceHeight;
      if (sourceY < bitmap.height) {
        pdf.addPage();
        y = margin;
      }
    }
    if (gapAfter) y += 8;
  } finally {
    bitmap.close();
  }
}

scope.onmessage = (event) => {
  const { id, bitmap, finish, gapAfter = true } = event.data;
  queue = queue.then(async () => {
    try {
      if (finish) {
        const buffer = pdf.output("arraybuffer");
        scope.postMessage({ id, buffer }, [buffer]);
      } else {
        await append(bitmap, gapAfter);
        scope.postMessage({ id });
      }
    } catch (error) {
      scope.postMessage({ id, error: String(error) });
    }
  });
};
// 能力预检，失败时在提交任何页面前回退到兼容路径。
void (async () => {
  try {
    const probe = new OffscreenCanvas(1, 1);
    if (!probe.getContext("2d")) throw new Error("PDF canvas context unavailable");
    await probe.convertToBlob({ type: "image/jpeg" });
    scope.postMessage({ ready: true });
  } catch (error) {
    scope.postMessage({ error: String(error) });
  }
})();
