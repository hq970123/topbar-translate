/* Local vocabulary book, kept in localStorage. */
const Vocab = (() => {
  const KEY = "tt_vocab_v1";
  function load() {
    try { const a = JSON.parse(localStorage.getItem(KEY) || "[]"); return Array.isArray(a) ? a : []; }
    catch { return []; }
  }
  function store(list) { try { localStorage.setItem(KEY, JSON.stringify(list)); } catch {} }
  const norm = (s) => (s || "").trim().toLowerCase();
  function has(src) { return load().some((e) => norm(e.src) === norm(src)); }
  function add(entry) {
    const list = load().filter((e) => norm(e.src) !== norm(entry.src));
    list.unshift({ id: Date.now().toString(36) + Math.random().toString(36).slice(2, 6), ts: Date.now(), ...entry });
    store(list);
  }
  function removeSrc(src) { store(load().filter((e) => norm(e.src) !== norm(src))); }
  function remove(id) { store(load().filter((e) => e.id !== id)); }
  function clear() { localStorage.removeItem(KEY); }
  function search(q) {
    q = norm(q);
    const list = load();
    return q ? list.filter((e) => norm(e.src).includes(q) || norm(e.dst).includes(q)) : list;
  }
  function csv() {
    const cell = (v) => `"${String(v ?? "").replace(/"/g, '""')}"`;
    const rows = [["word", "phonetic", "translation", "definition", "added"]];
    load().forEach((e) => rows.push([e.src, e.phonetic, e.dst, e.def, new Date(e.ts).toISOString().slice(0, 10)]));
    return rows.map((r) => r.map(cell).join(",")).join("\r\n");
  }
  return { has, add, remove, removeSrc, clear, search, csv, count: () => load().length };
})();
