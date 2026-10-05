// Pages served by the running app: the update widget, recent traces, and Quit.
(() => {
  const $ = (s) => document.querySelector(s);
  const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  const icon = (name) => document.getElementById(`icon-${name}`)?.innerHTML || '';
  const box = $('#update'), foot = $('#foot-update'), mini = $('#update-mini');
  const root = document.body.dataset.root || '';
  let timer = null;

  async function post(path) {
    const response = await fetch(path, { method: 'POST' });
    return response.json().catch(() => ({}));
  }

  // ---- updates -------------------------------------------------------------------------------
  function card(title, text, action) {
    return `<div class="upd-card"><b>${title}</b>${text ? `<span class="muted">${text}</span>` : ''}${action || ''}</div>`;
  }
  function render(u) {
    if (!box) return;
    const v = esc(u.version), notes = u.notes_url ? `<a href="${esc(u.notes_url)}" target="_blank" rel="noopener">What's new</a>` : '';
    let html = '', footer = '', dot = false;
    switch (u.state) {
      case 'disabled':
        html = `<div class="upd">${icon('info')} Development build · updates off</div>`;
        footer = 'development build';
        break;
      case 'idle': case 'checking':
        html = `<div class="upd"><span class="spin"></span> Checking for updates…</div>`;
        footer = 'checking for updates…';
        break;
      case 'up_to_date':
        html = `<div class="upd">${icon('check')} Up to date <button class="link" data-act="check">Check now</button></div>`;
        footer = 'up to date';
        break;
      case 'available':
        dot = true;
        footer = `v${v} available`;
        html = u.installable
          ? card(`Update available: v${v}`, notes, `<button class="btn" data-act="install">${icon('download')} Download &amp; install</button>`)
          : card(`v${v} is available`, `${esc(u.why_not)} ${notes}`,
                 `<a class="btn" href="${esc(u.notes_url || u.releases_url)}" target="_blank" rel="noopener">${icon('download')} Download</a>`);
        break;
      case 'downloading':
        dot = true;
        footer = `downloading v${v}…`;
        html = card(`Downloading v${v}…`, '', '<div class="upd-progress"><i></i></div>');
        break;
      case 'ready':
        dot = true;
        footer = `v${v} ready to install`;
        html = card(`v${v} is ready`, `${notes}${notes ? ' · ' : ''}installs by itself when COAT is idle`,
                    `<button class="btn" data-act="install">${icon('refresh')} Restart to update</button>`);
        break;
      case 'installing':
        footer = `installing v${v}…`;
        html = card(`Installing v${v}…`, 'COAT restarts in a moment.', '<div class="upd-progress"><i></i></div>');
        break;
      case 'failed':
        html = `<div class="upd warn">${icon('alert')} Update failed <button class="link" data-act="check">Try again</button></div>
                <div class="upd" title="${esc(u.message)}" style="padding-top:0">${esc(u.message)}</div>`;
        footer = 'update failed';
        break;
    }
    if (u.state !== 'disabled') {
      html += `<label class="upd-auto"><input type="checkbox" data-act="auto" ${u.auto ? 'checked' : ''}> Update automatically</label>`;
    }
    box.innerHTML = html;
    if (foot) foot.textContent = footer;
    if (mini) mini.querySelector('.upd-dot').hidden = !dot;
    clearTimeout(timer);
    const busy = ['idle', 'checking', 'downloading'].includes(u.state);
    timer = setTimeout(load, busy ? 1500 : 60000);
  }
  async function load() {
    try { render(await (await fetch('/api/update')).json()); } catch (e) { timer = setTimeout(load, 60000); }
  }
  async function install(version) {
    render({ state: 'installing', version });
    clearTimeout(timer);
    await post('/api/update/install');
    const deadline = Date.now() + 90000;
    const wait = async () => {
      try {
        const ping = await (await fetch('/api/ping', { cache: 'no-store' })).json();
        if (ping.version !== document.body.dataset.version) {
          // The new version starts fresh: re-run this trace, or reload the start page.
          location.href = root ? `/?q=${encodeURIComponent(root)}` : '/';
          return;
        }
        const status = await (await fetch('/api/update')).json();
        if (status.state === 'failed') return render(status);
      } catch (e) { /* restarting: not answering for a moment */ }
      if (Date.now() < deadline) setTimeout(wait, 800);
    };
    setTimeout(wait, 800);
  }
  box?.addEventListener('click', async (e) => {
    const target = e.target.closest('[data-act]');
    if (!target || target.dataset.act === 'auto') return;
    if (target.dataset.act === 'check') { render({ state: 'checking' }); await post('/api/update/check'); load(); }
    if (target.dataset.act === 'install') {
      const status = await (await fetch('/api/update')).json();
      install(status.version);
    }
  });
  box?.addEventListener('change', async (e) => {
    if (e.target.dataset.act === 'auto') render(await post(`/api/update/auto?on=${e.target.checked ? 1 : 0}`));
  });
  mini?.addEventListener('click', () => window.coatToggleSidebar?.());
  load();

  // ---- recent traces -------------------------------------------------------------------------
  const recent = $('#recent-list');
  async function loadRecent() {
    if (!recent) return;
    try {
      const items = await (await fetch('/api/recent')).json();
      recent.innerHTML = items.length
        ? items.map((r) => `<li><a class="sb-btn${location.pathname === r.url ? ' active' : ''}" href="${esc(r.url)}" title="${esc(r.outcome)}">
             <code>${esc(r.session)}</code><small>${esc(r.summary)} · ${esc(r.when)}${r.scrubbed ? ' · scrubbed' : ''}</small></a></li>`).join('')
        : '<li class="sb-empty">Traces you open show up here.</li>';
    } catch (e) { /* keep what's there */ }
  }
  loadRecent();

  // ---- quit ----------------------------------------------------------------------------------
  $('#quit')?.addEventListener('click', async () => {
    if (!confirm('Stop COAT? Open the COAT app again to start it.')) return;
    try { await fetch('/api/quit', { method: 'POST' }); } catch (e) {}
    document.body.innerHTML = "<div class='landing'><h1>COAT</h1><p class='tag'>COAT has stopped. You can close this tab.</p>" +
      "<p class='muted'>To use it again, open the COAT app.</p></div>";
  });
})();
