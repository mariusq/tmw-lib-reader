"""Repeatable read-only index/provenance checks; no source EPUBs are used."""
import hashlib
import re
import sqlite3
import time
import unicodedata
from pathlib import Path

root = Path(__file__).resolve().parents[2]
source = (root / "crates/japanese-core/src/dictionary.rs").read_text(encoding="utf-8")
predicate = re.search(r'pub const LOOKUP_PREDICATE:\s*&str\s*=\s*"(.*?)";', source, re.S)[1]
path = root / "apps/android/src-tauri/assets/jmdict-20260928-v2.sqlite3"
db = sqlite3.connect(f"file:{path.as_posix()}?mode=ro", uri=True)
assert db.execute("PRAGMA quick_check").fetchone()[0] == "ok"
assert db.execute("SELECT count(*) FROM entries").fetchone()[0] == 499285
db.executescript("CREATE TEMP VIEW dictionary_entries AS SELECT *,1 AS dictionary_id FROM entries; CREATE TEMP VIEW dictionaries AS SELECT 1 AS id,1 AS enabled;")
mobile = "SELECT e.term,e.reading,e.definitions FROM entries e WHERE " + predicate
desktop = "SELECT e.term,e.reading,e.definitions FROM dictionary_entries e JOIN dictionaries d ON d.id=e.dictionary_id WHERE d.enabled=1 AND " + predicate
for query in ("猫", "食べ", "食べる", "日本語", "学校", "ねこ", "ﾈｺ", "不存在語"):
    normalized = " ".join(unicodedata.normalize("NFKC", query).split())
    started = time.perf_counter()
    results = db.execute(mobile, (normalized, normalized + "%")).fetchall()
    elapsed = (time.perf_counter()-started)*1000
    assert results == db.execute(desktop, (normalized, normalized + "%")).fetchall()
    print(query, f"{elapsed:.2f} ms", [row[0] for row in results[:3]])
print("Query plan:", db.execute("EXPLAIN QUERY PLAN " + mobile, ("猫", "猫%")).fetchall())
for asset in (path, path.with_suffix(path.suffix + ".gz")):
    print(asset.name, asset.stat().st_size, hashlib.sha256(asset.read_bytes()).hexdigest())

