"""Run the native SeriesCatalog SQL template on isolated in-memory fixtures.

Reads the Kotlin query itself, not a second implementation. Android compilation
and physical-device behavior are separate acceptance checks.
"""
import json
import re
import sqlite3
from pathlib import Path

root = Path(__file__).resolve().parents[1]
source = (root / 'apps/android/src-tauri/gen/android/app/src/main/java/com/tmw/companion/SeriesCatalog.kt').read_text(encoding='utf-8')
templates = re.findall(r'return (?:"""(.*?)"""|"(WITH filtered[^"\n]*)")', source, re.S)
plain = next(short for long,short in templates if short)
template = next(long for long,short in templates if long)

def query(db, group, collapse=True, selected=False, offset=0):
    where="c.ns=? AND c.gen=(SELECT v FROM settings WHERE k='generation-'||c.ns)"
    if selected:
        where+=" AND c.seriesKey='series-a' AND c.seriesConfidence<=2"
    filtered=f"SELECT c.*,d.version AS downloadVersion,d.file AS downloadFile,d.bytes AS downloadBytes,{group} AS groupKey,c.seriesTitle AS groupTitle FROM catalog c LEFT JOIN downloads d ON d.ns=c.ns AND d.id=c.id WHERE {where}"
    sql=template if collapse else plain
    sort='CASE WHEN g.groupCount>1 THEN 0 ELSE 1 END,f.title' if collapse else 'f.title'
    for name,value in {'filtered':filtered,'sort':sort}.items():
        sql=sql.replace('$'+name,value)
    assert '$' not in sql
    assert ' OVER ' not in sql.upper(), 'API 24 query must not require SQLite window functions'
    return db.execute(sql,('pc',offset)).fetchall()

db=sqlite3.connect(':memory:')
db.executescript('''CREATE TABLE settings(k TEXT PRIMARY KEY,v TEXT); INSERT INTO settings VALUES('generation-pc','live');
CREATE TABLE catalog(ns TEXT,gen TEXT,id TEXT,json TEXT,title TEXT,creator TEXT,series TEXT,search TEXT,available INTEGER,deleted INTEGER,modified INTEGER,dateAdded INTEGER,PRIMARY KEY(ns,gen,id));
CREATE TABLE downloads(ns TEXT,id TEXT,version TEXT,file TEXT,bytes INTEGER,PRIMARY KEY(ns,id));''')
# Additive native column declarations work twice and preserve existing rows.
for _ in range(2):
    existing={row[1] for row in db.execute('PRAGMA table_info(catalog)')}
    for name,definition in re.findall(r'"(seriesKey|seriesTitle|seriesConfidence|seriesOrder|copyKey)" to "([^"]+)"',source):
        if name not in existing:db.execute(f'ALTER TABLE catalog ADD COLUMN {name} {definition}')

def add(identifier,group='series-a',confidence=2,copy=None,gen='live',ns='pc',available=1):
    row={'id':identifier,'title':identifier}
    db.execute('INSERT INTO catalog VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)',(ns,gen,identifier,json.dumps(row),identifier,'Author','','',available,0,0,0,group,'Series A',confidence,0,copy or identifier))

for index in range(30):add(f'volume-{index:02}')
for index in range(26):add(f'individual-{index:02}',group='',confidence=0)
add('staged-book',gen='stage')
add('other-namespace',ns='other')
group="CASE WHEN c.seriesKey<>'' AND c.seriesConfidence BETWEEN 1 AND 2 THEN c.seriesKey ELSE 'book:'||c.id END"
first=query(db,group)
second=query(db,group,offset=25)
assert len(first)==25 and len(second)==2
assert first[0][8]==30
assert len({row[6] for row in first+second})==27
assert len(query(db,group,offset=50))==0
assert query(db,group,collapse=False,selected=True)[0][8]==1
high="CASE WHEN c.seriesKey<>'' AND c.seriesConfidence BETWEEN 1 AND 1 THEN c.seriesKey ELSE 'book:'||c.id END"
assert all(row[8]==1 for row in query(db,high))
# Copy expansion includes all matching copies, preferring downloaded ones.
add('copy-unavailable',copy='copies',available=0)
add('copy-downloaded',copy='copies')
db.execute("INSERT INTO downloads VALUES('pc','copy-downloaded','sha256-fixture','fixture.epub',10)")
copies=query(db,'c.copyKey',selected=True)
assert next(row for row in copies if row[6]=='copies')[3]=='sha256-fixture'
assert next(row for row in copies if row[6]=='copies')[8]==2
# Changing only the active generation publishes membership atomically.
db.execute("UPDATE settings SET v='stage' WHERE k='generation-pc'")
published=query(db,group)
assert len(published)==1 and json.loads(published[0][0])['id']=='staged-book'
print('Native SQL: additive schema, confidence, pre-pagination grouping, copies, namespaces and generation publication passed.')

# A realistic full catalog must not execute a correlated full scan per row.
import time

db.execute("UPDATE settings SET v='live' WHERE k='generation-pc'")
for index in range(17412):
    add(f'large-{index:05}',group=f'large-series-{index // 12}',confidence=1)
started=time.monotonic()
large=query(db,group)
elapsed=time.monotonic()-started
assert len(large)==25
assert elapsed < 5, f'Full catalog grouping took {elapsed:.2f}s'
print(f'17,412 additional fixture books: grouped page in {elapsed:.3f}s.')
