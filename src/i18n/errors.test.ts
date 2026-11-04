import { describe, expect, it } from "vitest";
import { localizeError } from "./errors";
import { mainTranslations } from "./main";

describe("localizeError", () => {
  it("translates known Rust errors", () => {
    expect(localizeError("截图会话已失效，请重新截图", "en")).toBe(
      "The capture session has expired. Please take another screenshot.",
    );
    expect(localizeError("未检测到可截图的显示器", "ru")).toBe(
      "Не найдено ни одного дисплея для снимка.",
    );
  });

  it("preserves unknown technical details", () => {
    expect(localizeError("CGWindowListCreateImage failed", "en")).toBe(
      "CGWindowListCreateImage failed",
    );
  });
});

describe("mainTranslations", () => {
  it("localizes the capture-expired message", () => {
    expect(mainTranslations["zh-CN"].captureExpired).toContain("截图");
    expect(mainTranslations.en.captureExpired).toContain("capture");
    expect(mainTranslations.ru.captureExpired).toContain("снимок");
  });
});
