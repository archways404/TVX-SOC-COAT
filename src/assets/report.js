// The report page: copy route, issue/variable filters, SIP message viewer, log explorer.
(() => {
  const $ = (s) => document.querySelector(s);
  const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[c]));

  const copy = $('#copyroute');
  if (copy) copy.addEventListener('click', async () => {
    try { await navigator.clipboard.writeText(copy.dataset.text); copy.textContent = 'Copied ✓'; }
    catch (e) { copy.textContent = 'Copy failed'; }
    setTimeout(() => { copy.textContent = 'Copy route as text'; }, 1600);
  });

  const noise = $('#shownoise');
  if (noise) noise.addEventListener('change', () => $('table.issues').classList.toggle('shownoise', noise.checked));

  const vf = $('#varfilter');
  if (vf) vf.addEventListener('input', () => {
    const q = vf.value.toLowerCase();
    document.querySelectorAll('#vartable tr[data-step]').forEach((tr) => {
      tr.style.display = tr.textContent.toLowerCase().includes(q) ? '' : 'none';
    });
  });

  const sipData = $('#sipdata') ? JSON.parse($('#sipdata').textContent) : [];
  document.querySelectorAll('.ladder .msg').forEach((g) => g.addEventListener('click', () => {
    document.querySelectorAll('.ladder .msg.sel').forEach((x) => x.classList.remove('sel'));
    g.classList.add('sel');
    const m = sipData[+g.dataset.i];
    $('#sipmsg').textContent = `${m.t}  ${m.f} → ${m.to}  [${m.s}]  ${m.c || ''}\n\n${m.m || m.l}`;
  }));

  const logData = $('#logdata') ? JSON.parse($('#logdata').textContent) : [];
  const rowsEl = $('#logrows'), more = $('#logmore'), countEl = $('#logcount');
  const PAGE = 1500;
  let matches = [], shown = 0;
  function rowHtml(r) {
    const [t, sess, host, app, level, loc, msg, body, , thread] = r;
    return `<div class="lr${body ? ' hasbody' : ''}"><span>${esc(t)}</span><span class="lv lv-${esc(level)}">${esc(level)}</span>` +
      `<span class="ap" title="${esc(host)} ${esc(app)} [${esc(sess)}] ${esc(thread || '')}">${esc(app)}</span>` +
      `<span class="m">${esc(msg)} <span class="loc">${esc(loc)}</span></span>` +
      (body ? `<span class="body">${esc(body)}</span>` : '') + `</div>`;
  }
  function renderMore() {
    rowsEl.insertAdjacentHTML('beforeend', matches.slice(shown, shown + PAGE).map(rowHtml).join(''));
    shown = Math.min(shown + PAGE, matches.length);
    more.hidden = shown >= matches.length;
  }
  function applyFilter() {
    const text = $('#logfilter').value, step = $('#logstep').value, app = $('#logapp').value, level = $('#loglevel').value;
    let re = null;
    if (text) { try { re = new RegExp(text, 'i'); } catch (e) { re = null; } }
    const needle = text.toLowerCase();
    matches = logData.filter((r) => {
      if (step && String(r[8]) !== step) return false;
      if (app && r[3] !== app) return false;
      if (level === 'PROBLEM' ? !(r[4] === 'WARN' || r[4] === 'ERROR') : (level && r[4] !== level)) return false;
      if (!text) return true;
      const hay = r[6] + ' ' + r[5] + ' ' + r[7];
      return re ? re.test(hay) : hay.toLowerCase().includes(needle);
    });
    rowsEl.innerHTML = ''; shown = 0; countEl.textContent = matches.length; renderMore();
  }
  if (rowsEl) {
    ['#logfilter', '#logstep', '#logapp', '#loglevel'].forEach((s) => $(s).addEventListener('input', applyFilter));
    more.addEventListener('click', renderMore);
    rowsEl.addEventListener('click', (e) => { const lr = e.target.closest('.lr.hasbody'); if (lr) lr.classList.toggle('open'); });
    document.querySelectorAll('.loglink').forEach((a) => a.addEventListener('click', () => {
      $('#logstep').value = a.dataset.step; applyFilter();
    }));
    applyFilter();
  }
})();
