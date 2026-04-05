// E2Eテスト用フィクスチャデータを生成するスクリプト
// 実行: npx tsx e2e/fixtures/generate-fixtures.ts
//
// 生成するファイル:
// - pictures/ (JPEG x3 + large.png) — セットジャンプ + CGスクロールテスト
// - gallery/ (JPEG x2) — セットジャンプテスト用セット2
// - archive/images.zip (JPEG x3) — アーカイブテスト用
// - archive/mixed.zip (JPEG + MP4) — アーカイブ+動画テスト用
// - videos/ (MP4 x2 + unsupported.mkv) — 動画タブテスト用
// - docs/sample.pdf (2ページ) + corrupted.pdf — PDFテスト用
// - nested/dirs/sub1/ + nested/dirs/sub2/ + nested/extra/sub3/ — ネストナビゲーション用
// - empty/ — エッジケース

import fs from "node:fs";
import path from "node:path";
import { deflateSync } from "node:zlib";

const OUT_DIR = path.resolve(import.meta.dirname, "test-data");

// --- 最小バイナリ定義 (tests/conftest.py から流用) ---

// 最小 JPEG (JFIF ヘッダ + EOI)
const MINIMAL_JPEG = Buffer.from([
  0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01,
  0x01, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0xff, 0xd9,
]);

// 最小 MP4 (ftyp ボックスのみ + パディング)
const MINIMAL_MP4 = Buffer.concat([
  Buffer.from([
    0x00, 0x00, 0x00, 0x14, // size=20
    0x66, 0x74, 0x79, 0x70, // type=ftyp
    0x69, 0x73, 0x6f, 0x6d, // brand=isom
    0x00, 0x00, 0x00, 0x00, // minor_version
    0x69, 0x73, 0x6f, 0x6d, // compatible_brand
  ]),
  Buffer.alloc(100),
]);

// 最小 PDF (2ページ)
function generateMinimalPdf(): Buffer {
  const lines = [
    "%PDF-1.4",
    "1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj",
    "2 0 obj<</Type/Pages/Kids[3 0 R 4 0 R]/Count 2>>endobj",
    "3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]>>endobj",
    "4 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]>>endobj",
    "xref",
    "0 5",
    "0000000000 65535 f ",
    "0000000009 00000 n ",
    "0000000058 00000 n ",
    "0000000115 00000 n ",
    "0000000182 00000 n ",
    "trailer<</Size 5/Root 1 0 R>>",
    "startxref",
    "249",
    "%%EOF",
  ];
  return Buffer.from(lines.join("\n"));
}

// CG スクロールテスト用の縦長 PNG (グレースケール、全黒)
// ブラウザが確実にデコードできる有効な PNG を生成する
// viewport width 1024 で表示すると、アスペクト比に応じて高さが viewport を超える
function generateTallPng(width: number, height: number): Buffer {
  const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);

  // IHDR (13 bytes)
  const ihdrData = Buffer.alloc(13);
  ihdrData.writeUInt32BE(width, 0);
  ihdrData.writeUInt32BE(height, 4);
  ihdrData[8] = 8;  // bit depth
  ihdrData[9] = 0;  // color type: greyscale
  const ihdr = createPngChunk("IHDR", ihdrData);

  // IDAT: 各行 = filter byte (0=None) + width ピクセル (全 0 = 黒)
  const rawData = Buffer.alloc(height * (1 + width), 0);
  const compressed = deflateSync(rawData);
  const idat = createPngChunk("IDAT", compressed);

  const iend = createPngChunk("IEND", Buffer.alloc(0));

  return Buffer.concat([signature, ihdr, idat, iend]);
}

// PNG チャンク構築: length(4) + type(4) + data + crc32(4)
function createPngChunk(type: string, data: Buffer): Buffer {
  const typeBuffer = Buffer.from(type, "ascii");
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length, 0);
  const crcValue = crc32(Buffer.concat([typeBuffer, data]));
  const crcBuf = Buffer.alloc(4);
  crcBuf.writeUInt32BE(crcValue, 0);
  return Buffer.concat([length, typeBuffer, data, crcBuf]);
}

// 最小 MKV (EBML ヘッダのみ) — ブラウザ非対応フォーマットテスト用
const MINIMAL_MKV = Buffer.from([
  0x1a, 0x45, 0xdf, 0xa3, // EBML ID
  0x93, // size = 19
  0x42, 0x86, 0x81, 0x01, // EBMLVersion = 1
  0x42, 0xf7, 0x81, 0x01, // EBMLReadVersion = 1
  0x42, 0xf2, 0x81, 0x04, // EBMLMaxIDLength = 4
  0x42, 0xf3, 0x81, 0x08, // EBMLMaxSizeLength = 8
  0x42, 0x82, 0x84, 0x6d, 0x61, 0x74, 0x72, // DocType = "matr" (truncated)
]);

// 破損 PDF — エラー表示テスト用
const CORRUPTED_PDF = Buffer.from("%PDF-1.4\nCORRUPTED_DATA_HERE\n%%EOF\n");

// --- ZIP 生成 (Node.js 標準の zlib は ZIP 形式未対応なので手動構築) ---

// ZIP ローカルファイルヘッダ + セントラルディレクトリを構築
function createZip(entries: Array<{ name: string; data: Buffer }>): Buffer {
  const localHeaders: Buffer[] = [];
  const centralHeaders: Buffer[] = [];
  let offset = 0;

  for (const entry of entries) {
    const nameBuffer = Buffer.from(entry.name, "utf-8");

    // ローカルファイルヘッダ
    const localHeader = Buffer.alloc(30 + nameBuffer.length + entry.data.length);
    localHeader.writeUInt32LE(0x04034b50, 0); // signature
    localHeader.writeUInt16LE(20, 4); // version needed
    localHeader.writeUInt16LE(0, 6); // flags
    localHeader.writeUInt16LE(0, 8); // compression (stored)
    localHeader.writeUInt16LE(0, 10); // mod time
    localHeader.writeUInt16LE(0, 12); // mod date
    localHeader.writeUInt32LE(crc32(entry.data), 14); // crc32
    localHeader.writeUInt32LE(entry.data.length, 18); // compressed size
    localHeader.writeUInt32LE(entry.data.length, 22); // uncompressed size
    localHeader.writeUInt16LE(nameBuffer.length, 26); // name length
    localHeader.writeUInt16LE(0, 28); // extra length
    nameBuffer.copy(localHeader, 30);
    entry.data.copy(localHeader, 30 + nameBuffer.length);
    localHeaders.push(localHeader);

    // セントラルディレクトリヘッダ
    const centralHeader = Buffer.alloc(46 + nameBuffer.length);
    centralHeader.writeUInt32LE(0x02014b50, 0); // signature
    centralHeader.writeUInt16LE(20, 4); // version made by
    centralHeader.writeUInt16LE(20, 6); // version needed
    centralHeader.writeUInt16LE(0, 8); // flags
    centralHeader.writeUInt16LE(0, 10); // compression
    centralHeader.writeUInt16LE(0, 12); // mod time
    centralHeader.writeUInt16LE(0, 14); // mod date
    centralHeader.writeUInt32LE(crc32(entry.data), 16); // crc32
    centralHeader.writeUInt32LE(entry.data.length, 20); // compressed size
    centralHeader.writeUInt32LE(entry.data.length, 24); // uncompressed size
    centralHeader.writeUInt16LE(nameBuffer.length, 28); // name length
    centralHeader.writeUInt16LE(0, 30); // extra length
    centralHeader.writeUInt16LE(0, 32); // comment length
    centralHeader.writeUInt16LE(0, 34); // disk number start
    centralHeader.writeUInt16LE(0, 36); // internal attrs
    centralHeader.writeUInt32LE(0, 38); // external attrs
    centralHeader.writeUInt32LE(offset, 42); // local header offset
    nameBuffer.copy(centralHeader, 46);
    centralHeaders.push(centralHeader);

    offset += localHeader.length;
  }

  const centralDirOffset = offset;
  const centralDirSize = centralHeaders.reduce((sum, h) => sum + h.length, 0);

  // EOCD (End of Central Directory)
  const eocd = Buffer.alloc(22);
  eocd.writeUInt32LE(0x06054b50, 0); // signature
  eocd.writeUInt16LE(0, 4); // disk number
  eocd.writeUInt16LE(0, 6); // disk with central dir
  eocd.writeUInt16LE(entries.length, 8); // entries on this disk
  eocd.writeUInt16LE(entries.length, 10); // total entries
  eocd.writeUInt32LE(centralDirSize, 12); // central dir size
  eocd.writeUInt32LE(centralDirOffset, 16); // central dir offset
  eocd.writeUInt16LE(0, 20); // comment length

  return Buffer.concat([...localHeaders, ...centralHeaders, eocd]);
}

// CRC-32 (ZIP 用)
function crc32(data: Buffer): number {
  let crc = 0xffffffff;
  for (const byte of data) {
    crc ^= byte;
    for (let j = 0; j < 8; j++) {
      crc = crc & 1 ? (crc >>> 1) ^ 0xedb88320 : crc >>> 1;
    }
  }
  return (crc ^ 0xffffffff) >>> 0;
}

// --- ディレクトリ構造生成 ---

function ensureDir(dir: string): void {
  fs.mkdirSync(dir, { recursive: true });
}

function writeFile(filePath: string, data: Buffer): void {
  ensureDir(path.dirname(filePath));
  fs.writeFileSync(filePath, data);
  console.log(`  ${path.relative(OUT_DIR, filePath)}`);
}

function main(): void {
  console.log(`テストデータを生成: ${OUT_DIR}\n`);

  // 既存データをクリーン
  if (fs.existsSync(OUT_DIR)) {
    fs.rmSync(OUT_DIR, { recursive: true });
  }

  // pictures/ — セットジャンプ対象 (セット1) + CG スクロールテスト用
  writeFile(path.join(OUT_DIR, "pictures", "photo1.jpg"), MINIMAL_JPEG);
  writeFile(path.join(OUT_DIR, "pictures", "photo2.jpg"), MINIMAL_JPEG);
  writeFile(path.join(OUT_DIR, "pictures", "photo3.jpg"), MINIMAL_JPEG);
  writeFile(path.join(OUT_DIR, "pictures", "large.png"), generateTallPng(8, 24));

  // gallery/ — セットジャンプ対象 (セット2)
  writeFile(path.join(OUT_DIR, "gallery", "art1.jpg"), MINIMAL_JPEG);
  writeFile(path.join(OUT_DIR, "gallery", "art2.jpg"), MINIMAL_JPEG);

  // archive/images.zip — 画像のみ
  const imagesZip = createZip([
    { name: "page01.jpg", data: MINIMAL_JPEG },
    { name: "page02.jpg", data: MINIMAL_JPEG },
    { name: "page03.jpg", data: MINIMAL_JPEG },
  ]);
  writeFile(path.join(OUT_DIR, "archive", "images.zip"), imagesZip);

  // archive/mixed.zip — 画像 + 動画
  const mixedZip = createZip([
    { name: "thumb.jpg", data: MINIMAL_JPEG },
    { name: "clip.mp4", data: MINIMAL_MP4 },
  ]);
  writeFile(path.join(OUT_DIR, "archive", "mixed.zip"), mixedZip);

  // videos/ — 動画タブテスト用 + MKV 非対応フォーマットテスト
  writeFile(path.join(OUT_DIR, "videos", "clip1.mp4"), MINIMAL_MP4);
  writeFile(path.join(OUT_DIR, "videos", "clip2.mp4"), MINIMAL_MP4);
  writeFile(path.join(OUT_DIR, "videos", "unsupported.mkv"), MINIMAL_MKV);

  // docs/ — PDFテスト用 + 破損 PDF エラーテスト + セットジャンプ対象
  writeFile(path.join(OUT_DIR, "docs", "sample.pdf"), generateMinimalPdf());
  writeFile(path.join(OUT_DIR, "docs", "corrupted.pdf"), CORRUPTED_PDF);
  writeFile(path.join(OUT_DIR, "docs", "zzz_next.pdf"), generateMinimalPdf());

  // nested/dirs/ + nested/extra/ — ネストナビゲーション + first-viewable テスト用
  writeFile(path.join(OUT_DIR, "nested", "dirs", "sub1", "deep.jpg"), MINIMAL_JPEG);
  writeFile(path.join(OUT_DIR, "nested", "dirs", "sub2", "wide.jpg"), MINIMAL_JPEG);
  writeFile(path.join(OUT_DIR, "nested", "extra", "sub3", "extra.jpg"), MINIMAL_JPEG);

  // empty/ — エッジケース
  ensureDir(path.join(OUT_DIR, "empty"));
  console.log("  empty/");

  console.log("\n完了");
}

main();
