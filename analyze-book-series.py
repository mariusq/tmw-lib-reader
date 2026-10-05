"""Conservative title/author series proposals. Never updates SQLite or EPUBs."""
import collections
import datetime
import html
import json
import re
import unicodedata
from pathlib import Path

def norm(value):
    value = value or ''
    for _ in range(3):
        value = html.unescape(value)
    return unicodedata.normalize('NFKC', value).casefold().strip()

def key(value):
    return ''.join(c for c in norm(value) if c.isalnum())

def clean(title):
    title = norm(title)
    title = re.sub(r'[【\[][^】\]]*(?:電子|特典|限定|書き下ろし)[^】\]]*[】\]]', '', title)
    title = re.sub(r'\([^()]*(?:文庫|ブックス|books|ノベルス|novels|コミックス|コミック|出版|レーベル)[^()]*\)', '', title)
    return title.strip()

NUMBER = r'(?:\d{1,3}(?:\.\d)?|[〇零一二三四五六七八九十百]+|[ivx]{1,6})'
PATTERNS = [
    ('explicit_volume', re.compile(r'(?i)(?:第\s*|\bvol(?:ume)?\.?\s*|\bbook\s*|\b巻\s*)(' + NUMBER + r')\s*(?:巻|冊|部|話)?')),
    ('numbered_volume', re.compile(r'(\d{1,3}(?:\.\d)?)\s*巻')),
    ('bracketed_volume', re.compile(r'[\(\[〈《](' + NUMBER + r'|上|中|下)[\)\]〉》]')),
    ('trailing_number', re.compile(r'(\d{1,3}(?:\.\d)?)\s*$')),
    ('separated_number', re.compile(r'(?<=\s)(' + NUMBER + r')(?=\s|[~〜:「『―—-])')),
    ('trailing_part', re.compile(r'(?:\s|[・:])([上中下])\s*$')),
]

def marker(title):
    for method, pattern in PATTERNS:
        for match in pattern.finditer(title):
            prefix = title[:match.start()].strip(' .・:-~〜―—')
            if len(key(prefix)) < 3:
                continue
            # Dates, periodical issues and technical version numbers are not volumes.
            if re.search(r'雑誌|週刊|月刊|年\d|\d月|\bno\.|\bversion\b', title):
                continue
            suffix = title[match.end():].strip(' .・:-~〜―—')
            volume = match.group(1)
            if volume in ('上', '中', '下'):
                parent = re.search(r'\s*(\d{1,3})\s*$', prefix)
                if parent and len(key(prefix[:parent.start()])) >= 3:
                    volume = parent.group(1) + ' ' + volume
                    prefix = prefix[:parent.start()].strip()
            return prefix, volume, method, suffix
    return None

rows = json.loads(Path('series-analysis-catalog.json').read_text(encoding='utf-8'))
prepared = []
for row in rows:
    title = clean(row['title'])
    author = key(row['author'])
    if author in ('unknown', '不明', '作者不詳'):
        author = ''
    prepared.append(dict(row, cleaned=title, author_key=author, marker=marker(title)))

groups = collections.defaultdict(dict)
for row in prepared:
    if row['author_key'] and row['marker']:
        base, volume, method, suffix = row['marker']
        groups[(row['author_key'], key(base))][row['id']] = (row, base, volume, method, suffix)

# Unnumbered first volumes are eligible only for an exact normalized base-title match.
by_title = collections.defaultdict(list)
for row in prepared:
    if row['author_key']:
        by_title[(row['author_key'], key(row['cleaned']))].append(row)
for group_key, members in groups.items():
    for row in by_title.get(group_key, []):
        if row['id'] not in members and row['marker'] is None:
            members[row['id']] = (row, row['cleaned'], None, 'unnumbered_base_title', '')

def book(entry):
    row, base, volume, method, suffix = entry
    return {
        'book_id': row['id'], 'title': row['title'], 'author': row['author'],
        'proposed_volume': volume, 'match_method': method,
        'subtitle_after_volume': suffix or None,
        'existing_series': row['existing_series'] or None,
        'existing_volume': row['existing_volume'] or None,
        'catalog_status': row['extraction_status'],
    }

def sort_volume(entry):
    value = entry[2]
    if value is None:
        return (0, 0, entry[0]['id'])
    if re.fullmatch(r'\d+(?:\.\d)?', value):
        return (1, float(value), entry[0]['id'])
    return (2, value, entry[0]['id'])

output = []
assigned = set()
for (author, base_key), members in groups.items():
    entries = list(members.values())
    volumes = {entry[2] for entry in entries if entry[2] is not None}
    has_unnumbered = any(entry[2] is None for entry in entries)
    if len(entries) < 2 or (len(volumes) < 2 and not (volumes and has_unnumbered)):
        continue
    caution = []
    if len(volumes) < 2:
        caution.append('One numbered volume matches an unnumbered base title; the unnumbered volume is not assumed to be volume 1.')
    if any(re.search(r'合本|分冊|新装|改訂|完全版|新版|特装|短編|画集|コミック|漫画', entry[0]['title']) for entry in entries):
        caution.append('Edition, omnibus, short-story or manga labels occur; review series scope.')
    existing = {key(entry[0]['existing_series']) for entry in entries if entry[0]['existing_series']}
    if len(existing) > 1:
        caution.append('Existing catalog series labels disagree.')
    counts = collections.Counter(entry[2] for entry in entries if entry[2] is not None)
    if any(count > 1 for count in counts.values()):
        caution.append('Several records share a volume marker; copies/editions are retained separately.')
    bases = collections.Counter(entry[1] for entry in entries)
    output.append({
        'proposed_series': bases.most_common(1)[0][0],
        'authors': sorted({entry[0]['author'] for entry in entries}),
        'confidence': 'medium' if caution else 'high',
        'reason': 'Same normalized author and title prefix with distinct volume markers or an exact unnumbered base title.',
        'review_notes': caution,
        'books': [book(entry) for entry in sorted(entries, key=sort_volume)],
    })
    assigned.update(members)

# Weaker subtitle families: a shared substantial title before a subtitle delimiter.
possible = collections.defaultdict(dict)
for row in prepared:
    if not row['author_key'] or row['id'] in assigned:
        continue
    title = row['cleaned']
    split = re.search(r'\s+[~〜―—]|[~〜]|\s+[-:]\s+|\s+[「『]', title)
    if split:
        prefix = title[:split.start()].strip()
        if len(key(prefix)) >= 6:
            possible[(row['author_key'], key(prefix))][row['id']] = (row, prefix, None, 'shared_subtitle_prefix', title[split.end():])
for group_key, members in possible.items():
    for row in by_title.get(group_key, []):
        if row['id'] not in assigned:
            members[row['id']] = (row, row['cleaned'], None, 'unnumbered_base_title', '')
    entries = list(members.values())
    if len({key(entry[0]['cleaned']) for entry in entries}) < 2:
        continue
    output.append({
        'proposed_series': entries[0][1],
        'authors': sorted({entry[0]['author'] for entry in entries}),
        'confidence': 'possible',
        'reason': 'Same normalized author and substantial title prefix before differing subtitles; no reliable volume sequence.',
        'review_notes': ['May be editions, related works or a title family rather than a series. Review all members.'],
        'books': [book(entry) for entry in sorted(entries, key=lambda entry: entry[0]['title'])],
    })
    assigned.update(members)

output.sort(key=lambda group: ({'high': 0, 'medium': 1, 'possible': 2}[group['confidence']], group['proposed_series']))
for index, group in enumerate(output, 1):
    group['proposal_id'] = f'series-{index:04d}'
confidence_counts = dict(collections.Counter(group['confidence'] for group in output))
result = {
    'schema_version': 1,
    'generated_at': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'source': 'Read-only metadata snapshot of com.tmw.epublibrary/catalog.sqlite3',
    'purpose': 'Review-only inferred series proposals; no catalog assignments applied.',
    'method': 'Unicode NFKC/HTML normalization; title patterns blocked by matching complete normalized author fields. Filenames and folders are not used.',
    'limitations': ['Confidence labels are heuristic, not probabilities.', 'Unmarked or differently titled sequels and differing author credits can be missed.', 'No web research or EPUB content analysis performed.', 'Book IDs are specific to this catalog; duplicate copies are not removed.', 'Missing-author books are excluded; unmatched books are not asserted to be standalone.'],
    'summary': {'catalog_books': len(rows), 'proposed_groups': len(output), 'books_in_proposals': len(assigned), 'books_without_proposal': len(rows)-len(assigned), 'groups_by_confidence': confidence_counts},
    'series': output,
}
Path('book-series-proposals.json').write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf-8')
lines = ['BOOK SERIES PROPOSALS', json.dumps(result['summary'],ensure_ascii=False), 'Review only. No SQLite or EPUB changes.', '']
for group in output:
    lines.append(f"{group['proposal_id']} [{group['confidence']}] {group['proposed_series']}")
    for row in group['books']:
        lines.append(f"  [{row['book_id']}] {row['title']} | {row['author']} | volume {row['proposed_volume'] or '?'}")
    lines.extend('  NOTE: '+note for note in group['review_notes'])
    lines.append('')
Path('book-series-proposals.txt').write_text('\n'.join(lines),encoding='utf-8')
print(json.dumps(result['summary']))
