// Optional real-browser smoke test. Node 22 + installed Google Chrome; no npm deps.
// Usage: node docs/stage1-html/browser-check.mjs [file:///.../index.html | http://localhost:8000/index.html]
import { spawn } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { join, resolve } from 'node:path';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

const root = resolve(import.meta.dirname);
const home = process.argv[2] || pathToFileURL(resolve(root, 'index.html')).href;
const base = new URL('.', home);
const tempRoot = join(tmpdir(), 'kilo');
mkdirSync(tempRoot, { recursive: true });
const profile = mkdtempSync(join(tempRoot, 'visual-c-doc1-'));
const chrome = spawn('google-chrome', [
  '--headless', '--no-sandbox', '--disable-gpu', '--disable-dev-shm-usage',
  '--remote-debugging-port=9229', '--user-data-dir=' + profile,
  'about:blank'
], { stdio: 'ignore' });
let socket;
try {
  let pages;
  for (let i = 0; i < 80; i++) {
    try {
      pages = await (await fetch('http://127.0.0.1:9229/json/list')).json();
      if (pages[0]?.webSocketDebuggerUrl) break;
    } catch { /* Chrome has not opened the debugging port yet. */ }
    await new Promise(done => setTimeout(done, 100));
  }
  if (!pages?.[0]?.webSocketDebuggerUrl) throw Error('Chrome DevTools did not start');
  socket = new WebSocket(pages[0].webSocketDebuggerUrl);
  await new Promise((done, fail) => { socket.addEventListener('open', done, { once: true }); socket.addEventListener('error', fail, { once: true }); });
  let nextId = 0;
  const requests = new Map();
  const failed = [];
  socket.addEventListener('message', event => {
    const result = JSON.parse(event.data);
    if (result.method === 'Network.loadingFailed' && !result.params.canceled) failed.push(result.params.errorText);
    if (result.id && requests.has(result.id)) {
      const { done, fail } = requests.get(result.id);
      requests.delete(result.id);
      result.error ? fail(Error(JSON.stringify(result.error))) : done(result.result);
    }
  });
  function command(method, params = {}) {
    const id = ++nextId;
    return new Promise((done, fail) => {
      requests.set(id, { done, fail });
      socket.send(JSON.stringify({ id, method, params }));
    });
  }
  async function evaluate(expression) {
    const result = await command('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
    if (result.exceptionDetails) throw Error(result.exceptionDetails.text);
    return result.result.value;
  }
  async function navigate(path) {
    await command('Page.navigate', { url: new URL(path, base).href });
    for (let i = 0; i < 80; i++) {
      if (await evaluate(`document.readyState === "complete" && location.href === ${JSON.stringify(new URL(path, base).href)} && !!document.querySelector(".sidebar")`)) break;
      await new Promise(done => setTimeout(done, 100));
    }
    if (!await evaluate('document.readyState === "complete"')) throw Error('Page not loaded: ' + path);
  }
  function assert(value, message) { if (!value) throw Error(message); }
  await command('Network.enable');
  await command('Page.enable');
  await navigate('index.html');
  assert(await evaluate('document.querySelectorAll(".sidebar a").length >= 34'), 'sidebar incomplete');
  const sitePages = await evaluate(`[...document.querySelectorAll('.sidebar a[href$=".html"]')].map(a => a.getAttribute('href'))`);
  assert(await evaluate('document.querySelector(".breadcrumbs a").getAttribute("href") === "index.html"'), 'breadcrumbs');
  assert(await evaluate('!!document.querySelector(".pager a") && !!document.querySelector(".right-toc a")'), 'pager/outline');
  assert(await evaluate('document.querySelector(".theme-toggle").click(); !!document.documentElement.dataset.theme'), 'theme toggle');
  assert(await evaluate(`(() => {document.querySelector('.search-toggle').click(); const q=document.querySelector('#site-search'); q.value='HeapTimeline'; q.dispatchEvent(new Event('input',{bubbles:true})); return document.querySelectorAll('.search-results a').length>0})()`), 'offline symbol search');
  await navigate('call-flow.html');
  const graphSize = await evaluate('window.STAGE1_CALL_GRAPH.nodes.length');
  assert(await evaluate('document.querySelectorAll(".cf-list-item").length') === graphSize, 'call-flow nodes missing');
  assert(await evaluate(`(() => {const s=document.querySelector('.cf-control select');s.value='parser';s.dispatchEvent(new Event('change',{bubbles:true}));return document.querySelectorAll('.cf-list-item').length>0 && document.querySelectorAll('.cf-list-item').length<window.STAGE1_CALL_GRAPH.nodes.length})()`), 'subsystem filter');
  assert(await evaluate(`(() => {const s=document.querySelector('.cf-control select');s.value='all';s.dispatchEvent(new Event('change',{bubbles:true}));const b=[...document.querySelectorAll('.cf-list-item')].find(x=>x.textContent.includes('store.open'));b.click();return document.querySelector('.cf-detail').textContent.includes('src/trace_parser/mod.rs:240') && !!document.querySelector('.cf-doc[href="../../src/trace_parser/mod.rs"]')})()`), 'selection or source link');
  assert(await evaluate(`(() => {const b=[...document.querySelectorAll('.cf-list-item')].find(x=>x.textContent.includes('main'));b.click();const row=[...document.querySelectorAll('.cf-list-row')].find(x=>x.textContent.includes('consumer.heap'));row.querySelector('.cf-list-path').click();return document.querySelector('.cf-route')?.textContent.includes('consumer.heap')})()`), 'multi-hop path');
  assert(await evaluate(`(() => {const ss=[...document.querySelectorAll('.cf-control select')];const g=ss[ss.length-1];g.value='gdb-checkpoint';g.dispatchEvent(new Event('change',{bubbles:true}));return !document.querySelector('.cf-guided').hidden && document.querySelectorAll('.cf-guided-step').length===8 && document.querySelector('.cf-guided-current')?.textContent.includes('trace.gdb')})()`), 'guided path selection');
  assert(await evaluate(`(() => {document.querySelector('.cf-guided-next').click();return document.querySelector('.cf-guided-current')?.textContent.includes('gdb.reader') && document.querySelector('.cf-status').textContent.includes('step 2/8')})()`), 'guided path next/previous');
  await command('Emulation.setDeviceMetricsOverride', { width: 390, height: 844, deviceScaleFactor: 1, mobile: true });
  assert(await evaluate(`(() => {const b=document.querySelector('.nav-toggle');if(getComputedStyle(b).display==='none')return false;b.click();return document.body.classList.contains('nav-open') && b.getAttribute('aria-expanded')==='true'})()`), 'mobile navigation');
  for (const page of sitePages) {
    await navigate(page);
    assert(await evaluate('document.querySelectorAll("main h1").length === 1'), 'missing heading: ' + page);
    assert(await evaluate('[...document.querySelectorAll(".diagram img")].every(img => img.complete && img.naturalWidth > 0)'), 'broken diagram: ' + page);
  }
  assert(!failed.length, 'missing asset requests: ' + failed.join(', '));
  console.log('Browser smoke passed:', home, sitePages.length, 'pages, diagrams, sidebar, breadcrumbs, outline, theme, offline search, graph selection/filter/path, mobile navigation, assets');
} finally {
  socket?.close();
  chrome.kill('SIGTERM');
  if (chrome.exitCode === null) await new Promise(done => chrome.once('exit', done));
  await new Promise(done => setTimeout(done, 500));
  try {
    rmSync(profile, { recursive: true, force: true, maxRetries: 15, retryDelay: 100 });
  } catch {
    // Chrome can still hold a singleton/lock while exiting; the system temp
    // directory reclaims the disposable profile. Never fail a passed smoke test
    // solely because a browser has not released a file yet.
  }
}
