// The start page: start a trace, show progress, open the report.
(() => {
  const $ = (s) => document.querySelector(s);
  const form = $('#f'), input = $('#q'), scrub = $('#scrub'), fresh = $('#fresh');
  const progress = $('#progress'), plog = $('#plog'), perr = $('#perr'), bar = $('.progress .bar');
  async function start(target) {
    progress.classList.add('on'); bar.style.display = ''; plog.textContent = ''; perr.textContent = '';
    const query = `q=${encodeURIComponent(target)}&scrub=${scrub.checked ? 1 : 0}${fresh.checked ? '&fresh=1' : ''}`;
    const response = await fetch(`/api/start?${query}`);
    const body = await response.json();
    if (!response.ok) return fail(body.error);
    poll(body.job);
  }
  async function poll(id) {
    const body = await (await fetch(`/api/job/${id}`)).json();
    plog.textContent = (body.log || []).join('\n');
    if (body.state === 'done') { location.href = body.url; return; }
    if (body.state === 'failed' || body.error) return fail(body.error);
    setTimeout(() => poll(id), 300);
  }
  function fail(message) { bar.style.display = 'none'; perr.textContent = message || 'Something went wrong'; }
  const looksLikeCall = (text) => /sessionid=|\[LID:|^\s*[A-Za-z0-9_-]{4,64}\s*$/.test(text);
  form.addEventListener('submit', (e) => { e.preventDefault(); if (input.value.trim()) start(input.value.trim()); });
  // Paste a link and it starts right away, no button needed.
  input.addEventListener('paste', () => setTimeout(() => { if (looksLikeCall(input.value)) start(input.value.trim()); }, 0));
  // Dropping a link anywhere on the page works too.
  document.addEventListener('dragover', (e) => e.preventDefault());
  document.addEventListener('drop', (e) => {
    e.preventDefault();
    const text = e.dataTransfer.getData('text/uri-list') || e.dataTransfer.getData('text/plain');
    if (text && looksLikeCall(text)) { input.value = text.trim(); start(input.value); }
  });
  
  const params = new URLSearchParams(location.search);
  const preset = params.get('q') || params.get('sessionid');
  if (params.get('scrub') === '1') scrub.checked = true;
  if (params.has('fresh')) fresh.checked = true;
  if (preset) { input.value = preset; history.replaceState(null, '', '/'); start(preset); }
})();
