/**
 * Script untuk generate icon.ico dari icon.png menggunakan sharp + png-to-ico
 * Jalankan: node build/generate-ico.mjs
 */
import sharp from 'sharp';
import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const srcPng = path.join(__dirname, 'icon.png');
const destIco = path.join(__dirname, 'icon.ico');

// ICO file terdiri dari satu atau lebih gambar PNG dalam berbagai ukuran
const sizes = [16, 32, 48, 64, 128, 256];

async function generateIco() {
  console.log('Generating icon.ico from icon.png...');

  // Buat buffer PNG untuk setiap ukuran
  const pngBuffers = await Promise.all(
    sizes.map(size =>
      sharp(srcPng)
        .resize(size, size, { fit: 'contain', background: { r: 0, g: 0, b: 0, alpha: 0 } })
        .png()
        .toBuffer()
    )
  );

  // Buat file ICO secara manual
  // Format ICO: https://en.wikipedia.org/wiki/ICO_(file_format)
  const numImages = pngBuffers.length;
  const headerSize = 6; // ICONDIR
  const dirEntrySize = 16; // ICONDIRENTRY per gambar
  const dataOffset = headerSize + dirEntrySize * numImages;

  // Hitung total size
  let totalSize = dataOffset;
  const offsets = [];
  for (const buf of pngBuffers) {
    offsets.push(totalSize);
    totalSize += buf.length;
  }

  const icoBuffer = Buffer.alloc(totalSize);
  let pos = 0;

  // ICONDIR header
  icoBuffer.writeUInt16LE(0, pos); pos += 2;     // reserved = 0
  icoBuffer.writeUInt16LE(1, pos); pos += 2;     // type = 1 (icon)
  icoBuffer.writeUInt16LE(numImages, pos); pos += 2; // count

  // ICONDIRENTRY untuk setiap gambar
  for (let i = 0; i < numImages; i++) {
    const size = sizes[i];
    const buf = pngBuffers[i];
    icoBuffer.writeUInt8(size >= 256 ? 0 : size, pos); pos += 1; // width (0 = 256)
    icoBuffer.writeUInt8(size >= 256 ? 0 : size, pos); pos += 1; // height
    icoBuffer.writeUInt8(0, pos); pos += 1;   // color count
    icoBuffer.writeUInt8(0, pos); pos += 1;   // reserved
    icoBuffer.writeUInt16LE(1, pos); pos += 2; // planes
    icoBuffer.writeUInt16LE(32, pos); pos += 2; // bit count
    icoBuffer.writeUInt32LE(buf.length, pos); pos += 4; // size of image data
    icoBuffer.writeUInt32LE(offsets[i], pos); pos += 4; // offset of image data
  }

  // Data gambar
  for (const buf of pngBuffers) {
    buf.copy(icoBuffer, pos);
    pos += buf.length;
  }

  writeFileSync(destIco, icoBuffer);
  console.log(`✅ icon.ico generated at: ${destIco}`);
  console.log(`   Sizes: ${sizes.join(', ')} px`);
  console.log(`   Total: ${(totalSize / 1024).toFixed(1)} KB`);
}

generateIco().catch(err => {
  console.error('Failed to generate icon.ico:', err);
  process.exit(1);
});
