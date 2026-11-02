// UI localization. Source strings are Simplified Chinese; see ui/i18n.tsv.
const UI_LANGS = [
  ["zh-CN", "简体中文"], ["zh-TW", "繁體中文"], ["en", "English"], ["fr", "Français"],
  ["es", "Español"], ["ru", "Русский"], ["ar", "العربية"], ["ja", "日本語"], ["ko", "한국어"],
];
let uiLang = "zh-CN";
const i18nPatterns = {};

function resolveUiLang(pref) {
  if (pref && pref !== "auto" && UI_LANGS.some(([c]) => c === pref)) return pref;
  for (const raw of navigator.languages?.length ? navigator.languages : [navigator.language || "en"]) {
    const l = raw.toLowerCase();
    if (l.startsWith("zh")) return /tw|hk|mo|hant/.test(l) ? "zh-TW" : "zh-CN";
    const base = l.split("-")[0];
    if (UI_LANGS.some(([c]) => c === base)) return base;
  }
  return "en";
}

function fmt(s, args) {
  return s.replace(/\{(\d+)\}/g, (m, i) => (args[i] !== undefined ? args[i] : m));
}

function patternsFor(lang) {
  if (i18nPatterns[lang]) return i18nPatterns[lang];
  const list = [];
  for (const [key, val] of Object.entries(I18N_DATA[lang] || {})) {
    if (!/\{\d+\}/.test(key)) continue;
    const idx = [];
    const re = key.split(/(\{\d+\})/).map((part) => {
      const m = part.match(/^\{(\d+)\}$/);
      if (m) { idx.push(+m[1]); return "([\\s\\S]*?)"; }
      return part.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    }).join("");
    list.push({ re: new RegExp(`^${re}$`), idx, val });
  }
  return (i18nPatterns[lang] = list);
}

/** Translate a Chinese source string; {0}.. are filled from args. Also matches
 *  already-formatted messages (e.g. errors from Rust) against templated keys. */
function t(s, ...args) {
  if (s == null) return s;
  s = String(s);
  const dict = I18N_DATA[uiLang];
  if (!dict) return fmt(s, args);
  if (dict[s] !== undefined) return fmt(dict[s], args);
  const trimmed = s.trim();
  if (trimmed !== s && dict[trimmed] !== undefined) return s.replace(trimmed, fmt(dict[trimmed], args));
  if (!args.length) {
    for (const p of patternsFor(uiLang)) {
      const m = s.match(p.re);
      if (m) {
        const vals = [];
        p.idx.forEach((n, i) => (vals[n] = t(m[i + 1])));
        return fmt(p.val, vals);
      }
    }
  }
  return fmt(s, args);
}

const I18N_ATTRS = ["title", "placeholder", "aria-label"];
const HAS_CJK = /[\u4e00-\u9fff]/;
let i18nBusy = false;

function i18nText(n, force) {
  if (n.nodeValue === n.__out && !force) return;
  if (n.__out === undefined || n.nodeValue !== n.__out) {
    if (!HAS_CJK.test(n.nodeValue)) return;
    n.__src = n.nodeValue;
  }
  const out = t(n.__src);
  n.__out = out;
  if (n.nodeValue !== out) n.nodeValue = out;
}

function i18nAttrs(el, force) {
  for (const a of I18N_ATTRS) {
    if (!el.hasAttribute(a)) continue;
    const cur = el.getAttribute(a), sk = `data-i18n-${a}`, ok = `__i18n_${a}`;
    if (cur !== el[ok]) {
      if (!HAS_CJK.test(cur)) continue;
      el.setAttribute(sk, cur);
    } else if (!force) continue;
    const out = t(el.getAttribute(sk));
    el[ok] = out;
    if (cur !== out) el.setAttribute(a, out);
  }
}

function i18nTree(node, force) {
  if (node.nodeType === 3) return i18nText(node, force);
  if (node.nodeType !== 1 || node.hasAttribute("data-no-i18n") || node.tagName === "SCRIPT" || node.tagName === "STYLE") return;
  i18nAttrs(node, force);
  for (const c of node.childNodes) i18nTree(c, force);
}

/** Translate the whole page; force re-translates nodes already handled (language switch). */
function applyI18n(force = false) {
  i18nBusy = true;
  i18nTree(document.body, force);
  const de = document.documentElement;
  if (!de.dataset.title) de.dataset.title = document.title;
  document.title = t(de.dataset.title);
  i18nBusy = false;
}

// Anything rendered later (innerHTML, textContent, toasts, errors) is translated automatically.
new MutationObserver((records) => {
  if (i18nBusy) return;
  i18nBusy = true;
  for (const r of records) {
    if (r.type === "childList") r.addedNodes.forEach((n) => i18nTree(n, false));
    else if (r.type === "characterData") i18nText(r.target, false);
    else if (r.type === "attributes") i18nAttrs(r.target, false);
  }
  i18nBusy = false;
}).observe(document.documentElement, { subtree: true, childList: true, characterData: true, attributes: true, attributeFilter: I18N_ATTRS });

/** Set UI language ("auto" follows the system), re-translate static DOM, push native strings. */
function setUiLang(pref) {
  uiLang = resolveUiLang(pref);
  document.documentElement.lang = uiLang;
  document.documentElement.dir = uiLang === "ar" ? "rtl" : "ltr";
  applyI18n(true);
  pushNativeStrings();
}

function pushNativeStrings() {
  try {
    invoke("set_ui_strings", {
      strings: ["显示翻译窗口", "设置…", "API 管理…", "退出 TopBar Translate", "TopBar Translate 设置"].map((s) => t(s)),
    });
  } catch (_) {}
}

uiLang = resolveUiLang("auto");
