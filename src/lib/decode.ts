/** Decode PNG bytes into an ImageBitmap. */
export async function decodePng(bytes: Uint8Array): Promise<ImageBitmap> {
  const blob = new Blob([bytes as BlobPart], { type: "image/png" });
  return createImageBitmap(blob);
}

/** Convert raw RGBA pixels into an ImageBitmap. */
export async function bitmapFromRgba(rgba: Uint8Array, width: number, height: number): Promise<ImageBitmap> {
  return createImageBitmap(new ImageData(new Uint8ClampedArray(rgba), width, height));
}

/** Decode the raw overlay payload: 8-byte little-endian dimensions followed by RGBA. */
export async function decodeOverlayFrame(payload: Uint8Array): Promise<ImageBitmap> {
  if (payload.byteLength < 8) throw new Error("截图数据不完整");
  const header = new DataView(payload.buffer, payload.byteOffset, 8);
  const width = header.getUint32(0, true);
  const height = header.getUint32(4, true);
  const expected = width * height * 4;
  if (payload.byteLength !== 8 + expected) throw new Error("截图尺寸与像素数据不匹配");

  const rgba = new Uint8ClampedArray(payload.buffer, payload.byteOffset + 8, expected);
  return createImageBitmap(new ImageData(rgba, width, height));
}

export async function blobToBytes(blob: Blob): Promise<Uint8Array> {
  return new Uint8Array(await blob.arrayBuffer());
}
