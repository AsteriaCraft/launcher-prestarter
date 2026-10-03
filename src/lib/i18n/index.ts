// Interface texts. The message files live with the Rust code (src-tauri/src/i18n/messages) and are the single
// source for both sides: Rust uses them for native dialogs, the window imports them here.

import be from "../../../src-tauri/src/i18n/messages/be.json";
import en from "../../../src-tauri/src/i18n/messages/en.json";
import pl from "../../../src-tauri/src/i18n/messages/pl.json";
import ru from "../../../src-tauri/src/i18n/messages/ru.json";
import uk from "../../../src-tauri/src/i18n/messages/uk.json";
import type { Lang } from "$lib/types/events";

type Messages = Record<string, string | string[]>;

export const catalogs: Record<Lang, Messages> = { be, en, pl, ru, uk };

export type Params = Record<string, string | number>;

export interface Translator {
  lang: Lang;
  t(key: string, params?: Params): string;
  tips(): string[];
}

export function translator(lang: Lang): Translator {
  const messages = catalogs[lang] ?? catalogs.en;
  return {
    lang,
    t(key, params = {}) {
      const raw = messages[key] ?? catalogs.en[key] ?? key;
      const template = Array.isArray(raw) ? raw.join(" ") : raw;
      return Object.entries(params).reduce((text, [name, value]) => text.replaceAll(`{${name}}`, String(value)), template);
    },
    tips() {
      const tips = messages.tips ?? catalogs.en.tips;
      return Array.isArray(tips) ? tips : [];
    },
  };
}
