// The app shell on every page: sidebar open/collapsed (remembered), ⌘B / Ctrl+B,
// the slide-in sheet on narrow screens, and highlighting the section you're looking at.
(() => {
  const shell = document.getElementById('shell');
  if (!shell) return;
  const KEY = 'coat.sidebar';
  const narrow = () => matchMedia('(max-width: 820px)').matches;
  try { if (localStorage.getItem(KEY) === 'collapsed') shell.dataset.sidebar = 'collapsed'; } catch (e) {}

  function toggle() {
    if (narrow()) { shell.dataset.mobile = shell.dataset.mobile === 'open' ? '' : 'open'; return; }
    shell.dataset.sidebar = shell.dataset.sidebar === 'collapsed' ? 'expanded' : 'collapsed';
    try { localStorage.setItem(KEY, shell.dataset.sidebar); } catch (e) {}
  }
  window.coatToggleSidebar = toggle;
  document.querySelectorAll('[data-sidebar-toggle]').forEach((b) => b.addEventListener('click', toggle));
  document.getElementById('sb-backdrop')?.addEventListener('click', () => { shell.dataset.mobile = ''; });
  document.addEventListener('keydown', (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'b') { e.preventDefault(); toggle(); }
    if (e.key === 'Escape') shell.dataset.mobile = '';
  });
  document.querySelectorAll('.sidebar a').forEach((a) => a.addEventListener('click', () => { if (narrow()) shell.dataset.mobile = ''; }));

  // Collapsed: the search icon opens the sidebar and focuses the box.
  const search = document.querySelector('.sb-search');
  search?.addEventListener('click', () => {
    if (shell.dataset.sidebar === 'collapsed' && !narrow()) { toggle(); setTimeout(() => search.querySelector('input')?.focus(), 220); }
  });
  // "/" focuses the search box, like many web apps.
  document.addEventListener('keydown', (e) => {
    const input = search?.querySelector('input');
    if (e.key === '/' && input && !/INPUT|TEXTAREA|SELECT/.test(document.activeElement?.tagName || '')) { e.preventDefault(); input.focus(); }
  });

  // Highlight the sidebar entry of the section (and step) on screen.
  const links = [...document.querySelectorAll('.sidebar [data-spy]')];
  const targets = links.map((a) => document.getElementById(a.dataset.spy)).filter(Boolean);
  if (!('IntersectionObserver' in window) || !targets.length) return;
  const visible = new Set();
  const update = () => {
    const sections = targets.filter((t) => !t.id.startsWith('step-') && visible.has(t.id));
    const steps = targets.filter((t) => t.id.startsWith('step-') && visible.has(t.id));
    const section = sections[0]?.id, step = steps[0]?.id;
    links.forEach((a) => {
      const id = a.dataset.spy;
      a.classList.toggle('active', id === section || id === step);
      a.classList.toggle('has-active', a.dataset.spyParent === 'steps' && !!step);
    });
  };
  const observer = new IntersectionObserver((entries) => {
    entries.forEach((en) => (en.isIntersecting ? visible.add(en.target.id) : visible.delete(en.target.id)));
    update();
  }, { rootMargin: '-64px 0px -55% 0px' });
  targets.forEach((t) => observer.observe(t));
})();
