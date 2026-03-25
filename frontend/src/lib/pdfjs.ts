// pdfjs-dist の唯一の import 元
// - アプリコードはこのモジュール経由でのみ pdfjs-dist にアクセスする
// - テストではこのモジュールのみをモックすることで Worker 等の副作用を隔離

import { GlobalWorkerOptions, getDocument } from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.mjs?url";

GlobalWorkerOptions.workerSrc = workerUrl;

export { getDocument };
export type {
  PDFDocumentProxy,
  PDFDocumentLoadingTask,
  PDFPageProxy,
  RenderTask,
  PageViewport,
} from "pdfjs-dist";
