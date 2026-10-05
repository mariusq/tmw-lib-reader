import json, sqlite3
from pathlib import Path

source = Path(r'C:\Users\Marius\AppData\Roaming\com.tmw.epublibrary\catalog.sqlite3')
connection = sqlite3.connect(source.as_uri() + '?mode=ro', uri=True)
connection.execute('PRAGMA query_only=ON')
connection.row_factory = sqlite3.Row
rows = connection.execute("""
SELECT b.id, COALESCE(NULLIF(o.title,''),NULLIF(b.discovered_title,''),b.file_name) title,
COALESCE(NULLIF(o.creator,''),b.discovered_creator,'') author,
COALESCE(NULLIF(o.series_name,''),b.discovered_series,'') existing_series,
COALESCE(NULLIF(o.volume_label,''),b.discovered_series_index,'') existing_volume,
b.extraction_status
FROM books b LEFT JOIN book_overrides o ON o.book_id=b.id ORDER BY b.id
""").fetchall()
Path('series-analysis-catalog.json').write_text(json.dumps([dict(row) for row in rows],ensure_ascii=False,indent=2),encoding='utf-8')
connection.close()
print(f'Exported {len(rows)} metadata rows; SQLite connection was read-only.')
