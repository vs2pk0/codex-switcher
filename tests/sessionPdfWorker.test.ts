import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { Worker } from "node:worker_threads";
import test from "node:test";
import ts from "typescript";
import { pdfCaptureSlices } from "../src/sessionPdfWorker.ts";

// 用真实 jsPDF 执行后台分页代码；画布编码用固定图片替身，避免依赖桌面环境。
const require = createRequire(import.meta.url);
const source = readFileSync(new URL("../src/workers/sessionPdf.worker.ts", import.meta.url), "utf8");
const code = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
function createWorker() {
  return new Worker(`
    const {parentPort} = require('node:worker_threads');
    const exports = {};
    const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=', 'base64');
    global.self = {postMessage: (message, transfer) => parentPort.postMessage(message, transfer)};
    global.OffscreenCanvas = class {
      constructor(width,height) { this.width=width; this.height=height; }
      getContext() {return {drawImage() {}};}
      async convertToBlob() { return new Blob([png], {type:'image/png'}); }
    };
    parentPort.on('message', data => {
      if(data.bitmap) data.bitmap.close = () => {};
      self.onmessage({data});
    });
    ${code.replace('require("jspdf")', `require(${JSON.stringify(require.resolve("jspdf"))})`)}
  `, { eval: true });
}

test("PDF worker preserves queued batch order and generates a valid multi-page document", async () => {
  const worker = createWorker();
  try {
    const messages: any[] = [];
    const completed = new Promise<void>((resolve,reject) => {
      worker.on("error", reject);
      worker.on("message", message => {
        messages.push(message);
        if(message.ready) {
          // 连续发两批和结束请求，验证 worker 自身也会串行消费。
          worker.postMessage({id:1,bitmap:{width:724,height:2200}});
          worker.postMessage({id:2,bitmap:{width:724,height:300}});
          worker.postMessage({id:3,finish:true});
        }
        if(message.error) reject(new Error(message.error));
        if(message.id===3) resolve();
      });
    });
    await completed;
    assert.deepEqual(messages.filter(m=>m.id).map(m=>m.id), [1,2,3]);
    const buffer = messages.at(-1).buffer;
    assert.ok(buffer instanceof ArrayBuffer);
    const pdf = Buffer.from(buffer).toString("latin1");
    assert.ok(pdf.startsWith("%PDF-"));
    assert.match(pdf, /\/Count 3\b/);
    assert.ok(pdf.includes("%%EOF"));
  } finally { await worker.terminate(); }
});

test("long PDF captures are bounded and cover every pixel exactly once", () => {
  for (const height of [1, 1400, 1401, 100000, 1400.1]) {
    const slices = [...pdfCaptureSlices(height)];
    let end = 0;
    for (const slice of slices) {
      assert.equal(slice.y, end);
      assert.ok(slice.height > 0 && slice.height <= 1400);
      end += slice.height;
    }
    assert.equal(end, Math.ceil(height));
    assert.equal(slices.filter(slice => slice.last).length, 1);
    assert.equal(slices.at(-1)?.last, true);
  }
  for (const invalid of [0, -1, NaN, Infinity]) assert.throws(() => [...pdfCaptureSlices(invalid)]);
});
