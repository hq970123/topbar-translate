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
  if (!S.providerId || !S.state.providers.some((p) => p.id === S.providerId && p.enabled)) {
    S.providerId = S.state.settings.default_provider;
  }
  $("pinBtn").classList.toggle("active", S.state.pinned);
  $("hint").textContent = `${isMac() ? "⌘" : "Ctrl"} + Enter 翻译 · Esc 收起 · ${prettyShortcut(S.state.settings.shortcut, S.state.platform)} 唤起`;
  $("welcomeKey").textContent = prettyShortcut(S.state.settings.shortcut, S.state.platform);
  renderLangs();
  renderProviders();
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
    item("auto", "自动检测", S.source === "auto") + `<div class="sep"></div>` + st.languages.map((l) => item(l.code, l.label, S.source === l.code)).join("");
  $("tgtMenu").innerHTML =
    item("auto", "智能选择", S.target === "auto", "中文→英语，其他→中文") + `<div class="sep"></div>` + st.languages.map((l) => item(l.code, l.label, S.target === l.code)).join("");
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
        return `<div class="item ${x.id === S.providerId ? "sel" : ""}" data-id="${x.id}">${esc(x.name)}<span class="sub">${esc(warn)}${x.id === S.state.settings.default_provider ? " · 默认" : ""}</span></div>`;
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
  try {
    const r = await invoke("translate", { req: { text, image: S.image, source: S.source, target: S.target, provider_id: S.providerId } });
    if (id !== S.reqId) return; // a newer request superseded this one
    S.last = r;
    res.textContent = r.text;
    const auto = S.source === "auto" ? "检测到 " : "";
    $("meta").textContent = `${auto}${langLabel(S.state, r.source)} → ${langLabel(S.state, r.target)} · ${r.provider_name} · ${(r.ms / 1000).toFixed(1)}s`;
    renderLangs();
  } catch (err) {
    if (id !== S.reqId) return;
    showError(err);
  }
}

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
});
listen("panel:pinned", (e) => $("pinBtn").classList.toggle("active", !!e.payload));
function maybeWelcome() {
  if (localStorage.getItem("welcomed")) return;
  localStorage.setItem("welcomed", "1");
  $("welcome").classList.remove("hidden");
  setTimeout(() => $("welcome").classList.add("hidden"), 12000);
}
listen("config:changed", async () => { await loadState(); });

loadState().then(() => {
  updateCounter();
  reportSize();
  // The window may have been shown before this script started listening.
  if (S.state.visible) openPanel();
});
