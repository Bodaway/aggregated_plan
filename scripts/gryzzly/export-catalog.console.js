/* Gryzzly keyless catalog export — NO API KEY NEEDED.
 * Paste into the DevTools Console on https://app.gryzzly.io while logged in.
 * After pasting, click any project (or refresh the page) ONCE: it captures your
 * session's auth header IN-PAGE ONLY (never stored/sent anywhere), pulls every
 * project + task, and downloads gryzzly_catalog.json to your browser's downloads. */
(() => {
  let tok = null;
  const O = XMLHttpRequest.prototype.open, S = XMLHttpRequest.prototype.setRequestHeader;
  XMLHttpRequest.prototype.open = function (m, u) { this.__u = u; return O.apply(this, arguments); };
  XMLHttpRequest.prototype.setRequestHeader = function (k, v) {
    if (this.__u && ('' + this.__u).includes('api.gryzzly.io') && /^authorization$/i.test(k)) tok = v;
    return S.apply(this, arguments);
  };
  console.log('%cGryzzly export armed — now click any project (or refresh) once...', 'color:#16a34a;font-weight:bold');
  const iv = setInterval(async () => {
    if (!tok) return;
    clearInterval(iv);
    try {
      const H = { 'Content-Type': 'application/json', 'Authorization': tok };
      const post = (m, b) => fetch('https://api.gryzzly.io/' + m, { method: 'POST', headers: H, body: JSON.stringify(b || {}) }).then(r => r.json());
      const projects = (await post('view/projects.list', { filter: '', range: '', search: '', limit: 1000 })).payload || [];
      const pmap = {}; projects.forEach(p => pmap[p.id] = p);
      const rows = [];
      const flat = (arr, pid, depth = 0) => {
        if (depth > 50) return;
        (arr || []).forEach(t => {
          const p = pmap[t.project_id || pid] || {};
          rows.push([t.id, (t.name || '').trim(), t.project_id || pid, (p.name || '').trim(), (p.customer_name || '').trim(), (!t.completed_at && !t.deleted_at) ? 1 : 0]);
          flat(t.tasks, t.project_id || pid, depth + 1);
        });
      };
      for (const p of projects) { const r = await post('expandedProjectMetrics.get', { project_id: p.id }); flat((r.payload || {}).tasks, p.id); }
      const blob = new Blob([JSON.stringify(rows)], { type: 'application/json' });
      const a = document.createElement('a'); a.href = URL.createObjectURL(blob); a.download = 'gryzzly_catalog.json';
      document.body.appendChild(a); a.click(); setTimeout(() => { URL.revokeObjectURL(a.href); a.remove(); }, 1500);
      console.log('%cDownloaded gryzzly_catalog.json — ' + rows.length + ' tasks across ' + projects.length + ' projects.', 'color:#16a34a;font-weight:bold');
    } catch (e) {
      console.error('Gryzzly export failed:', e);
    }
  }, 700);
})();
