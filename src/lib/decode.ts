/** Decode PNG bytes into an ImageBitmap. */
export async function decodePng(bytes: Uint8Array): Promise<ImageBitmap> {
  const blob = new Blob([bytes as BlobPart], { type: "image/png" });
  return createImageBitmap(blob);
}

/** Convert raw RGBA pixels into an ImageBitmap. */
export async function bitmapFromRgba(rgba: Uint8Array, width: number, height: number): Promise<ImageBitmap> {
  return createImageBitmap(new ImageData(new Uint8ClampedArray(rgba), width, height));
}

export async function blobToBytes(blob: Blob): Promise<Uint8Array> {
  return new Uint8Array(await blob.arrayBuffer());
}
