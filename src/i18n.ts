// Язык интерфейса: русский или английский. Строки лежат прямо в коде парами
// L("по-русски", "in English") - так их проще держать рядом и не потерять перевод.

export type Lang = "ru" | "en";

let current: Lang = "ru";

/** Язык системы: русский для русско-, украино-, белорусско- и казахоязычных, иначе английский. */
export function systemLang(): Lang {
  const tags = navigator.languages?.length ? navigator.languages : [navigator.language];
  const first = (tags[0] ?? "").toLowerCase().split("-")[0];
  return ["ru", "uk", "be", "kk"].includes(first) ? "ru" : "en";
}

export function setLang(lang: Lang) {
  current = lang;
  document.documentElement.lang = lang;
}

export const lang = () => current;

/** Строка на текущем языке. */
export const L = (ru: string, en: string) => (current === "en" ? en : ru);

/** Формат чисел: «1,5» или «1.5». */
export const locale = () => (current === "en" ? "en-US" : "ru-RU");
