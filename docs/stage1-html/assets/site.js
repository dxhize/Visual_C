/* Small offline enhancements. Core links/pages work with JavaScript disabled. */
(() => {
  'use strict';
  const root = document.documentElement;
  const theme = document.querySelector('.theme-toggle');
  const nav = document.querySelector('.nav-toggle');
  const sidebar = document.querySelector('.sidebar');
  const overlay = document.querySelector('.search-overlay');
  const searchButton = document.querySelector('.search-toggle');
  const input = document.querySelector('#site-search');
  const results = document.querySelector('.search-results');
  let previousFocus = null;
  try { const saved = localStorage.getItem('stage1-theme'); if (saved === 'dark' || saved === 'light') root.dataset.theme = saved; } catch (_) { /* local file/private storage may be denied */ }
  theme.addEventListener('click', () => {
    const dark = root.dataset.theme === 'dark' || (!root.dataset.theme && matchMedia('(prefers-color-scheme:dark)').matches);
    root.dataset.theme = dark ? 'light' : 'dark';
    try { localStorage.setItem('stage1-theme', root.dataset.theme); } catch (_) { /* no persistent storage */ }
  });
  function toggleNav(force) {
    const open = force === undefined ? !document.body.classList.contains('nav-open') : force;
    document.body.classList.toggle('nav-open', open);
    nav.setAttribute('aria-expanded', String(open));
  }
  nav.addEventListener('click', () => toggleNav());
  document.addEventListener('click', (e) => {
    if (document.body.classList.contains('nav-open') && !sidebar.contains(e.target) && !nav.contains(e.target)) toggleNav(false);
  });
  sidebar.addEventListener('click', (e) => { if (e.target.closest('a')) toggleNav(false); });
  function openSearch() { previousFocus = document.activeElement; overlay.hidden = false; input.value = ''; renderResults(''); input.focus(); }
  function closeSearch() { overlay.hidden = true; input.value = ''; results.replaceChildren(); if (previousFocus && previousFocus.focus) previousFocus.focus(); }
  searchButton.addEventListener('click', openSearch);
  overlay.addEventListener('click', (e) => { if (e.target === overlay) closeSearch(); });
  document.addEventListener('keydown', (e) => {
    const typing = ['INPUT','TEXTAREA'].includes(document.activeElement.tagName) || document.activeElement.isContentEditable;
    if (e.key === 'Escape') { if (!overlay.hidden) closeSearch(); else if (document.body.classList.contains('nav-open')) { toggleNav(false); nav.focus(); } }
    if ((e.key === '/' && !typing && !e.altKey && !e.ctrlKey && !e.metaKey) || ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k')) { e.preventDefault(); if (overlay.hidden) openSearch(); else input.focus(); }
    if (!overlay.hidden && e.key === 'Tab') {
      const focusables = [input, ...results.querySelectorAll('a')]; const first = focusables[0], last = focusables[focusables.length - 1];
      if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
      else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
    }
  });
  const index = window.STAGE1_SEARCH_INDEX || [];
  function score(s, terms) {
    const title = s.title.toLowerCase(), text = s.text.toLowerCase(), headings = s.headings.map(h => h.title.toLowerCase()).join(' ');
    if (!terms.every(t => (title + ' ' + text + ' ' + headings).includes(t))) return 0;
    return terms.reduce((n,t) => n + (title.includes(t) ? 7 : 0) + (headings.includes(t) ? 3 : 0) + (text.includes(t) ? 1 : 0), 0);
  }
  function renderResults(query) {
    results.replaceChildren();
    const terms = query.toLowerCase().trim().split(/\s+/).filter(Boolean).slice(0, 8);
    if (!terms.length) { const hint = document.createElement('p'); hint.textContent = 'Search titles, headings and summaries. All results are local.'; results.append(hint); return; }
    const matches = index.map(item => ({item, rank:score(item,terms)})).filter(x=>x.rank).sort((a,b)=>b.rank-a.rank).slice(0,12);
    if (!matches.length) { const empty = document.createElement('p'); empty.textContent = 'No matching pages. Try a shorter term.'; results.append(empty); return; }
    for (const {item} of matches) {
      const matchedHeading = item.headings.find(h => terms.some(t=>h.title.toLowerCase().includes(t)));
      const a = document.createElement('a'); a.href = matchedHeading ? matchedHeading.url : item.url;
      const strong = document.createElement('strong'); strong.textContent = item.title;
      const small = document.createElement('small'); small.textContent = item.group + (matchedHeading ? ' · ' + matchedHeading.title : '') + ' — ' + item.summary.slice(0,115);
      a.append(strong,small); results.append(a);
    }
  }
  input.addEventListener('input', () => renderResults(input.value));
  for (const btn of document.querySelectorAll('.copy')) {
    btn.addEventListener('click', async () => {
      const text = btn.parentElement.nextElementSibling.textContent;
      try { await navigator.clipboard.writeText(text); btn.textContent = 'Copied'; setTimeout(()=>{btn.textContent='Copy'},1500); }
      catch (_) { btn.textContent='Select text'; setTimeout(()=>{btn.textContent='Copy'},2000); }
    });
  }
  if ('IntersectionObserver' in window) {
    const links = new Map([...document.querySelectorAll('.right-toc a')].map(a=>[decodeURIComponent(a.hash.slice(1)),a]));
    const observer = new IntersectionObserver(entries => { for (const entry of entries) if (entry.isIntersecting) { for (const a of links.values()) a.classList.remove('active'); links.get(entry.target.id)?.classList.add('active'); } }, {rootMargin:'-15% 0px -75% 0px'});
    for (const h of document.querySelectorAll('.document h2,.document h3')) observer.observe(h);
  }
})();
