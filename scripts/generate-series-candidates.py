"""Build the desktop's review-only proposal index from local metadata reports.

No database or EPUB access. Exact title/author checks prevent mixing snapshots.
"""
import json
import hashlib
import unicodedata
from pathlib import Path

root = Path(__file__).resolve().parents[1]
catalog = {book['id']: book for book in json.loads((root / 'duplicate-analysis-catalog.json').read_text(encoding='utf-8'))}
groups = json.loads((root / 'book-series-proposals.json').read_text(encoding='utf-8'))['series']
levels = {'high': 1, 'medium': 2, 'possible': 3}
candidates = []
for group in groups:
    normalize = lambda text: ''.join(c for c in unicodedata.normalize('NFKC', text).casefold() if c.isalnum())
    identity = json.dumps([normalize(group['proposed_series']), sorted({normalize(author) for author in group['authors']})], ensure_ascii=False, separators=(',', ':'))
    stable_key = hashlib.sha256(identity.encode('utf-8')).hexdigest()
    for position, book in enumerate(group['books']):
        source = catalog.get(book['book_id'])
        if not source or source['title'] != book['title'] or source['author'] != book['author']:
            raise ValueError(f"Mismatched catalog snapshot for book {book['book_id']}")
        candidates.append({
            'path': source['file_path'], 'title': book['title'], 'author': book['author'],
            'group_key': stable_key, 'name': group['proposed_series'],
            'confidence': max(levels[group['confidence']], levels[book.get('membership_confidence', group['confidence'])]), 'volume_order': position,
        })
if len({candidate['path'] for candidate in candidates}) != len(candidates):
    raise ValueError('Proposal paths must have unique membership')
(root / 'src-tauri/src/db/series_candidates.json').write_text(
    json.dumps(candidates, ensure_ascii=False, separators=(',', ':')), encoding='utf-8')
print(f'Generated {len(candidates)} proposal members')
