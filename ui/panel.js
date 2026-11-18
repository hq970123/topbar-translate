/* Top panel logic. */
const S = { state: null, source: "auto", target: "auto", providerId: null, image: null, reqId: 0, busy: false, last: null, autoTimer: null, detectTimer: null, composing: false };
const panel = $("panel");
const input = $("input");

const isMac = () => S.state && S.state.platform === "macos";

/* ---------- size sync ---------- */
let lastH = 0;
function reportSize() {
  let h = panel.getBoundingClientRect().height;
  // Make room for an open dropdown that extends below the card.
  document.querySelectorAll(".dd.open .menu").forEach((m) => {
    const r = m.getBoundingClientRect();
    const bottom = r.bottom - panel.getBoundingClientRect().top;
    if (bottom > h) h = bottom + 8;
  });
  h = Math.ceil(h);
  if (Math.abs(h - lastH) >= 1) {
    lastH = h;
    invoke("resize_panel", { height: h });
  }
}
new ResizeObserver(reportSize).observe(panel);

/* ---------- state ---------- */
async function loadState() {
  S.state = await invoke("get_state");
  applyTheme(S.state.settings.theme);
  setUiLang(S.state.settings.ui_language);
  if (!S.providerId || !S.state.providers.some((p) => p.id === S.providerId && p.enabled)) {
    S.providerId = S.state.settings.default_provider;
  }
  $("pinBtn").classList.toggle("active", S.state.pinned);
  $("hint").textContent = `${isMac() ? "⌘" : "Ctrl"} + Enter 翻译 · Esc 收起 · ${prettyShortcut(S.state.settings.shortcut, S.state.platform)} 唤起`;
  $("welcomeKey").textContent = prettyShortcut(S.state.settings.shortcut, S.state.platform);
  renderLangs();
  renderProviders();
  if ((S.state.shortcut_conflicts || []).length && !S.warnedSc) {
    S.warnedSc = true;
    setTimeout(() => toast(t("部分快捷键已被其他应用占用，可在设置中更换")), 600);
  }
}

function currentProvider() {
  return S.state.providers.find((p) => p.id === S.providerId) || S.state.providers[0];
}

/* ---------- dropdowns ---------- */
function closeMenus(except) {
  document.querySelectorAll(".dd.open").forEach((d) => d !== except && d.classList.remove("open"));
  syncHold();
}
function syncHold() {
  const hold = !!document.querySelector(".dd.open") || S.composing;
  invoke("ui_activity", { typing: false, hold });
  requestAnimationFrame(reportSize);
}
function toggleMenu(dd) {
  const open = !dd.classList.contains("open");
  closeMenus(dd);
  dd.classList.toggle("open", open);
  syncHold();
}
document.addEventListener("mousedown", (e) => {
  if (!e.target.closest(".dd")) closeMenus();
});

function renderLangs() {
  const st = S.state;
  const effTarget = S.target === "auto" ? (S.last ? S.last.target : st.settings.target_lang) : S.target;
  $("srcLabel").textContent = S.source === "auto" ? (S.detected ? `自动检测（${langLabel(st, S.detected)}）` : "自动检测") : langLabel(st, S.source);
  $("tgtLabel").textContent = S.target === "auto" ? langLabel(st, effTarget) : langLabel(st, S.target);
  const item = (code, label, sel, sub = "") => `<div class="item ${sel ? "sel" : ""}" data-code="${code}">${esc(label)}${sub ? `<span class="sub">${esc(sub)}</span>` : ""}</div>`;
  $("srcMenu").innerHTML =
    item("auto", "自动检测", S.source === "auto") + `<div class="sep"></div>` + st.languages.map((l) => item(l.code, langName(l), S.source === l.code)).join("");
  $("tgtMenu").innerHTML =
    item("auto", "智能选择", S.target === "auto", "中文→英语，其他→中文") + `<div class="sep"></div>` + st.languages.map((l) => item(l.code, langName(l), S.target === l.code)).join("");
}

$("srcBtn").onclick = () => toggleMenu($("srcDD"));
$("tgtBtn").onclick = () => toggleMenu($("tgtDD"));
$("provBtn").onclick = () => toggleMenu($("provDD"));
$("srcMenu").onclick = (e) => {
  const it = e.target.closest(".item"); if (!it) return;
  S.source = it.dataset.code; closeMenus(); renderLangs(); retranslateIfNeeded();
};
$("tgtMenu").onclick = (e) => {
  const it = e.target.closest(".item"); if (!it) return;
  S.target = it.dataset.code; closeMenus(); renderLangs(); retranslateIfNeeded();
};
$("swapBtn").onclick = () => {
  const st = S.state;
  const src = S.source === "auto" ? (S.last?.source || S.detected || null) : S.source;
  const tgt = S.target === "auto" ? (S.last?.target || st.settings.target_lang) : S.target;
  if (!src || src === "auto") { toast("请先输入内容或手动选择源语言"); return; }
  S.source = tgt; S.target = src;
  // Put the translation into the input, like most translators do.
  if (S.last && S.last.text) { input.value = S.last.text; onInputChanged(false); }
  renderLangs();
  retranslateIfNeeded(true);
};

function renderProviders() {
  const p = currentProvider();
  $("provLabel").textContent = p ? `${p.name} · ${providerTag(p)}` : "未配置翻译源";
  const list = S.state.providers.filter((x) => x.enabled);
  $("provMenu").innerHTML =
    list
      .map((x) => {
        const warn = x.key_required && !x.has_key ? "未配置 Key" : providerTag(x);
        return `<div class="item ${x.id === S.providerId ? "sel" : ""}" data-id="${x.id}">${esc(x.name)}<span class="sub">${esc(warn)}${x.id === S.state.settings.default_provider ? " · " + t("默认") : ""}</span></div>`;
      })
      .join("") + `<div class="sep"></div><div class="item" data-id="__manage">API 管理…</div>`;
}
$("provMenu").onclick = (e) => {
  const it = e.target.closest(".item"); if (!it) return;
  closeMenus();
  if (it.dataset.id === "__manage") { invoke("open_settings", { tab: "api" }); return; }
  S.providerId = it.dataset.id;
  renderProviders();
  retranslateIfNeeded(true); // keep the source text, translate with the new provider
};

/* ---------- input ---------- */
function updateCounter() {
  const n = [...input.value].length;
  $("counter").textContent = `${n}/5000`;
  $("goBtn").disabled = !(input.value.trim() || S.image);
}
function onInputChanged(typing = true) {
  updateCounter();
  if (typing) invoke("ui_activity", { typing: true, hold: !!document.querySelector(".dd.open") });
  clearTimeout(S.detectTimer);
  S.detectTimer = setTimeout(async () => {
    const t = input.value.trim();
    S.detected = t ? await invoke("detect_language", { text: t }) : null;
    renderLangs();
  }, 250);
  clearTimeout(S.autoTimer);
  if (S.state?.settings.auto_translate && (input.value.trim() || S.image)) {
    S.autoTimer = setTimeout(() => doTranslate(), S.state.settings.auto_translate_delay_ms || 800);
  }
  // Newer input makes any in-flight result stale.
  if (typing) S.reqId++;
}
input.addEventListener("input", () => onInputChanged(true));
input.addEventListener("compositionstart", () => { S.composing = true; syncHold(); });
input.addEventListener("compositionend", () => { S.composing = false; syncHold(); });
input.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); doTranslate(); }
});
input.addEventListener("paste", (e) => {
  const items = e.clipboardData?.items || [];
  for (const it of items) {
    if (it.type.startsWith("image/")) {
      e.preventDefault();
      const f = it.getAsFile();
      const rd = new FileReader();
      rd.onload = () => setImage(rd.result);
      rd.readAsDataURL(f);
      return;
    }
  }
});
panel.addEventListener("dragover", (e) => e.preventDefault());
panel.addEventListener("drop", (e) => {
  const f = [...(e.dataTransfer?.files || [])].find((x) => x.type.startsWith("image/"));
  if (!f) return;
  e.preventDefault();
  const rd = new FileReader();
  rd.onload = () => setImage(rd.result);
  rd.readAsDataURL(f);
});
function setImage(dataUrl) {
  S.image = dataUrl;
  $("imgThumb").src = dataUrl || "";
  $("imgChip").classList.toggle("hidden", !dataUrl);
  updateCounter();
  if (dataUrl) doTranslate();
}
$("imgClear").onclick = () => setImage(null);

$("pasteBtn").onclick = async () => {
  try {
    const t = await invoke("paste_text");
    if (!t) { toast("剪贴板中没有文本"); return; }
    const s = input.selectionStart ?? input.value.length;
    const e = input.selectionEnd ?? input.value.length;
    input.value = (input.value.slice(0, s) + t + input.value.slice(e)).slice(0, 5000);
    input.focus();
    onInputChanged(true);
    if (!S.state.settings.auto_translate) doTranslate();
  } catch (err) { toast(String(err)); }
};

/* ---------- translate ---------- */
function retranslateIfNeeded(force) {
  if ((input.value.trim() || S.image) && (force || S.last)) doTranslate();
}

async function doTranslate() {
  clearTimeout(S.autoTimer);
  const text = input.value;
  if (!text.trim() && !S.image) return;
  const id = ++S.reqId;
  const res = $("result");
  res.innerHTML = `<span class="loading">翻译中</span>`;
  $("meta").textContent = "";
  clearCompare();
  clearDict();
  const dictP = !S.image && isWord(text) && S.state.settings.word_lookup ? invoke("lookup_word", { word: text.trim() }).catch(() => null) : null;
  try {
    const r = await invoke("translate", { req: { text, image: S.image, source: S.source, target: S.target, provider_id: S.providerId } });
    if (id !== S.reqId) return; // a newer request superseded this one
    S.last = r;
    res.textContent = r.text;
    let srcText = text, fromImage = !!S.image && !text.trim(), image = S.image;
    if (r.ocr_text) {
      // Text was recognized on-device: put it in the input so it can be edited, and drop the image.
      srcText = text.trim() ? `${text.trimEnd()}\n${r.ocr_text}` : r.ocr_text;
      input.value = srcText;
      S.image = image = null;
      $("imgThumb").src = "";
      $("imgChip").classList.add("hidden");
      updateCounter();
    }
    History.add({ src: srcText.trim().slice(0, 5000), dst: r.text, sl: r.source, tl: r.target, prov: r.provider_name, img: fromImage }, S.state.settings);
    const auto = S.source === "auto" ? t("检测到") + " " : "";
    const fb = r.fallback_from ? t("{0} 失败，已改用 {1}", r.fallback_from, r.provider_name) + " · " : "";
    $("meta").textContent = `${fb}${auto}${langLabel(S.state, r.source)} → ${langLabel(S.state, r.target)} · ${r.provider_name} · ${(r.ms / 1000).toFixed(1)}s`;
    renderLangs();
    if (r.ocr_text) $("meta").textContent = t("本地 OCR") + " · " + $("meta").textContent;
    runCompare(id, srcText, image, r);
    updateStar(srcText, r);
    if (dictP) dictP.then((d) => { if (id === S.reqId && d) renderDict(d); });
  } catch (err) {
    if (id !== S.reqId) return;
    showError(err);
  }
}

/* ---------- word lookup + vocabulary ---------- */
const isWord = (s) => /^[A-Za-z][A-Za-z'-]{0,39}$/.test((s || "").trim());
const canStar = (s) => { s = (s || "").trim(); return !!s && s.length <= 60 && !/\n/.test(s); };
const SPK_SVG = `<svg class="lucide" viewBox="0 0 24 24"><path d="M11 4.702a.705.705 0 0 0-1.203-.498L6.413 7.587A1.4 1.4 0 0 1 5.416 8H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2.416a1.4 1.4 0 0 1 .997.413l3.383 3.384A.705.705 0 0 0 11 19.298z" /> <path d="M16 9a5 5 0 0 1 0 6" /></svg>`;
function clearDict() {
  S.dict = null;
  $("dict").innerHTML = "";
  $("dict").classList.add("hidden");
  $("starBtn").classList.add("hidden");
}
function renderDict(d) {
  S.dict = d;
  const head = `<div class="dict-head"><b>${esc(d.word)}</b>${d.phonetic ? `<span class="ph">${esc(d.phonetic)}</span>` : ""}${d.audio ? `<button class="icon-btn" id="dictAudio" title="${esc(t("发音"))}">${SPK_SVG}</button>` : ""}</div>`;
  const body = d.meanings.map((m) => `<div class="dict-m"><i>${esc(m.pos)}</i> ${m.defs.map(esc).join("<br>")}</div>`).join("");
  $("dict").innerHTML = head + body;
  $("dict").classList.remove("hidden");
  const a = $("dictAudio");
  if (a) a.onclick = () => { try { new Audio(d.audio).play(); } catch {} };
  reportSize();
}
function updateStar(text, r) {
  const b = $("starBtn");
  const ok = canStar(text) && !S.image;
  b.classList.toggle("hidden", !ok);
  b.classList.toggle("active", ok && Vocab.has(text));
  b.dataset.src = ok ? text.trim() : "";
}
$("starBtn").onclick = () => {
  const b = $("starBtn"); const src = b.dataset.src;
  if (!src || !S.last) return;
  if (Vocab.has(src)) { Vocab.removeSrc(src); b.classList.remove("active"); toast("已移出生词本"); return; }
  const d = S.dict && S.dict.word.toLowerCase() === src.toLowerCase() ? S.dict : null;
  Vocab.add({ src, dst: S.last.text, sl: S.last.source, tl: S.last.target, phonetic: d?.phonetic || "", audio: d?.audio || "", def: d ? d.meanings.map((m) => `${m.pos}. ${m.defs[0]}`).join(" / ") : "" });
  b.classList.add("active");
  toast("已加入生词本");
};
function renderVocab() {
  const list = Vocab.search($("vocabSearch").value);
  $("vocabList").innerHTML = !list.length
    ? `<div class="hist-empty">${esc(t(Vocab.count() ? "没有匹配的单词" : "生词本还是空的，翻译单词后点星标收藏"))}</div>`
    : list.map((e) => `<div class="hist-item" data-id="${e.id}" data-src="${esc(e.src)}">
      <div class="src">${esc(e.src)}${e.phonetic ? ` <span class="ph">${esc(e.phonetic)}</span>` : ""}</div>
      <div class="dst">${esc(e.dst)}</div>
      <button class="icon-btn del" data-del="${e.id}" title="${esc(t("删除"))}"><svg class="lucide" viewBox="0 0 24 24"><path d="M18 6 6 18" /> <path d="m6 6 12 12" /></svg></button>
    </div>`).join("");
}
function showVocab(on) {
  if (on) showHistory(false);
  $("vocabCard").classList.toggle("hidden", !on);
  document.querySelector(".input-card").classList.toggle("hidden", on);
  $("resultCard").classList.toggle("hidden", on);
  $("compare").classList.toggle("hist-off", on);
  $("vocabBtn").classList.toggle("active", on);
  if (on) { $("vocabSearch").value = ""; renderVocab(); setTimeout(() => $("vocabSearch").focus(), 20); }
  requestAnimationFrame(reportSize);
}
$("vocabBtn").onclick = () => showVocab($("vocabCard").classList.contains("hidden"));
$("vocabClose").onclick = () => showVocab(false);
$("vocabSearch").addEventListener("input", () => { invoke("ui_activity", { typing: true, hold: false }); renderVocab(); });
$("vocabSearch").addEventListener("keydown", (e) => { if (e.key === "Escape") { e.stopPropagation(); showVocab(false); } });
$("vocabList").addEventListener("click", (e) => {
  const del = e.target.closest("[data-del]");
  if (del) { e.stopPropagation(); Vocab.remove(del.dataset.del); renderVocab(); return; }
  const it = e.target.closest(".hist-item"); if (!it) return;
  showVocab(false);
  input.value = it.dataset.src;
  input.dispatchEvent(new Event("input"));
  doTranslate();
});
$("vocabExport").onclick = async () => {
  if (!Vocab.count()) { toast("生词本还是空的"); return; }
  try { const p = await invoke("export_vocab", { csv: Vocab.csv() }); toast(t("已导出到 {0}", p)); }
  catch (err) { toast(String(err)); }
};
armConfirm($("vocabClear"), () => { Vocab.clear(); renderVocab(); toast("已清空生词本"); }, () => Vocab.count() > 0);

/* ---------- multi-engine compare ---------- */
function clearCompare() {
  const box = $("compare");
  box.innerHTML = "";
  box.classList.add("hidden");
}
function runCompare(id, text, image, main) {
  const ids = (S.state.settings.compare_providers || []).filter((x) => x !== main.provider_id);
  const list = ids
    .map((x) => S.state.providers.find((p) => p.id === x))
    .filter((p) => p && p.enabled && (!p.key_required || p.has_key) && (!image || p.supports_image));
  const box = $("compare");
  if (!list.length) { clearCompare(); return; }
  box.innerHTML = list
    .map((p) => `<div class="cmp-row" data-id="${p.id}"><div class="cmp-head"><b>${esc(p.name)}</b><button class="icon-btn cmp-copy" title="复制"><svg class="lucide" viewBox="0 0 24 24"><rect width="14" height="14" x="8" y="8" rx="2" ry="2" /> <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" /></svg></button></div><div class="cmp-text"><span class="loading">翻译中</span></div></div>`)
    .join("");
  box.classList.remove("hidden");
  reportSize();
  list.forEach(async (p) => {
    const row = box.querySelector(`.cmp-row[data-id="${p.id}"] .cmp-text`);
    try {
      const r = await invoke("translate", { req: { text, image, source: main.source, target: main.target, provider_id: p.id, no_failover: true } });
      if (id !== S.reqId) return;
      row.textContent = r.text;
    } catch (err) {
      if (id !== S.reqId) return;
      row.innerHTML = `<span class="err">⚠ ${esc((err && err.message) || String(err))}</span>`;
    }
    reportSize();
  });
}
$("compare").addEventListener("click", async (e) => {
  const b = e.target.closest(".cmp-copy"); if (!b) return;
  const txt = b.closest(".cmp-row").querySelector(".cmp-text").textContent;
  if (!txt) return;
  await invoke("copy_text", { text: txt });
  toast("已复制");
});

function showError(err) {
  const e = typeof err === "string" ? { code: "error", message: err } : err || {};
  const res = $("result");
  const others = S.state.providers.filter((p) => p.enabled && p.id !== S.providerId && (!p.key_required || p.has_key));
  const btns = [`<button class="ghost" data-act="retry">重试</button>`];
  if (["auth", "quota", "model", "config"].includes(e.code)) btns.push(`<button class="ghost" data-act="settings">去设置</button>`);
  if (others.length) btns.push(`<button class="ghost" data-act="switch" data-id="${others[0].id}">改用 ${esc(others[0].name)}</button>`);
  res.innerHTML = `<div class="err">⚠ ${esc(e.message || "翻译失败")}</div><div class="err-actions">${btns.join("")}</div>`;
  if (e.detail) res.title = e.detail;
}
$("result").addEventListener("click", (ev) => {
  const b = ev.target.closest("button[data-act]"); if (!b) return;
  if (b.dataset.act === "retry") doTranslate();
  if (b.dataset.act === "settings") invoke("open_settings", { tab: "api" });
  if (b.dataset.act === "switch") { S.providerId = b.dataset.id; renderProviders(); doTranslate(); }
});

$("goBtn").onclick = () => doTranslate();
$("retryBtn").onclick = () => doTranslate();
$("copyBtn").onclick = async () => {
  if (!S.last?.text) { toast("还没有翻译结果"); return; }
  await invoke("copy_text", { text: S.last.text });
  toast("已复制");
};
$("expandBtn").onclick = () => {
  const r = $("result");
  r.classList.toggle("expanded");
  $("expandBtn").classList.toggle("active", r.classList.contains("expanded"));
};
$("speakBtn").onclick = () => {
  if (!S.last?.text || !window.speechSynthesis) { toast("没有可朗读的内容"); return; }
  speechSynthesis.cancel();
  const u = new SpeechSynthesisUtterance(S.last.text);
  const l = S.state.languages.find((x) => x.code === S.last.target);
  if (l) u.lang = l.speech;
  speechSynthesis.speak(u);
};

/* ---------- window controls ---------- */
$("pinBtn").onclick = async () => {
  const on = !$("pinBtn").classList.contains("active");
  await invoke("set_pinned", { pinned: on });
  $("pinBtn").classList.toggle("active", on);
  toast(on ? "已固定，不会自动收起" : "已取消固定");
};
$("gearBtn").onclick = () => invoke("open_settings", { tab: null });
$("closeBtn").onclick = () => invoke("hide_panel");
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    if (document.querySelector(".dd.open")) closeMenus();
    else invoke("hide_panel");
  }
});
/* ---------- show / hide ---------- */
function openPanel() {
  clearTimeout(S.clearTimer);
  maybeWelcome();
  panel.classList.remove("closing");
  requestAnimationFrame(() => panel.classList.add("open"));
  setTimeout(() => { input.focus(); input.select(); }, 30);
  reportSize();
}
listen("panel:show", openPanel);
listen("panel:hide", () => {
  closeMenus();
  panel.classList.add("closing");
  panel.classList.remove("open");
  speechSynthesis?.cancel();
  if (S.state?.settings.clear_on_hide !== false) {
    clearTimeout(S.clearTimer);
    S.clearTimer = setTimeout(clearContent, 260); // after the slide-up animation
  }
});
function clearContent() {
  if (panel.classList.contains("open")) return; // re-opened meanwhile
  clearTimeout(S.autoTimer);
  S.reqId++; // drop any in-flight result
  input.value = "";
  S.image = null;
  $("imgThumb").src = "";
  $("imgChip").classList.add("hidden");
  S.last = null;
  S.detected = null;
  $("result").innerHTML = `<span class="placeholder">${t("翻译结果将显示在这里...")}</span>`;
  $("result").removeAttribute("title");
  $("result").classList.remove("expanded");
  $("expandBtn").classList.remove("active");
  $("meta").textContent = "";
  clearCompare();
  clearDict();
  showHistory(false);
  showVocab(false);
  updateCounter();
  renderLangs();
}
listen("panel:pinned", (e) => $("pinBtn").classList.toggle("active", !!e.payload));
function maybeWelcome() {
  if (localStorage.getItem("welcomed")) return;
  localStorage.setItem("welcomed", "1");
  $("welcome").classList.remove("hidden");
  setTimeout(() => $("welcome").classList.add("hidden"), 12000);
}
listen("config:changed", async () => { await loadState(); refreshVoice(); });

/* ---------- screenshot translation ---------- */
$("shotBtn").onclick = () => invoke("start_snip");
listen("panel:image", (e) => { if (e.payload) setImage(e.payload); });
listen("panel:toast", (e) => toast(String(e.payload)));

/* ---------- selection translation (global shortcut, text grabbed by the backend) ---------- */
listen("panel:text", (e) => {
  const text = String(e.payload || "").slice(0, 5000);
  if (!text.trim()) return;
  showHistory(false);
  if (S.image) setImage(null);
  input.value = text;
  onInputChanged(false);
  S.reqId++;
  doTranslate();
});

/* ---------- voice input (shown only when a local model is downloaded) ---------- */
const mic = $("micBtn");
let recording = false;
async function refreshVoice() {
  try {
    const v = await invoke("voice_status");
    mic.classList.toggle("hidden", !v.ready);
  } catch (_) { mic.classList.add("hidden"); }
}
listen("voice:changed", refreshVoice);
listen("voice:level", (e) => { if (recording) mic.style.setProperty("--lvl", Math.min(1, e.payload * 3).toFixed(2)); });
mic.onclick = async () => {
  if (!recording) {
    try {
      await invoke("voice_start");
      recording = true;
      mic.classList.add("rec");
    } catch (err) { toast(String(err)); }
    return;
  }
  recording = false;
  mic.classList.remove("rec");
  mic.classList.add("busy");
  try {
    const text = await invoke("voice_stop", { source: S.source });
    if (text) {
      const pos = input.selectionStart ?? input.value.length;
      input.value = input.value.slice(0, pos) + text + input.value.slice(input.selectionEnd ?? pos);
      input.dispatchEvent(new Event("input"));
      doTranslate();
    }
  } catch (err) { toast(String(err)); }
  finally { mic.classList.remove("busy"); }
};
refreshVoice();

loadState().then(() => {
  updateCounter();
  reportSize();
  // The window may have been shown before this script started listening.
  if (S.state.visible) openPanel();
});

/* ---------- history ---------- */
function fmtWhen(ts) {
  const d = new Date(ts), now = new Date();
  const hm = d.toLocaleTimeString(uiLang || undefined, { hour: "2-digit", minute: "2-digit" });
  if (d.toDateString() === now.toDateString()) return hm;
  return d.toLocaleDateString(uiLang || undefined, { month: "short", day: "numeric" }) + " " + hm;
}
function renderHistory() {
  const list = History.search($("histSearch").value, S.state?.settings);
  const box = $("histList");
  if (!list.length) {
    const off = S.state?.settings.history_enabled === false;
    box.innerHTML = `<div class="hist-empty">${esc(t(off ? "历史记录已关闭，可在设置中开启" : ($("histSearch").value.trim() ? "没有匹配的记录" : "暂无历史记录")))}</div>`;
    return;
  }
  box.innerHTML = list.slice(0, 200).map((e) => `
    <div class="hist-item" data-id="${e.id}">
      <div class="src">${e.img ? "🖼 " : ""}${esc(e.src || t("图片翻译"))}</div>
      <div class="dst">${esc(e.dst)}</div>
      <div class="when">${esc(fmtWhen(e.ts))} · ${esc(langLabel(S.state, e.sl))} → ${esc(langLabel(S.state, e.tl))}${e.prov ? " · " + esc(e.prov) : ""}</div>
      <button class="icon-btn del" data-del="${e.id}" title="${esc(t("删除"))}"><svg class="lucide" viewBox="0 0 24 24"><path d="M18 6 6 18" /> <path d="m6 6 12 12" /></svg></button>
    </div>`).join("");
}
function showHistory(on) {
  if (on && !$("vocabCard").classList.contains("hidden")) showVocab(false);
  $("historyCard").classList.toggle("hidden", !on);
  document.querySelector(".input-card").classList.toggle("hidden", on);
  $("resultCard").classList.toggle("hidden", on);
  $("compare").classList.toggle("hist-off", on);
  $("histBtn").classList.toggle("active", on);
  if (on) { $("histSearch").value = ""; renderHistory(); setTimeout(() => $("histSearch").focus(), 20); }
  requestAnimationFrame(reportSize);
}
$("histBtn").onclick = () => showHistory($("historyCard").classList.contains("hidden"));
$("histClose").onclick = () => showHistory(false);
$("histSearch").addEventListener("input", () => { invoke("ui_activity", { typing: true, hold: false }); renderHistory(); });
$("histSearch").addEventListener("keydown", (e) => { if (e.key === "Escape") { e.stopPropagation(); showHistory(false); } });
armConfirm($("histClear"), () => { History.clear(); renderHistory(); toast("已清空历史记录"); }, () => History.count() > 0);
$("histList").addEventListener("click", (ev) => {
  const del = ev.target.closest("[data-del]");
  if (del) { ev.stopPropagation(); History.remove(del.dataset.del); renderHistory(); return; }
  const item = ev.target.closest(".hist-item"); if (!item) return;
  const e = History.all(S.state?.settings).find((x) => x.id === item.dataset.id); if (!e) return;
  S.reqId++;
  input.value = e.src || "";
  setImage(null);
  S.last = { text: e.dst, source: e.sl, target: e.tl, provider_name: e.prov };
  $("result").textContent = e.dst;
  $("meta").textContent = `${langLabel(S.state, e.sl)} → ${langLabel(S.state, e.tl)}${e.prov ? " · " + e.prov : ""} · ${fmtWhen(e.ts)}`;
  updateCounter();
  showHistory(false);
});
