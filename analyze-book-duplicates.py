import json,re,html,unicodedata,collections,difflib,itertools
from pathlib import Path
rows=json.loads(Path('duplicate-analysis-catalog.json').read_text(encoding='utf-8'))
def normalize(s):
 for _ in range(3):s=html.unescape(s or '')
 return unicodedata.normalize('NFKC',s).casefold()
def compact(s):return ''.join(c for c in normalize(s) if c.isalnum())
def clean(s):
 s=normalize(s)
 s=re.sub(r'^\._','',s)
 s=re.sub(r'[\(（][^()（）]*(?:文庫|ブックス|books|ノベルス|novels|コミックス|コミック|出版|レーベル)[^()（）]*[\)）]','',s)
 s=re.sub(r'[【\[][^】\]]*(?:電子|特典|限定)[^】\]]*[】\]]','',s)
 return compact(s)
def nums(s):
 s=normalize(s)
 digits=tuple(re.findall(r'\d+',s))
 roman=tuple(re.findall(r'(?<![a-z])[ivxlcdm]+(?![a-z])',s))
 japanese=tuple(re.findall(r'[〇零一二三四五六七八九十百千万壱弐参]+',s))
 parts=tuple(re.findall(r'[上中下](?=[>〉》」』】)）\s]|$)|(?<=[<〈《「『【(（])[上中下]',s))
 parts += tuple(re.findall(r'前編|後編|前章|後章|第[^「『【(（ ]{1,5}部',s))
 return (digits,roman,japanese,parts)

def authors(s):return set(filter(None,(compact(x) for x in re.split(r'[,、;；/&]|著者:',s or ''))))
def variant(s):return bool(re.search(r'合本|分冊|新装|改訂|完全版|新版|増補|愛蔵|特装|短編|ショートストーリー|購入特典|限定特典|画集|コミック|漫画',normalize(s)))
for r in rows:
 r['key']=clean(r['title']);r['numbers']=nums(normalize(r['title']));r['authors']=authors(r['author']);r['variant']=variant(r['title'])
pairs={}
def add(i,j,level,reason):
 a,b=rows[i],rows[j]
 if a['title']==b['title']:return
 distinctions=r'初中級|初級|中級|上級|sss|改(?=\s*\d)|新装版|改訂版|完全版|合本|分冊'
 if re.findall(distinctions,normalize(a['title']))!=re.findall(distinctions,normalize(b['title'])):return
 if a['numbers']!=b['numbers']:return
 if a['volume'] and b['volume'] and compact(a['volume'])!=compact(b['volume']):return
 same_author=bool(a['authors'] & b['authors'])
 if a['authors'] and b['authors'] and not same_author:return
 if a['variant']!=b['variant']:return
 if level=='HIGH' and (a['variant'] or b['variant'] or not same_author):level='POSSIBLE';reason+='; edition/bonus content or author needs review'
 k=tuple(sorted((i,j)))
 if k not in pairs or level=='HIGH':pairs[k]=(level,reason)
keys=collections.defaultdict(list);ids=collections.defaultdict(list)
for i,r in enumerate(rows):
 if len(r['key'])>=3:keys[r['key']].append(i)
 ident=normalize(r['discovered_identifier']).strip()
 if re.fullmatch(r'b0[a-z0-9]{8}',ident) or re.fullmatch(r'(?:urn:isbn:|isbn:)?(?:97[89])[-\d]{10,16}',ident):ids[ident].append(i)
for group in keys.values():
 for i,j in itertools.combinations(group,2):add(i,j,'HIGH','Same title after Unicode/HTML, punctuation and publisher/bonus-label normalization')
for ident,group in ids.items():
 for i,j in itertools.combinations(group,2):
  a,b=rows[i]['key'],rows[j]['key']
  if difflib.SequenceMatcher(None,a,b).ratio()>=.65 or (min(len(a),len(b))>=5 and (a in b or b in a)):
   add(i,j,'HIGH','Same ASIN/ISBN and compatible title, author and volume')
# Trigram candidates are bounded by author and numeric sequence, preserving volume identity.
blocks=collections.defaultdict(list)
for i,r in enumerate(rows):
 for author in r['authors']:blocks[(author,r['numbers'])].append(i)
for group in blocks.values():
 if len(group)>450:continue
 for i,j in itertools.combinations(group,2):
  if (i,j) in pairs:continue
  a,b=rows[i]['key'],rows[j]['key']
  if min(len(a),len(b))<5 or a==b:continue
  if abs(len(a)-len(b))>max(len(a),len(b))*.45:continue
  sim=difflib.SequenceMatcher(None,a,b,autojunk=False).ratio()
  if sim>=.96 and a[:8]==b[:8] and not (a in b or b in a):
   add(i,j,'POSSIBLE',f'Same author and numeric/volume markers; similar title ({sim:.0%}); subtitle or edition may differ')
# Groups contain only edges at their stated confidence; retain evidence for every pair.
report=['DIFFERENTLY NAMED DUPLICATE CANDIDATES','Based on a read-only catalog snapshot; no books or catalog entries changed.','Exact-title pairs excluded. Different numeric/volume markers and conflicting authors excluded.','HIGH: strong metadata match; verify which copy to hide. POSSIBLE: review before filtering.','No content hashes exist in this catalog. These are metadata candidates, not proven identical EPUBs.','Calibre IDs, UUIDs and generic publisher IDs are not used as duplicate proof.','Arabic, Roman and Japanese numeral sequences plus upper/middle/lower part markers must match; alternative volume notation may be missed.','Translations and unrelated alternative titles may be missed without a shared reliable identifier.','No automatic hide/remove decisions have been applied.','']
export=[]
for level in ['HIGH','POSSIBLE']:
 edges={k:v for k,v in pairs.items() if v[0]==level};adj=collections.defaultdict(set)
 for i,j in edges:adj[i].add(j);adj[j].add(i)
 seen=set();groups=[]
 for start in sorted(adj,key=lambda i:rows[i]['title']):
  if start in seen:continue
  todo=[start];group=set()
  while todo:
   x=todo.pop()
   if x in group:continue
   group.add(x);todo.extend(adj[x]-group)
  seen|=group;groups.append(sorted(group,key=lambda i:rows[i]['title']))
 report.append(f'{level} CONFIDENCE: {len(groups)} groups, {len(edges)} differently named matching pairs')
 report.append('Groups can contain transitive matches; pair evidence below identifies the actual comparisons.')
 for n,group in enumerate(groups,1):
  report.extend(['',f'{level} {n:04d}'])
  for i in group:
   r=rows[i];report.extend([f"  [{r['id']}] {r['title']}",f"    Author: {r['author'] or '(missing)'} | Volume: {r['volume'] or '(unspecified)'} | Status: {r['extraction_status']}",f"    File: {r['file_path']}"])
  evidence=[]
  for i,j in itertools.combinations(sorted(group),2):
   if (i,j) in edges:
    reason=edges[i,j][1];report.append(f"    Match {rows[i]['id']} <-> {rows[j]['id']}: {reason}");evidence.append({'ids':[rows[i]['id'],rows[j]['id']],'reason':reason})
  export.append({'confidence':level,'group':n,'books':[{k:rows[i][k] for k in ['id','title','author','volume','file_path','extraction_status']} for i in group],'matches':evidence})
 report.append('')
 print(level,len(groups),'groups',len(edges),'pairs')
Path('duplicate-book-candidates.txt').write_text('\n'.join(report),encoding='utf-8-sig')
Path('duplicate-book-candidates.json').write_text(json.dumps(export,ensure_ascii=False,indent=2),encoding='utf-8')
