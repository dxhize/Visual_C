#!/usr/bin/env python3
"""Offline Stage 1 docs builder. Python stdlib only; run from any directory.

Edit content/*.md and diagrams.json, then run: python3 docs/stage1-html/build.py
The Markdown subset is deliberately small: headings, paragraphs, fenced code,
pipe tables, lists, blockquote callouts, links, inline code and {{diagram:id}}.
"""
from __future__ import annotations
import html
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent
SOURCE = ROOT / 'content'
SVG = ROOT / 'diagrams'
PAGES = [
 ('index','Start here','Introduction'),
 ('architecture','System architecture','Introduction'),
 ('engine-build','Compilation & diagnostics','Trace Engine'),
 ('engine-debugger','GDB/MI and checkpoints','Trace Engine'),
 ('engine-memory','Frames, variables & memory','Trace Engine'),
 ('engine-metadata','DWARF, Clang & fusion','Trace Engine'),
 ('engine-heap','Allocation lifetimes','Trace Engine'),
 ('engine-diagnostics','Valgrind & sanitizers','Trace Engine'),
 ('engine-process','Processes & event transport','Trace Engine'),
 ('engine-limits','Budgets, gaps & export','Trace Engine'),
 ('model','Trace data model','Data model'),
 ('parser-ingestion','TP-1 · Load & compatibility','Trace Parser'),
 ('parser-indexes','TP-2 · Position indexes','Trace Parser'),
 ('parser-views','TP-3 · Borrowed views','Trace Parser'),
 ('parser-validation','TP-5 · Deep validation','Trace Parser'),
 ('parser-semantic','TP-6 · Evidence confidence','Trace Parser'),
 ('parser-consumer','TP-7 · Bounded queries','Trace Parser'),
 ('usage','Worked API walkthrough','Practical usage'),
 ('api','Public API reference','Reference'),
 ('schema','Schema & identity reference','Reference'),
 ('handoff','Stage 2 handoff & limits','Reference'),
]
GROUPS = list(dict.fromkeys(group for _,_,group in PAGES))
TITLES = {slug:title for slug,title,_ in PAGES}
DIAGRAMS = json.loads((ROOT / 'diagrams.json').read_text(encoding='utf-8'))

def esc(text): return html.escape(text, quote=True)
def slugify(text):
    text = re.sub(r'<[^>]+>', '', text).replace('`','')
    return re.sub(r'[^a-z0-9]+','-',text.lower()).strip('-')

def inline(text):
    """Escape first, then mark up only deliberately supported inline constructs."""
    tokens = []
    def hold(fragment):
        tokens.append(fragment)
        return f'\x02{len(tokens)-1}\x03'
    text = re.sub(r'`([^`]+)`', lambda m:hold('<code>'+esc(m[1])+'</code>'),text)
    text = re.sub(r'\[([^\]]+)\]\(([^)]+)\)',lambda m:hold('<a href="'+esc(m[2])+'">'+esc(m[1])+'</a>'),text)
    text = esc(text)
    text = re.sub(r'\*\*([^*]+)\*\*',r'<strong>\1</strong>',text)
    text = re.sub(r'(?<!\*)\*([^*]+)\*(?!\*)',r'<em>\1</em>',text)
    return re.sub(r'\x02(\d+)\x03',lambda m:tokens[int(m[1])],text)

# Highlight only known lexical tokens; input always HTML-escaped by esc().
TOKENS = re.compile(r'(?P<comment>//[^\n]*|/\*[\s\S]*?\*/|#[^\n]*?(?=\n|$))|(?P<string>"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\')|(?P<number>\b\d+(?:_\d+)*(?:\.\d+)?\b)|(?P<keyword>\b(?:pub|fn|let|mut|struct|enum|impl|self|Self|use|mod|if|else|match|Some|None|Ok|Err|return|for|in|while|async|await|const|static|type|true|false|int|char|void|size_t|unsigned|include|define|null|break|continue|def|class|from|import|as|echo|cargo|target|cd|cat|python3|gcc|gdb|jq)\b)')
def highlight(src, lang):
    if lang not in ('rust','c','json','sh','shell','bash'): return esc(src)
    out=[]; last=0
    for m in TOKENS.finditer(src):
        # JSON uses # only inside strings; the lexer matches strings first.
        out.append(esc(src[last:m.start()]))
        cls=m.lastgroup
        if lang=='json' and cls=='comment': cls=None
        out.append('<span class="tok-'+cls+'">'+esc(m.group())+'</span>' if cls else esc(m.group()))
        last=m.end()
    out.append(esc(src[last:]))
    return ''.join(out)

def render_md(text):
    lines=text.splitlines(); out=[]; toc=[]; i=0; seen={}
    while i<len(lines):
        line=lines[i]; s=line.strip()
        if not s: i+=1;continue
        if s.startswith('```'):
            lang=s[3:].strip().lower() or 'text'; body=[];i+=1
            while i<len(lines) and not lines[i].strip().startswith('```'):
                body.append(lines[i]);i+=1
            assert i<len(lines),f'Unclosed code block: {body[:2]}'
            out.append('<div class="code-head"><span>'+esc(lang.upper())+'</span><button type="button" class="copy" aria-label="Copy code">Copy</button></div><pre class="language-'+esc(lang)+'"><code>'+highlight('\n'.join(body),lang)+'</code></pre>');i+=1;continue
        diagram=re.fullmatch(r'\{\{diagram:([\w-]+)\}\}',s)
        if diagram:
            key=diagram[1]; assert key in DIAGRAMS, key
            d=DIAGRAMS[key]
            out.append('<figure class="diagram"><a href="diagrams/'+esc(key)+'.svg" aria-label="Open full-size '+esc(d['title'])+' diagram"><img src="diagrams/'+esc(key)+'.svg" alt="'+esc(d['alt'])+'"></a><figcaption><strong>Figure · '+esc(d['title'])+'.</strong> '+inline(d.get('caption',d['alt']))+' <a href="diagrams/'+esc(key)+'.svg">View SVG</a></figcaption></figure>');i+=1;continue
        heading=re.match(r'^(#{1,4})\s+(.+)$',s)
        if heading:
            level=len(heading[1]);title=heading[2]; key=slugify(title)
            seen[key]=seen.get(key,0)+1
            if seen[key]>1:key+=f'-{seen[key]}'
            toc.append((level,key,re.sub(r'`','',title)))
            out.append(f'<h{level} id="{esc(key)}">{inline(title)} <a class="anchor" href="#{esc(key)}" aria-label="Link to {esc(title)}">#</a></h{level}>');i+=1;continue
        if s.startswith('|') and i+1<len(lines) and re.match(r'^\s*\|[\s:|\-]+\|\s*$',lines[i+1]):
            head=[x.strip() for x in s.strip('|').split('|')];i+=2; rows=[]
            while i<len(lines) and lines[i].strip().startswith('|'):
                rows.append([x.strip() for x in lines[i].strip().strip('|').split('|')]);i+=1
            assert all(len(r)==len(head) for r in rows),'Table columns mismatch'
            out.append('<div class="table-scroll"><table><thead><tr>'+''.join('<th scope="col">'+inline(c)+'</th>' for c in head)+'</tr></thead><tbody>'+''.join('<tr>'+''.join('<td>'+inline(c)+'</td>' for c in row)+'</tr>' for row in rows)+'</tbody></table></div>');continue
        if s.startswith('> '):
            para=[]
            while i<len(lines) and lines[i].lstrip().startswith('> '):para.append(lines[i].lstrip()[2:]);i+=1
            kind='warn' if para[0].startswith('WARNING:') else 'note'
            out.append('<aside class="admonition '+kind+'" role="note"><p>'+inline(' '.join(para))+'</p></aside>');continue
        if re.match(r'^[-*] ',s) or re.match(r'^\d+\. ',s):
            ordered=bool(re.match(r'^\d+\. ',s)); tag='ol' if ordered else 'ul';items=[]
            while i<len(lines) and (re.match(r'^\s*[-*] ',lines[i]) or re.match(r'^\s*\d+\. ',lines[i])):
                items.append(re.sub(r'^\s*(?:[-*]|\d+\.)\s+','',lines[i]));i+=1
            out.append('<'+tag+'>'+''.join('<li>'+inline(x)+'</li>' for x in items)+'</'+tag+'>');continue
        para=[]
        while i<len(lines) and lines[i].strip() and not (lines[i].lstrip().startswith(('```','# ','## ','### ','#### ','|','> ','{{diagram:')) or re.match(r'^\s*(?:[-*]|\d+\.)\s+',lines[i])):
            para.append(lines[i].strip());i+=1
        assert para,(i,s)
        out.append('<p>'+inline(' '.join(para))+'</p>')
    return '\n'.join(out), toc

def write_diagrams():
    SVG.mkdir(exist_ok=True)
    colors={'engine':('#185a97','#e7f2ff'),'parser':('#306451','#e8f6ee'),'evidence':('#825329','#fff3e0'),'risk':('#8b4146','#ffeded'),'source':('#555a85','#eeeeff')}
    for key,d in DIAGRAMS.items():
        nodes=d['nodes']; vertical=d.get('vertical',False)
        width=780 if vertical else max(840, len(nodes)*218+30)
        height=120+len(nodes)*98 if vertical else 235
        boxw=666 if vertical else 194;boxh=70 if vertical else 118
        svg=[f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" role="img" aria-labelledby="t desc"><title id="t">{esc(d["title"])}</title><desc id="desc">{esc(d["alt"])}</desc>',
        '<defs><marker id="arrow" markerWidth="10" markerHeight="10" refX="8" refY="5" orient="auto"><path d="M 0 0 L 10 5 L 0 10 z" fill="#657890"/></marker></defs>',
        f'<rect width="{width}" height="{height}" rx="16" fill="#fafcff"/>',
        f'<text x="24" y="32" font-family="system-ui,sans-serif" font-size="17" font-weight="700" fill="#213a54">{esc(d["title"])}</text>']
        for i,n in enumerate(nodes):
            x=55 if vertical else 22+i*218;y=50+i*98 if vertical else 57
            if i:
                if vertical:
                    svg.append(f'<path d="M {x+boxw//2} {y-27} V {y-9}" stroke="#657890" stroke-width="2" marker-end="url(#arrow)"/>')
                else: svg.append(f'<path d="M {x-20} {y+boxh//2} H {x-6}" stroke="#657890" stroke-width="2" marker-end="url(#arrow)"/>')
            fg,bg=colors[n.get('kind','engine')]
            svg.append(f'<rect x="{x}" y="{y}" width="{boxw}" height="{boxh}" rx="11" fill="{bg}" stroke="{fg}" stroke-width="1.5"/>')
            svg.append(f'<text x="{x+14}" y="{y+29}" font-family="system-ui,sans-serif" font-size="15" font-weight="700" fill="{fg}">{esc(n["label"])}</text>')
            for j,text in enumerate(n.get('lines',[])):
                svg.append(f'<text x="{x+14}" y="{y+53+j*17}" font-family="system-ui,sans-serif" font-size="12.4" fill="#33465c">{esc(text)}</text>')
        legend='Blue: engine  ·  Green: parser  ·  Amber: evidence  ·  Red: partial/unknown  ·  Purple: source'
        svg.append(f'<text x="24" y="{height-22}" font-family="system-ui,sans-serif" font-size="12" fill="#44566b">{esc(legend)}</text></svg>')
        (SVG/(key+'.svg')).write_text('\n'.join(svg)+'\n',encoding='utf-8')

def navigation(current):
    return ''.join('<div class="nav-group"><div class="nav-label">'+esc(g)+'</div>'+''.join('<a href="'+slug+'.html"'+(' aria-current="page"' if slug==current else '')+'>'+esc(title)+'</a>' for slug,title,group in PAGES if group==g)+'</div>' for g in GROUPS)

def build():
    write_diagrams(); search=[]
    for i,(slug,title,group) in enumerate(PAGES):
        path=SOURCE/(slug+'.md');assert path.exists(),path
        source=path.read_text(encoding='utf-8'); body,toc=render_md(source)
        assert all(any(k.startswith(f'level-{n}-') for _,k,_ in toc) for n in (1,2,3)), (slug,'missing levels')
        prev=PAGES[i-1] if i>0 else None; nxt=PAGES[i+1] if i+1<len(PAGES) else None
        plain=re.sub(r'\s+',' ',re.sub(r'[#*`|>\[\]]','',source)).strip()
        search.append(dict(url=slug+'.html',title=title,group=group,summary=plain[:230],text=plain,headings=[dict(title=t,url=slug+'.html#'+k) for _,k,t in toc]))
        toc_html=''.join('<a href="#'+esc(k)+'" class="toc-'+str(level)+'">'+esc(label)+'</a>' for level,k,label in toc if level in (2,3))
        pagination='<nav class="pager" aria-label="Adjacent pages">'+ ('<a href="'+prev[0]+'.html">← '+esc(prev[1])+'</a>' if prev else '<span></span>') + ('<a href="'+nxt[0]+'.html">'+esc(nxt[1])+' →</a>' if nxt else '<span></span>')+'</nav>'
        doc=f'''<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta name="color-scheme" content="light dark"><title>{esc(title)} — Visual C Stage 1</title><meta name="description" content="Offline Visual C Stage 1 systems documentation: {esc(title)}"><link rel="stylesheet" href="assets/site.css"><script src="assets/search-data.js" defer></script><script src="assets/site.js" defer></script></head>
<body><a class="skip" href="#main">Skip to content</a><header class="topbar"><button class="nav-toggle" type="button" aria-label="Open navigation" aria-controls="sidebar" aria-expanded="false">☰</button><a class="brand" href="index.html" aria-label="Visual C documentation home"><span class="brandmark">V<span>C</span></span> Visual C <small>Stage 1 · V2.2</small></a><div class="top-actions"><button class="search-toggle" type="button" aria-label="Search documentation">⌕ <span>Search <kbd>/</kbd></span></button><button class="theme-toggle" type="button" aria-label="Change color theme" title="Switch light or dark mode">◐</button></div></header>
<div class="shell"><aside class="sidebar" id="sidebar" aria-label="Documentation navigation"><nav>{navigation(slug)}</nav><div class="side-foot">Offline reference · schema v2<br>Engine + Parser, not Stage 2</div></aside><main id="main" tabindex="-1"><nav class="breadcrumbs" aria-label="Breadcrumbs"><a href="index.html">Documentation</a><span aria-hidden="true">›</span><span>{esc(group)}</span><span aria-hidden="true">›</span><span aria-current="page">{esc(title)}</span></nav><article class="document">{body}</article>{pagination}<footer class="footer">Visual C Stage 1 · Offline documentation · <a href="handoff.html">Evidence contract</a> · <a href="../stage1-final-audit.md">Freeze audit (Markdown)</a></footer></main><aside class="right-toc" aria-label="On this page"><p>On this page</p><nav>{toc_html}</nav></aside></div>
<div class="search-overlay" hidden><div class="search-dialog" role="dialog" aria-modal="true" aria-label="Search documentation"><label for="site-search">Search Stage 1 reference <kbd>Esc</kbd></label><input type="search" id="site-search" placeholder="Try “realloc”, “EvidenceRef”, “GDB” …" autocomplete="off"><div class="search-results" role="status" aria-live="polite"></div></div></div></body></html>
'''
        (ROOT/(slug+'.html')).write_text(doc,encoding='utf-8')
    data=json.dumps(search,ensure_ascii=False,separators=(',',':'))
    (ROOT/'assets/search.json').write_text(data,encoding='utf-8')
    # A regular script rather than fetch() makes local search work under file://
    # without a web server or browser-specific CORS permission.
    (ROOT/'assets/search-data.js').write_text('window.STAGE1_SEARCH_INDEX = '+data+';\n',encoding='utf-8')
    print(f'Built {len(PAGES)} pages, {len(DIAGRAMS)} SVG diagrams and local search index.')

if __name__=='__main__':build()
