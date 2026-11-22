#!/usr/bin/env python3
"""Export the site's chapter Markdown verbatim to one editable file.

Refuses to overwrite a reader's edits unless --force is explicitly supplied.
--check verifies that the export still matches all chapter sources.
"""
from __future__ import annotations

import argparse
from pathlib import Path

from build import PAGES, ROOT, SOURCE

DEST = ROOT / 'stage1-complete-text.md'


def begin(slug: str) -> str:
    return f'<!-- BEGIN CHAPTER: content/{slug}.md -->\n'


def end(slug: str) -> str:
    return f'<!-- END CHAPTER: content/{slug}.md -->\n'


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='compare export with its chapter sources')
    parser.add_argument('--force', action='store_true', help='overwrite an existing export')
    args = parser.parse_args()
    if args.check:
        text = DEST.read_text(encoding='utf-8')
        for slug, _, _ in PAGES:
            start, stop = begin(slug), end(slug)
            assert text.count(start) == text.count(stop) == 1, slug
            chapter = text.split(start, 1)[1].split(stop, 1)[0]
            assert chapter == (SOURCE / f'{slug}.md').read_text(encoding='utf-8'), slug
        print(f'Verified {len(PAGES)} unchanged chapters in {DEST}')
        return
    if DEST.exists() and not args.force:
        parser.error(f'{DEST} already exists; refusing to overwrite your edits (use --force)')

    header = (
        '# Complete text of the Visual C Stage 1 documentation\n\n'
        'This is an editable copy of the Markdown text in all 67 offline-site chapters, '
        'in the same order as the site navigation. The text **inside each BEGIN/END '
        'CHAPTER pair is copied verbatim** from the named `content/*.md` file. '
        'Edit that text and keep the chapter markers so your changes can later be '
        'applied to the corresponding pages. Changes to this collection do not '
        'automatically change the site. Do not regenerate this file after editing '
        'it unless you intend to discard those edits.\n\n'
        'Diagram, inventory and interactive-explorer placeholders are preserved '
        'exactly as they appear in the chapter sources; their generated graphics, '
        'source inventory and interface are not prose from the chapters. '
        'Links ending in `.html` are the original site links.\n\n'
        '## Chapter list\n\n'
    )
    lines = [header]
    for number, (slug, title, group) in enumerate(PAGES, 1):
        lines.append(f'{number}. **{group} · {title}** — `content/{slug}.md`\n')
    lines.append('\n---\n\n')
    for slug, _, _ in PAGES:
        chapter = (SOURCE / f'{slug}.md').read_text(encoding='utf-8')
        assert chapter.endswith('\n'), slug
        lines.extend((begin(slug), chapter, end(slug), '\n'))
    DEST.write_text(''.join(lines), encoding='utf-8')
    print(f'Exported {len(PAGES)} chapters to {DEST}')


if __name__ == '__main__':
    main()
