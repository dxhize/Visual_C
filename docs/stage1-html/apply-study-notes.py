#!/usr/bin/env python3
"""Archival helper for the site's former study-note headings.

Do not use with the restructured chapters. It refuses to edit any chapter
unless *all* note markers still exist, preventing accidental duplicate intros.
"""
from __future__ import annotations
import json
from pathlib import Path

root = Path(__file__).resolve().parent
notes = json.loads((root / 'study-notes.json').read_text(encoding='utf-8'))
start = '## Start here: the everyday explanation'
end = '### Keep reading: the detailed explanation'
if any(f'\n{start}\n' not in (root / 'content' / f'{slug}.md').read_text(encoding='utf-8') for slug in notes):
    raise SystemExit('Study-note markers no longer exist: edit content/*.md directly; no files were changed.')
for slug, entry in notes.items():
    page = root / 'content' / f'{slug}.md'
    text = page.read_text(encoding='utf-8')
    assert text.startswith('# '), page
    term_list = '\n'.join(f'- **{name}:** {definition}' for name, definition in entry['terms'])
    section = (
        f'{start}\n\n{entry["intro"]}\n\n'
        f'### Words used on this page\n\n{term_list}\n\n'
        f'### Follow a small example\n\n{entry["example"]}\n\n'
        f'**What not to assume:** {entry["boundary"]}\n\n{end}\n'
    )
    if f'\n{start}\n' in text:
        before, after = text.split(f'\n{start}\n', 1)
        assert f'\n{end}\n' in after, f'Missing detailed chapter marker: {page}'
        _, remaining = after.split(f'\n{end}\n', 1)
        updated = before + '\n' + section + remaining
    else:
        title, remainder = text.split('\n', 1)
        updated = title + '\n\n' + section + '\n' + remainder.lstrip('\n')
    page.write_text(updated, encoding='utf-8')
print(f'Updated plain-language introductions in {len(notes)} Markdown chapters.')
