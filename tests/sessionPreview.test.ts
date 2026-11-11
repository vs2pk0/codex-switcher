import assert from "node:assert/strict";
import test from "node:test";
import { SESSION_MESSAGE_PREVIEW_CHARS, sessionMessagePreview, sessionPreviewPageSize } from "../src/sessionPreview.ts";

test("large sessions request fewer turns on every preview page", () => {
  assert.equal(sessionPreviewPageSize(), 20);
  assert.equal(sessionPreviewPageSize(32 * 1024 * 1024 - 1), 20);
  assert.equal(sessionPreviewPageSize(32 * 1024 * 1024), 5);
  assert.equal(sessionPreviewPageSize(708_996_485), 5);
});

test("long message previews stay bounded while expansion preserves the original text", () => {
  const text = "内容\n".repeat(20_000);
  assert.equal(sessionMessagePreview(text).length, SESSION_MESSAGE_PREVIEW_CHARS);
  assert.equal(sessionMessagePreview(text, 2 * SESSION_MESSAGE_PREVIEW_CHARS), text.slice(0, 24_000));
  assert.equal(sessionMessagePreview(text, text.length), text);
  assert.equal(sessionMessagePreview("short"), "short");
  assert.equal(sessionMessagePreview(""), "");
});

test("preview boundaries preserve supplementary Unicode characters", () => {
  const text = "a".repeat(SESSION_MESSAGE_PREVIEW_CHARS - 1) + "𠮷余下内容";
  assert.equal(sessionMessagePreview(text), "a".repeat(SESSION_MESSAGE_PREVIEW_CHARS - 1));
  assert.equal(sessionMessagePreview(text, SESSION_MESSAGE_PREVIEW_CHARS + 1), "a".repeat(SESSION_MESSAGE_PREVIEW_CHARS - 1) + "𠮷");
});
