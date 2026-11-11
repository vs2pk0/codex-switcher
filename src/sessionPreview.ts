export const SESSION_MESSAGE_PREVIEW_CHARS = 12_000;

export function sessionPreviewPageSize(sizeBytes = 0): number {
  return sizeBytes >= 32 * 1024 * 1024 ? 5 : 20;
}

export function sessionMessagePreview(text: string, limit = SESSION_MESSAGE_PREVIEW_CHARS): string {
  let end = Math.min(text.length, limit);
  // Keep a supplementary Unicode character intact at the preview boundary.
  const last = text.charCodeAt(end - 1);
  const next = text.charCodeAt(end);
  if (last >= 0xd800 && last <= 0xdbff && next >= 0xdc00 && next <= 0xdfff) end -= 1;
  return text.slice(0, end);
}
