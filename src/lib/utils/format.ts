// Sizes and speeds the way the Rust side writes them (src-tauri/src/i18n/mod.rs `size`): decimal units, one
// decimal below 100, a comma as the decimal separator except in English.

import type { Translator } from "$lib/i18n";

export function formatBytes(bytes: number, tr: Translator): string {
  const units: Array<[number, string]> = [
    [1e9, "unit.gb"],
    [1e6, "unit.mb"],
    [1e3, "unit.kb"],
  ];
  const [divisor, unit] = units.find(([d]) => bytes >= d) ?? [1, "unit.b"];
  const value = bytes / divisor;
  const text = value >= 100 || unit === "unit.b" ? value.toFixed(0) : value.toFixed(1);
  const number = tr.lang === "en" ? text : text.replace(".", ",");
  return `${number} ${tr.t(unit)}`;
}

export function formatSpeed(bytesPerSecond: number, tr: Translator): string {
  return tr.t("progress.speed", { speed: formatBytes(bytesPerSecond, tr) });
}

export function percent(done: number, total: number): number {
  if (!Number.isFinite(done) || !Number.isFinite(total) || total <= 0) return 0;
  return Math.min(100, Math.max(0, (done / total) * 100));
}
