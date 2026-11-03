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
  document.querySelectorAll("[data-sc]").forEach((b) => (b.textContent = prettyShortcut(s[b.dataset.sc], ST.platform)));
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
let recording = null; // the shortcut button being recorded
document.querySelectorAll("[data-sc]").forEach((b) => (b.onclick = () => {
  if (recording) recording.classList.remove("rec");
  recording = b;
  b.classList.add("rec");
  b.textContent = "请按下组合键…";
}));
function stopRec() { recording?.classList.remove("rec"); const k = recording?.dataset.sc; recording = null; return k; }
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
  if (e.key === "Escape") { stopRec(); renderGeneral(); return; }
  if (e.key === "Backspace" || e.key === "Delete") { saveSettings({ [stopRec()]: "" }); return; }
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
  const combo = [...mods, k].join("+");
  const key = stopRec();
  const other = key === "shortcut" ? "snip_shortcut" : "shortcut";
  if (ST.settings[other] === combo) { toast("两个快捷键不能相同"); renderGeneral(); return; }
  saveSettings({ [key]: combo });
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

/* ---------- voice models ---------- */
const VOICE_DESC = { tiny: "最快，准确度一般", base: "推荐，速度与准确度均衡", small: "更准确，需要更好的电脑" };
let voice = null, voiceProg = {};
async function renderVoice() {
  try { voice = await invoke("voice_status"); } catch (_) { return; }
  $("voiceModels").innerHTML = voice.models.map((m) => {
    const dl = voice.downloading === m.name;
    const p = voiceProg[m.name];
    const pct = p && p.total ? Math.floor((p.received / p.total) * 100) : 0;
    const sel = voice.selected === m.name;
    let act;
    if (dl) act = `<div class="vprog"><span style="width:${pct}%"></span></div><em>${pct}%</em><button class="link" data-vcancel>取消</button>`;
    else if (m.downloaded) act = `${sel ? `<span class="badge">✓ 使用中</span>` : `<button class="link" data-vuse="${m.name}">使用</button>`}<button class="link danger" data-vdel="${m.name}">删除</button>`;
    else act = `<button class="ghost" data-vdl="${m.name}" ${voice.downloading ? "disabled" : ""}>下载</button>`;
    return `<div class="row"><div><b>${m.name} · ${m.size_mb} MB</b><small>${VOICE_DESC[m.name] || ""}</small></div><div class="inline">${act}</div></div>`;
  }).join("");
}
$("voiceModels").addEventListener("click", async (e) => {
  const b = e.target.closest("button");
  if (!b) return;
  if (b.dataset.vdl) {
    const name = b.dataset.vdl;
    voiceProg[name] = { received: 0, total: 0 };
    invoke("voice_download", { name })
      .then(async () => { toast("下载完成"); if (!voice.models.some((m) => m.name === voice.selected && m.downloaded)) await saveSettings({ voice_model: name }); })
      .catch((err) => toast(String(err)))
      .finally(renderVoice);
    setTimeout(renderVoice, 100);
  } else if (b.dataset.vcancel !== undefined) {
    invoke("voice_cancel_download");
  } else if (b.dataset.vuse) {
    await saveSettings({ voice_model: b.dataset.vuse });
    renderVoice();
  } else if (b.dataset.vdel) {
    try { await invoke("voice_delete", { name: b.dataset.vdel }); toast("已删除"); } catch (err) { toast(String(err)); }
    renderVoice();
  }
});
listen("voice:progress", (e) => {
  voiceProg[e.payload.name] = e.payload;
  const bar = document.querySelector(".vprog span");
  if (!bar) return renderVoice();
  const pct = e.payload.total ? Math.floor((e.payload.received / e.payload.total) * 100) : 0;
  bar.style.width = pct + "%";
  bar.parentElement.nextElementSibling.textContent = pct + "%";
});
listen("voice:changed", renderVoice);
renderVoice();
