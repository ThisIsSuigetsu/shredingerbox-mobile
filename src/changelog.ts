// История версий вшивается в приложение при сборке: окно «Что нового» после обновления
// показывает раздел своей версии без сети.
import changelogRu from "../CHANGELOG.md?raw";
import changelogEn from "../CHANGELOG.en.md?raw";
import { lang } from "./i18n";

const esc = (s: string) =>
  s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);

/** Пункты в CHANGELOG перенесены по ширине - склеиваем строки, чтобы текст переносился сам. */
function unwrap(text: string): string[] {
  const out: string[] = [];
  for (const line of text.trim().split("\n")) {
    if (line.startsWith("  ") && out.length && out[out.length - 1]) {
      out[out.length - 1] += " " + line.trim();
    } else {
      out.push(line.trimEnd());
    }
  }
  return out;
}

function section(changelog: string, version: string): string {
  const lines = changelog.replace(/\r/g, "").split("\n");
  const start = lines.indexOf(`## ${version}`);
  if (start < 0) return "";
  const end = lines.findIndex((l, i) => i > start && l.startsWith("## "));
  return lines.slice(start + 1, end < 0 ? undefined : end).join("\n").trim();
}

/** Раздел «## 0.4.2» на языке интерфейса или пусто, если такой версии там нет. */
export function changesOf(version: string): string {
  return (lang() === "en" && section(changelogEn, version)) || section(changelogRu, version);
}

/** Markdown из CHANGELOG (абзацы, пункты «- », `код`) → HTML для окна. */
export function notesHtml(text: string): string {
  const inline = (s: string) => esc(s).replace(/`([^`]+)`/g, "<code>$1</code>").replace(/\*\*([^*]+)\*\*/g, "<b>$1</b>");
  let html = "";
  let list = false;
  for (const line of unwrap(text)) {
    if (line.startsWith("- ")) {
      if (!list) html += "<ul>";
      list = true;
      html += `<li>${inline(line.slice(2))}</li>`;
      continue;
    }
    if (list) html += "</ul>";
    list = false;
    if (line.trim()) html += `<p>${inline(line)}</p>`;
  }
  return `<div class="notes">${html}${list ? "</ul>" : ""}</div>`;
}
