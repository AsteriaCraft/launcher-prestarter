import { describe, expect, it } from "vitest";
import { catalogs, translator } from "./index";
import type { Stage } from "$lib/types/events";

const stages: Stage[] = ["preparing", "waitLock", "jreCheck", "jreDownload", "jreInstall", "jreVerify", "jarDownload", "launching", "done"];

describe("messages", () => {
  it("every language has exactly the English keys", () => {
    const english = Object.keys(catalogs.en).sort();
    for (const [lang, messages] of Object.entries(catalogs)) {
      expect(Object.keys(messages).sort(), lang).toEqual(english);
    }
  });

  it("every stage the Rust side sends has a text", () => {
    for (const lang of Object.keys(catalogs) as Array<keyof typeof catalogs>) {
      const tr = translator(lang);
      for (const stage of stages) {
        expect(tr.t(`stage.${stage}`), `${lang} ${stage}`).not.toBe(`stage.${stage}`);
      }
    }
  });

  it("fills placeholders and falls back to the key", () => {
    expect(translator("uk").t("app.version", { version: "0.3.0" })).toBe("версія 0.3.0");
    expect(translator("en").t("no.such.key")).toBe("no.such.key");
  });

  it("has eight tips in every language", () => {
    for (const lang of Object.keys(catalogs) as Array<keyof typeof catalogs>) {
      expect(translator(lang).tips(), lang).toHaveLength(8);
    }
  });
});
