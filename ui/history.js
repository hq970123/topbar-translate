/* Local translation history, kept in localStorage (shared by the panel and settings windows). */
const History = (() => {
  const KEY = "tt_history_v1";
  const DAY = 86400000;

  function load() {
    try { const a = JSON.parse(localStorage.getItem(KEY) || "[]"); return Array.isArray(a) ? a : []; }
    catch { return []; }
  }
  function store(list) {
    try { localStorage.setItem(KEY, JSON.stringify(list)); }
    catch { // quota exceeded: drop the oldest half and retry once
      try { localStorage.setItem(KEY, JSON.stringify(list.slice(0, Math.floor(list.length / 2)))); } catch {}
    }
  }
  /** Apply the retention settings (newest first). 0 means unlimited. */
  function prune(list, settings) {
    const days = Number(settings?.history_max_days) || 0;
    const max = Number(settings?.history_max_items) || 0;
    if (days > 0) {
      const cut = Date.now() - days * DAY;
      list = list.filter((e) => e.ts >= cut);
    }
    if (max > 0 && list.length > max) list = list.slice(0, max);
    return list;
  }
  function all(settings) {
    const raw = load();
    const list = prune(raw, settings);
    if (list.length !== raw.length) store(list);
    return list;
  }
  function add(entry, settings) {
    if (settings && settings.history_enabled === false) return;
    if (!entry.src && !entry.img) return;
    let list = load();
    // Same source + same direction: move it to the top instead of duplicating.
    list = list.filter((e) => !(e.src === entry.src && e.tl === entry.tl && !e.img && !entry.img));
    list.unshift({ id: Date.now().toString(36) + Math.random().toString(36).slice(2, 6), ts: Date.now(), ...entry });
    store(prune(list, settings));
  }
  function remove(id) { store(load().filter((e) => e.id !== id)); }
  function clear() { localStorage.removeItem(KEY); }
  function count() { return load().length; }
  function search(q, settings) {
    const list = all(settings);
    q = (q || "").trim().toLowerCase();
    if (!q) return list;
    return list.filter((e) => (e.src || "").toLowerCase().includes(q) || (e.dst || "").toLowerCase().includes(q));
  }
  return { all, add, remove, clear, count, search, prune };
})();
