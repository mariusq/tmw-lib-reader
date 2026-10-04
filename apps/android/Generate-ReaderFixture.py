"""Generate an original, source-library-independent EPUB for emulator testing."""
from pathlib import Path
from zipfile import ZipFile, ZIP_STORED

destination = Path(__file__).parent / ".test-results" / "reader-proof.epub"
destination.parent.mkdir(exist_ok=True)
with ZipFile(destination, "w") as epub:
    epub.writestr("mimetype", "application/epub+zip", compress_type=ZIP_STORED)
    epub.writestr("META-INF/container.xml", '''<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OPS/package.opf" media-type="application/oebps-package+xml"/></rootfiles></container>''')
    epub.writestr("OPS/package.opf", '''<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">tmw-original-reader-proof-v1</dc:identifier><dc:title>日本語 Reader Proof</dc:title><dc:language>ja</dc:language><meta property="dcterms:modified">2026-10-04T00:00:00Z</meta></metadata><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="horizontal" href="horizontal.xhtml" media-type="application/xhtml+xml"/><item id="vertical" href="vertical.xhtml" media-type="application/xhtml+xml"/></manifest><spine page-progression-direction="rtl"><itemref idref="horizontal"/><itemref idref="vertical"/></spine></package>''')
    epub.writestr("OPS/nav.xhtml", '''<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>Contents</title></head><body><nav epub:type="toc"><ol><li><a href="horizontal.xhtml">Horizontal</a></li><li><a href="vertical.xhtml">Vertical</a></li></ol></nav></body></html>''')
    for direction in ("horizontal", "vertical"):
        style = "writing-mode:vertical-rl;" if direction == "vertical" else ""
        epub.writestr(f"OPS/{direction}.xhtml", f'''<html xmlns="http://www.w3.org/1999/xhtml" lang="ja"><head><title>{direction}</title><style>html,body{{font-size:24px;line-height:2;direction:ltr;{style}}}</style></head><body><h1>{direction}</h1><p>😀 <ruby>猫<rt>ねこ</rt></ruby>を食べました。</p><p><span>食</span><em>べ</em><span>ました</span>。</p><p><ruby>日本語<rt>にほんご</rt></ruby>の本です。</p><p>学校に行きました。</p></body></html>''')
print(destination.resolve())

# Separate publications express coherent page-progression direction. Keep the
# mixed-direction EPUB as an additional epub.js stress fixture.
for direction in ("horizontal", "vertical"):
    target = destination.with_name(f"reader-{direction}.epub")
    with ZipFile(destination) as original, ZipFile(target, "w") as epub:
        for name in original.namelist():
            data = original.read(name)
            if name == "OPS/package.opf":
                package = data.decode()
                package = package.replace('<itemref idref="horizontal"/><itemref idref="vertical"/>', f'<itemref idref="{direction}"/>')
                if direction == "horizontal":
                    package = package.replace('page-progression-direction="rtl"', 'page-progression-direction="ltr"')
                package = package.replace("tmw-original-reader-proof-v1", f"tmw-original-reader-{direction}-v1")
                data = package.encode()
            epub.writestr(name, data, compress_type=ZIP_STORED)
    print(target.resolve())


