#!/usr/bin/env python3
"""Local, no-network link/anchor/asset/SVG/structure checker for generated docs."""
from collections import Counter
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit
from xml.etree import ElementTree as ET
import json
import re
import sys

ROOT=Path(__file__).resolve().parent
sys.path.insert(0,str(ROOT))
from build import PAGES, DIAGRAMS, slugify  # noqa: E402 — static manifest, no generation on import

class Page(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.ids=[];self.links=[];self.images=[];self.headings=Counter();self.figures=0;self.captions=0;self.css=0;self.scripts=0;self.current=0;self.nav=0;self.title=0;self.doctype=0
    def handle_decl(self,decl):
        if decl.lower()=='doctype html':self.doctype+=1
    def handle_starttag(self,tag,attrs):
        d=dict(attrs)
        if 'id' in d:self.ids.append(d['id'])
        if 'href' in d:self.links.append(d['href'])
        if 'src' in d:self.links.append(d['src'])
        if tag=='img':self.images.append(d)
        if tag in ('h1','h2','h3','h4'):self.headings[tag]+=1
        if tag=='figure':self.figures+=1
        if tag=='figcaption':self.captions+=1
        if tag=='link' and d.get('rel')=='stylesheet':self.css+=1
        if tag=='script':self.scripts+=1
        if tag=='title':self.title+=1
        if tag=='nav' and d.get('aria-label')=='Documentation navigation':self.nav+=1

errors=[]
pages={}
for slug,_,_ in PAGES:
    path=ROOT/(slug+'.html')
    if not path.exists():errors.append(f'missing page {path}');continue
    data=path.read_text(encoding='utf-8');p=Page();p.feed(data);p.close();pages[path.resolve()]=p
    if len(p.ids)!=len(set(p.ids)):errors.append(f'{path}: duplicate ids {list((Counter(p.ids)-Counter(set(p.ids))).elements())}')
    if not (p.doctype==1 and p.title==1 and p.headings['h1']==1 and p.headings['h2']>=1):errors.append(f'{path}: missing HTML5 doctype/title/one H1/section headings')
    extras = path.name == 'call-flow.html'
    if p.css!=(2 if extras else 1) or p.scripts!=(5 if extras else 2):errors.append(f'{path}: missing local shared CSS/search/scripts')
    if p.figures!=p.captions:errors.append(f'{path}: diagram missing caption')
    if not all(img.get('alt') for img in p.images):errors.append(f'{path}: image missing alt')
    if 'https://' in data or 'http://' in data or re.search(r'\b(?:fetch|XMLHttpRequest)\s*\(',data):errors.append(f'{path}: network reference')

# Every generated link is relative/local; validate target existence and anchors.
checked=0
for path,p in pages.items():
    for link in p.links:
        if link.startswith(('http:','https:','//','data:','javascript:')):errors.append(f'{path.name}: external/embedded resource {link}');continue
        u=urlsplit(link);target=(path.parent/unquote(u.path)).resolve() if u.path else path
        if not target.is_file():errors.append(f'{path.name}: missing local target {link}');continue
        if u.fragment and target.suffix.lower()=='.html':
            other=pages.get(target)
            if other is None:
                other=Page();other.feed(target.read_text(encoding='utf-8'));other.close()
            if unquote(u.fragment) not in other.ids:errors.append(f'{path.name}: broken anchor {link}')
        checked+=1

for name,d in DIAGRAMS.items():
    path=ROOT/'diagrams'/f'{name}.svg'
    if not path.exists():errors.append(f'missing SVG {path}');continue
    try:
        x=ET.parse(path).getroot()
        if not x.tag.endswith('svg') or not x.get('viewBox'):errors.append(f'{path}: missing svg/viewBox')
        for req in ('title','desc'):
            if not x.find('{http://www.w3.org/2000/svg}'+req) is not None:errors.append(f'{path}: missing {req}')
        if len(x.findall('{http://www.w3.org/2000/svg}rect'))<len(d['nodes']):errors.append(f'{path}: fewer boxes than source nodes')
    except ET.ParseError as err:errors.append(f'{path}: invalid SVG: {err}')

assert (ROOT/'assets/search-data.js').exists()
index=json.loads((ROOT/'assets/search.json').read_text(encoding='utf-8'))
if not {(slug+'.html') for slug,_,_ in PAGES}.issubset({entry['url'] for entry in index}):errors.append('search index does not cover every page')
inventory=json.loads((ROOT/'assets/data/stage1-inventory.json').read_text(encoding='utf-8'))
repo=ROOT.parent.parent
actual={p.relative_to(repo).as_posix() for p in (repo/'src').rglob('*.rs') if not p.is_relative_to(repo/'src/semantic')}
actual|={p.relative_to(repo).as_posix() for p in (repo/'c_runtime').glob('*.c')}
actual.add('Cargo.toml')
catalog={item['path'] for item in inventory['files']}
if actual!=catalog:errors.append(f'inventory coverage differs: missing={sorted(actual-catalog)}, stale={sorted(catalog-actual)}')
if any(s['path'].startswith('src/semantic/') for s in inventory['symbols']):errors.append('Stage 2 symbols in inventory')
search_urls={entry['url'] for entry in index}
if any('files.html#file-'+slugify(f['path']) not in search_urls for f in inventory['files']):errors.append('search missing a source file')
if any('functions.html#symbol-'+slugify(s['id']) not in search_urls for s in inventory['symbols']):errors.append('search missing a source symbol')
graph=json.loads((ROOT/'assets/data/verified-relationships.json').read_text(encoding='utf-8'))
node_ids={n['id'] for n in graph['nodes']}
if len(node_ids)!=len(graph['nodes']):errors.append('duplicate call-flow node id')
for n in graph['nodes']:
    if n['path'].startswith('src/semantic/'):errors.append('Stage 2 call-flow node '+n['id'])
    target=repo/n['path']
    if not target.is_file():errors.append('missing graph source '+n['path']);continue
    lines=target.read_text(encoding='utf-8').splitlines()
    if not 1<=n['line']<=len(lines):errors.append('graph line out of range '+n['source'])
    page=n.get('docPage','')
    if page not in {(slug+'.html') for slug,_,_ in PAGES}:errors.append('graph docPage missing '+str(page))
for e in graph['edges']:
    if e['from'] not in node_ids or e['to'] not in node_ids:errors.append('graph edge endpoint missing '+e['from']+'->'+e['to'])
    if e['kind'] not in {'direct','callback','external','data','conceptual'}:errors.append('unknown graph edge kind '+e['kind'])
    path,line=e['evidence'].rsplit(':',1)
    target=repo/path
    if not target.is_file():errors.append('missing graph evidence file '+path);continue
    if not 1<=int(line)<=len(target.read_text(encoding='utf-8').splitlines()):errors.append('graph evidence line out of range '+e['evidence'])
if set(pages)!={(ROOT/(slug+'.html')).resolve() for slug,_,_ in PAGES}:errors.append('page count mismatch')
print(f'Checked {len(pages)} HTML pages, {checked} local references (including anchors), {len(DIAGRAMS)} SVGs and search index.')
if errors:
    print('\n'.join('ERROR: '+e for e in errors));sys.exit(1)
print('All documentation checks passed.')
