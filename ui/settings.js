/* Settings window. */
let ST = null;
const MODELS = {
  openai: ["gpt-4o-mini", "gpt-4o", "gpt-4.1-mini", "gpt-4.1"],
  deepseek: ["deepseek-chat"],
  gemini: ["gemini-2.5-flash", "gemini-2.5-pro", "gemini-2.0-flash"],
  claude: ["claude-haiku-4-5", "claude-sonnet-4-5"],
  ollama: ["qwen2.5:7b", "qwen3:8b", "llama3.1:8b"],
  custom: [],
};
const KIND_ORDER = ["openai", "deepseek", "gemini", "claude", "deepl", "microsoft", "ollama", "custom"];

/* ---------- tabs ---------- */
function showTab(name) {
  document.querySelectorAll(".tab").forEach((t) => t.classList.toggle("active", t.dataset.tab === name));
  document.querySelectorAll(".page").forEach((p) => p.classList.toggle("hidden", p.id !== `page-${name}`));
}
document.querySelectorAll(".tab").forEach((t) => (t.onclick = () => showTab(t.dataset.tab)));
listen("settings:tab", (e) => showTab(e.payload || "general"));
if (location.hash) showTab(location.hash.slice(1));

/* ---------- load / render ---------- */
async function load() {
  ST = await invoke("get_state");
  applyTheme(ST.settings.theme);
  setUiLang(ST.settings.ui_language);
  $("ver").textContent = `v${ST.version}`;
  $("ver2").textContent = `v${ST.version}`;
  renderGeneral();
  renderApis();
}

function renderGeneral() {
  const s = ST.settings;
  $("uiLangSel").innerHTML = `<option value="auto">${esc(t("跟随系统"))}</option>` +
    UI_LANGS.map(([c, n]) => `<option value="${c}">${esc(n)}</option>`).join("");
  document.querySelectorAll(".langsel").forEach((sel) => {
    sel.innerHTML = ST.languages.map((l) => `<option value="${l.code}">${esc(langName(l))}</option>`).join("");
  });
  document.querySelectorAll("[data-k]").forEach((el) => {
    const v = s[el.dataset.k];
    if (el.type === "checkbox") el.checked = !!v;
    else el.value = v;
  });
  $("shortcutBtn").textContent = prettyShortcut(s.shortcut, ST.platform);
}

async function saveSettings(patch) {
  const next = { ...ST.settings, ...patch };
  try {
    ST = await invoke("save_settings", { settings: next });
    applyTheme(ST.settings.theme);
    if (uiLang !== resolveUiLang(ST.settings.ui_language)) setUiLang(ST.settings.ui_language);
    renderGeneral();
    toast("已保存");
  } catch (e) {
    toast(String(e));
    renderGeneral();
  }
}

document.querySelectorAll("[data-k]").forEach((el) => {
  el.addEventListener("change", () => {
    const k = el.dataset.k;
    let v;
    if (el.type === "checkbox") v = el.checked;
    else if (el.type === "number") {
      v = Number(el.value);
      if (!Number.isFinite(v)) return renderGeneral();
    } else v = el.value;
    saveSettings({ [k]: v });
  });
});

/* ---------- shortcut recorder ---------- */
let recording = false;
$("shortcutBtn").onclick = () => {
  recording = true;
  $("shortcutBtn").classList.add("rec");
  $("shortcutBtn").textContent = "请按下组合键…";
};
function keyName(e) {
  const c = e.code;
  if (/^Key[A-Z]$/.test(c)) return c.slice(3);
  if (/^Digit\d$/.test(c)) return c.slice(5);
  if (/^F\d{1,2}$/.test(c)) return c;
  const map = { Space: "Space", Enter: "Enter", Tab: "Tab", Backquote: "`", Minus: "-", Equal: "=", BracketLeft: "[", BracketRight: "]", Backslash: "\\", Semicolon: ";", Quote: "'", Comma: ",", Period: ".", Slash: "/", ArrowUp: "Up", ArrowDown: "Down", ArrowLeft: "Left", ArrowRight: "Right" };
  return map[c] || null;
}
document.addEventListener("keydown", (e) => {
  if (!recording) return;
  e.preventDefault();
  e.stopPropagation();
  if (e.key === "Escape") { recording = false; $("shortcutBtn").classList.remove("rec"); renderGeneral(); return; }
  if (e.key === "Backspace" || e.key === "Delete") {
    recording = false; $("shortcutBtn").classList.remove("rec"); saveSettings({ shortcut: "" }); return;
  }
  const k = keyName(e);
  if (!k) return; // waiting for a non-modifier key
  const mods = [];
  const mac = ST.platform === "macos";
  if (mac) {
    if (e.metaKey) mods.push("CommandOrControl");
    if (e.ctrlKey) mods.push("Control");
  } else {
    if (e.ctrlKey) mods.push("CommandOrControl");
    if (e.metaKey) mods.push("Super");
  }
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (!mods.length && !/^F\d+$/.test(k)) { toast("请至少包含一个修饰键（Ctrl/⌘/Alt/Shift）"); return; }
  recording = false;
  $("shortcutBtn").classList.remove("rec");
  saveSettings({ shortcut: [...mods, k].join("+") });
}, true);

/* ---------- API list ---------- */
function statusOf(p) {
  if (!p.enabled) return { cls: "", text: "已禁用" };
  if (p.key_required && !p.has_key) return { cls: "warn", text: "未配置 API Key" };
  if (p.kind === "index") return { cls: "on", text: "官方免费翻译 · 无需 API Key" };
  return { cls: "on", text: "已配置" };
}

function renderApis() {
  const def = ST.settings.default_provider;
  const rows = ST.providers.map((p) => {
    const st = statusOf(p);
    const detail = p.kind === "index" ? st.text : `${t(KIND_LABEL[p.kind] || p.kind)}${p.model ? " · " + p.model : ""} · ${st.text}`;
    const isDef = p.id === def;
    return `<div class="prov-row" data-id="${p.id}">
      <span class="status ${st.cls}"></span>
      <div class="prov-main"><b>${esc(p.name)}</b>${isDef ? '<span class="badge">✓ 默认使用</span>' : ""}<small>${esc(detail)}</small></div>
      <div class="prov-actions">
        ${!isDef && p.enabled ? '<button class="link" data-act="default">设为默认</button>' : ""}
        <button class="link" data-act="test">测试</button>
        ${p.builtin ? "" : '<button class="link" data-act="edit">编辑</button><button class="link danger" data-act="delete">删除</button>'}
        <label class="switch" title="启用 / 禁用"><input type="checkbox" data-act="toggle" ${p.enabled ? "checked" : ""}/><span></span></label>
      </div>
    </div>`;
  });
  // Quick-add rows for providers that are not configured yet.
  const have = new Set(ST.providers.map((p) => p.kind));
  KIND_ORDER.filter((k) => !have.has(k) && k !== "custom").forEach((k) => {
    rows.push(`<div class="prov-row"><span class="status"></span><div class="prov-main"><b>${esc(KIND_LABEL[k])}</b><small>未配置</small></div><div class="prov-actions"><button class="link" data-add="${k}">添加</button></div></div>`);
  });
  $("provList").innerHTML = rows.join("");
  $("defaultSel").innerHTML = ST.providers
    .filter((p) => p.enabled)
    .map((p) => `<option value="${p.id}" ${p.id === def ? "selected" : ""}>${esc(p.name)}</option>`)
    .join("");
}

$("defaultSel").onchange = async (e) => {
  try { ST = await invoke("set_default_provider", { id: e.target.value }); renderApis(); toast("已设为默认"); }
  catch (err) { toast(String(err)); renderApis(); }
};

$("provList").addEventListener("click", async (e) => {
  const add = e.target.closest("[data-add]");
  if (add) return openEditor(null, add.dataset.add);
  const btn = e.target.closest("[data-act]");
  if (!btn || btn.tagName === "INPUT") return;
  const id = btn.closest(".prov-row").dataset.id;
  const p = ST.providers.find((x) => x.id === id);
  const act = btn.dataset.act;
  try {
    if (act === "default") { ST = await invoke("set_default_provider", { id }); renderApis(); toast("已设为默认"); }
    if (act === "edit") openEditor(p);
    if (act === "delete") {
      if (!confirm(t("确定删除「{0}」？保存在系统安全存储中的 API Key 也会一并删除。", p.name))) return;
      ST = await invoke("delete_provider", { id }); renderApis(); toast("已删除");
    }
    if (act === "test") {
      btn.textContent = "测试中…";
      try { const out = await invoke("test_provider", { provider: p, apiKey: null }); toast(`连接成功：${out}`); }
      catch (err) { toast(`失败：${err.message || err}`); }
      btn.textContent = "测试";
    }
  } catch (err) { toast(String(err)); }
});
$("provList").addEventListener("change", async (e) => {
  const cb = e.target.closest('input[data-act="toggle"]');
  if (!cb) return;
  const id = cb.closest(".prov-row").dataset.id;
  const p = { ...ST.providers.find((x) => x.id === id), enabled: cb.checked };
  try { ST = await invoke("save_provider", { provider: p, apiKey: null, makeDefault: false }); renderApis(); }
  catch (err) { toast(String(err)); renderApis(); }
});

/* ---------- editor ---------- */
let editing = null;
$("mKind").innerHTML = KIND_ORDER.map((k) => `<option value="${k}">${esc(KIND_LABEL[k])}</option>`).join("");

async function applyKind(kind, fresh) {
  const [url, model] = await invoke("kind_defaults", { kind });
  if (fresh) {
    $("mUrl").value = url;
    $("mModel").value = model;
    const kindNames = Object.values(KIND_LABEL).flatMap((v) => [v, t(v)].map((x) => x.replace(/\s*[（(].*[）)]/, "")));
    if (!$("mName").value || kindNames.includes($("mName").value)) $("mName").value = t(KIND_LABEL[kind]).replace(/\s*[（(].*[）)]/, "");
  }
  $("mUrl").placeholder = url || "https://example.com/v1";
  $("modelList").innerHTML = (MODELS[kind] || []).map((m) => `<option value="${m}">`).join("");
  $("mModelRow").classList.toggle("hidden", kind === "deepl" || kind === "microsoft");
  $("mRegionRow").classList.toggle("hidden", kind !== "microsoft");
  const needKey = !["ollama", "custom"].includes(kind);
  $("mKey").placeholder = editing && editing.has_key ? "已保存在系统安全存储（留空则不修改）" : needKey ? "必填" : "可选";
}

function openEditor(p, kind) {
  editing = p;
  $("mTitle").textContent = p ? `编辑「${p.name}」` : "添加 API";
  $("mKind").value = p ? p.kind : kind || "openai";
  $("mKind").disabled = !!p;
  $("mName").value = p ? p.name : "";
  $("mUrl").value = p ? p.base_url : "";
  $("mKey").value = "";
  $("mModel").value = p ? p.model : "";
  $("mRegion").value = p ? p.region : "";
  $("mTimeout").value = p ? p.timeout_secs : 30;
  $("mDefault").checked = p ? p.id === ST.settings.default_provider : false;
  $("mTest").textContent = "";
  $("mTest").className = "test";
  applyKind($("mKind").value, !p);
  $("modal").classList.remove("hidden");
}
$("addBtn").onclick = () => openEditor(null, "openai");
$("mKind").onchange = () => applyKind($("mKind").value, true);
$("mCancel").onclick = () => $("modal").classList.add("hidden");

function formProvider() {
  return {
    id: editing ? editing.id : "",
    name: $("mName").value.trim(),
    kind: $("mKind").value,
    base_url: $("mUrl").value.trim(),
    model: $("mModel").value.trim(),
    region: $("mRegion").value.trim(),
    timeout_secs: Math.max(5, Math.min(300, Number($("mTimeout").value) || 30)),
    enabled: editing ? editing.enabled : true,
    builtin: false,
  };
}

$("mTestBtn").onclick = async () => {
  const t = $("mTest");
  t.className = "test"; t.textContent = "正在测试…";
  try {
    const out = await invoke("test_provider", { provider: formProvider(), apiKey: $("mKey").value || null });
    t.className = "test ok"; t.textContent = `✓ 连接成功：Hello, world! → ${out}`;
  } catch (err) {
    t.className = "test bad"; t.textContent = `✕ ${err.message || err}`;
  }
};
$("mSave").onclick = async () => {
  try {
    const key = $("mKey").value;
    ST = await invoke("save_provider", { provider: formProvider(), apiKey: key ? key : null, makeDefault: $("mDefault").checked });
    $("modal").classList.add("hidden");
    renderApis();
    toast("已保存");
  } catch (err) {
    $("mTest").className = "test bad";
    $("mTest").textContent = `✕ ${err.message || err}`;
  }
};

$("quitBtn").onclick = () => invoke("quit_app");
listen("config:changed", load);
load();
