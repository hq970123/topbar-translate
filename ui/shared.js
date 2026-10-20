/* Shared helpers for the panel and settings windows. */
const TAURI = window.__TAURI__;
const invoke = (cmd, args) => TAURI.core.invoke(cmd, args);
const listen = (ev, cb) => TAURI.event.listen(ev, cb);
const $ = (id) => document.getElementById(id);

const KIND_LABEL = {
  index: "Index-Translate",
  openai: "OpenAI",
  deepseek: "DeepSeek",
  gemini: "Google Gemini",
  claude: "Claude",
  deepl: "DeepL",
  microsoft: "Microsoft Translator",
  ollama: "Ollama（本地）",
  custom: "自定义（OpenAI 兼容）",
};

function esc(s) {
  return String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}

let themeSetting = "system";
const darkMQ = window.matchMedia("(prefers-color-scheme: dark)");
function applyTheme(theme) {
  themeSetting = theme || "system";
  const dark = themeSetting === "dark" || (themeSetting === "system" && darkMQ.matches);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
}
darkMQ.addEventListener?.("change", () => applyTheme(themeSetting));

let toastTimer;
function toast(msg) {
  const t = $("toast");
  if (!t) return;
  t.textContent = msg;
  t.classList.add("show");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => t.classList.remove("show"), 1600);
}

function langLabel(state, code) {
  if (code === "auto") return "自动检测";
  const l = state.languages.find((x) => x.code === code);
  return l ? l.label : code;
}

function prettyShortcut(s, platform) {
  if (!s) return "未设置";
  const mac = platform === "macos";
  return s
    .split("+")
    .map((k) => {
      switch (k) {
        case "CommandOrControl": case "CmdOrCtrl": return mac ? "⌘" : "Ctrl";
        case "Command": case "Cmd": case "Super": case "Meta": return mac ? "⌘" : "Win";
        case "Control": case "Ctrl": return mac ? "⌃" : "Ctrl";
        case "Alt": case "Option": return mac ? "⌥" : "Alt";
        case "Shift": return mac ? "⇧" : "Shift";
        default: return k;
      }
    })
    .join(mac ? "" : " + ");
}

function providerTag(p) {
  if (p.kind === "index") return "免费";
  if (p.kind === "ollama") return "本地";
  return KIND_LABEL[p.kind] || p.kind;
}
