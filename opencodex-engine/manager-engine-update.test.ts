import { describe, expect, test } from "bun:test";
import { normalizeReleaseVersion, parseReleases } from "./manager-engine-update";

describe("OpenCodex release catalog", () => {
  test("accepts published semantic versions and excludes drafts or untrusted URLs", () => {
    expect(parseReleases([
      {
        tag_name: "v2.28.0",
        name: "v2.28.0",
        draft: false,
        prerelease: false,
        published_at: "2026-08-21T00:00:00Z",
        html_url: "https://github.com/lidge-jun/opencodex/releases/tag/v2.28.0",
      },
      {
        tag_name: "v2.29.0",
        draft: true,
        prerelease: false,
        html_url: "https://github.com/lidge-jun/opencodex/releases/tag/v2.29.0",
      },
      {
        tag_name: "v2.30.0",
        draft: false,
        prerelease: false,
        html_url: "https://example.com/v2.30.0",
      },
    ])).toEqual([{
      version: "2.28.0",
      tag: "v2.28.0",
      name: "v2.28.0",
      prerelease: false,
      publishedAt: "2026-08-21T00:00:00Z",
      url: "https://github.com/lidge-jun/opencodex/releases/tag/v2.28.0",
    }]);
  });

  test("rejects paths, latest aliases and malformed versions", () => {
    expect(normalizeReleaseVersion("v2.28.0-preview.20260821")).toBe("2.28.0-preview.20260821");
    expect(normalizeReleaseVersion("latest")).toBeNull();
    expect(normalizeReleaseVersion("v2.28.0/../../bad")).toBeNull();
    expect(normalizeReleaseVersion("v2.28")).toBeNull();
  });
});
