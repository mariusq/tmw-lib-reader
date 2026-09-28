use std::{
    collections::HashMap,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

use quick_xml::{events::Event, Reader};
use unicode_normalization::UnicodeNormalization;
use zip::ZipArchive;

use crate::models::book::ExtractedBookMetadata;

#[derive(Debug, Clone)]
struct XmlElement {
    name: String,
    attributes: HashMap<String, String>,
    text: String,
}

/// A matching-only normalization; callers must retain the original strings for display.
pub fn normalize_for_search(value: &str) -> String {
    value
        .nfkc()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn extract_epub(path: &Path, book_id: i64, cache_directory: &Path) -> ExtractedBookMetadata {
    match extract_epub_inner(path, book_id, cache_directory) {
        Ok(metadata) => metadata,
        Err(error) => ExtractedBookMetadata {
            title: fallback_title(path),
            extraction_error: Some(error),
            ..Default::default()
        },
    }
}

fn extract_epub_inner(
    path: &Path,
    book_id: i64,
    cache_directory: &Path,
) -> Result<ExtractedBookMetadata, String> {
    let file = File::open(path).map_err(|error| format!("Could not open EPUB: {error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("Invalid EPUB ZIP archive: {error}"))?;
    let container = read_zip_text(&mut archive, "META-INF/container.xml")
        .map_err(|error| format!("Could not read EPUB container.xml: {error}"))?;
    let rootfile = parse_xml(&container)?
        .into_iter()
        .find(|element| element.name == "rootfile")
        .and_then(|element| element.attributes.get("full-path").cloned())
        .ok_or_else(|| {
            "EPUB container.xml does not identify an OPF package document.".to_string()
        })?;
    let opf = read_zip_text(&mut archive, &rootfile)
        .map_err(|error| format!("Could not read OPF package document: {error}"))?;
    let elements = parse_xml(&opf)?;
    let mut metadata = ExtractedBookMetadata {
        title: first_text(&elements, "title").or_else(|| fallback_title(path)),
        creator: first_text(&elements, "creator"),
        language: first_text(&elements, "language"),
        identifier: first_text(&elements, "identifier"),
        ..Default::default()
    };
    for element in elements.iter().filter(|element| element.name == "meta") {
        let property = element.attributes.get("property").map(String::as_str);
        let name = element.attributes.get("name").map(String::as_str);
        let value = element
            .attributes
            .get("content")
            .cloned()
            .unwrap_or_else(|| element.text.clone());
        if matches!(property, Some("belongs-to-collection"))
            || matches!(name, Some("calibre:series"))
        {
            metadata.series = nonempty(value.clone());
        }
        if matches!(property, Some("group-position"))
            || matches!(name, Some("calibre:series_index"))
        {
            metadata.series_index = nonempty(value);
        }
    }
    if let Some((href, media_type)) = cover_item(&elements) {
        let entry_name = resolve_archive_path(&rootfile, &href);
        match extract_cover(
            &mut archive,
            &entry_name,
            media_type.as_deref(),
            cache_directory,
            book_id,
        ) {
            Ok(cover) => metadata.cover_path = Some(cover.to_string_lossy().into_owned()),
            Err(error) => {
                metadata.extraction_error = Some(format!(
                    "Metadata read succeeded, but cover extraction failed: {error}"
                ))
            }
        }
    } else if let Some(cover_page) = guide_cover_page(&elements) {
        // Older EPUB 2 files often only point to a cover XHTML page in the guide.
        // Follow that page and extract the image it embeds.
        let page_name = resolve_archive_path(&rootfile, &cover_page);
        let page = read_zip_text(&mut archive, &page_name)
            .map_err(|error| format!("Could not read cover guide page: {error}"))?;
        let image_href = parse_xml(&page)?
            .into_iter()
            .find(|element| matches!(element.name.as_str(), "img" | "image"))
            .and_then(|element| element.attributes.get("src").or_else(|| element.attributes.get("href")).cloned());
        if let Some(image_href) = image_href {
            let image_name = resolve_archive_path(&page_name, &image_href);
            match extract_cover(&mut archive, &image_name, None, cache_directory, book_id) {
                Ok(cover) => metadata.cover_path = Some(cover.to_string_lossy().into_owned()),
                Err(error) => metadata.extraction_error = Some(format!("Metadata read succeeded, but guide cover extraction failed: {error}")),
            }
        }
    }
    Ok(metadata)
}

fn parse_xml(xml: &str) -> Result<Vec<XmlElement>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut elements = Vec::new();
    let mut current: Option<XmlElement> = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                if let Some(element) = current.take() {
                    elements.push(element);
                }
                current = Some(XmlElement {
                    name: local_name(event.name().as_ref()),
                    attributes: attributes(&event)?,
                    text: String::new(),
                });
            }
            Ok(Event::Empty(event)) => elements.push(XmlElement {
                name: local_name(event.name().as_ref()),
                attributes: attributes(&event)?,
                text: String::new(),
            }),
            Ok(Event::Text(text)) => {
                if let Some(element) = current.as_mut() {
                    element
                        .text
                        .push_str(&String::from_utf8_lossy(text.as_ref()));
                }
            }
            Ok(Event::CData(text)) => {
                if let Some(element) = current.as_mut() {
                    element
                        .text
                        .push_str(&String::from_utf8_lossy(text.as_ref()));
                }
            }
            Ok(Event::End(_)) => {
                if let Some(element) = current.take() {
                    elements.push(element);
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(format!("Malformed XML: {error}")),
            _ => {}
        }
    }
    Ok(elements)
}

fn attributes(
    event: &quick_xml::events::BytesStart<'_>,
) -> Result<HashMap<String, String>, String> {
    event
        .attributes()
        .map(|attribute| {
            let attribute = attribute.map_err(|error| error.to_string())?;
            Ok((
                local_name(attribute.key.as_ref()),
                String::from_utf8_lossy(attribute.value.as_ref()).into_owned(),
            ))
        })
        .collect()
}

fn local_name(name: &[u8]) -> String {
    String::from_utf8_lossy(name)
        .rsplit(':')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn first_text(elements: &[XmlElement], name: &str) -> Option<String> {
    elements
        .iter()
        .find(|element| element.name == name)
        .and_then(|element| nonempty(element.text.clone()))
}
fn nonempty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.trim().to_owned())
}
fn fallback_title(path: &Path) -> Option<String> {
    path.file_stem()
        .map(|name| name.to_string_lossy().into_owned())
}

fn cover_item(elements: &[XmlElement]) -> Option<(String, Option<String>)> {
    let cover_id = elements
        .iter()
        .find(|e| {
            e.name == "meta"
                && e.attributes
                    .get("name")
                    .is_some_and(|n| n.eq_ignore_ascii_case("cover"))
        })
        .and_then(|e| e.attributes.get("content"));
    elements
        .iter()
        .find(|element| {
            element.name == "item"
                && (cover_id.is_some_and(|id| element.attributes.get("id") == Some(id))
                    || element
                        .attributes
                        .get("properties")
                        .is_some_and(|p| p.split_whitespace().any(|part| part == "cover-image")))
        })
        .and_then(|element| {
            element
                .attributes
                .get("href")
                .cloned()
                .map(|href| (href, element.attributes.get("media-type").cloned()))
        })
        .or_else(|| {
            // Some publishers omit the standard metadata but use conventional
            // manifest IDs or filenames. This is a fallback, never a guess at
            // arbitrary content images.
            elements.iter().find(|element| {
                element.name == "item" && element.attributes.get("media-type").is_some_and(|type_| type_.starts_with("image/")) && (
                    element.attributes.get("id").is_some_and(|id| id.to_ascii_lowercase().contains("cover")) ||
                    element.attributes.get("href").is_some_and(|href| href.to_ascii_lowercase().contains("cover"))
                )
            }).and_then(|element| element.attributes.get("href").cloned().map(|href| (href, element.attributes.get("media-type").cloned())))
        })
}

fn guide_cover_page(elements: &[XmlElement]) -> Option<String> {
    elements.iter().find(|element| {
        element.name == "reference" && element.attributes.get("type").is_some_and(|type_| type_.eq_ignore_ascii_case("cover"))
    }).and_then(|element| element.attributes.get("href").cloned())
}

fn resolve_archive_path(opf: &str, href: &str) -> String {
    let base = Path::new(opf).parent().unwrap_or_else(|| Path::new(""));
    let mut parts = Vec::new();
    let archive_href = href.split(['#', '?']).next().unwrap_or_default();
    let joined = base.join(archive_href).to_string_lossy().replace('\\', "/");
    for part in joined.split('/') {
        match part {
            "" | "." => {},
            ".." => { parts.pop(); },
            segment => parts.push(segment),
        }
    }
    parts.join("/")
}

fn read_zip_text(archive: &mut ZipArchive<File>, entry: &str) -> Result<String, String> {
    let mut file = archive.by_name(entry).map_err(|error| error.to_string())?;
    let mut text = String::new();
    file.read_to_string(&mut text)
        .map_err(|error| error.to_string())?;
    Ok(text)
}

fn extract_cover(
    archive: &mut ZipArchive<File>,
    entry: &str,
    media_type: Option<&str>,
    cache: &Path,
    book_id: i64,
) -> Result<PathBuf, String> {
    let mut image = archive.by_name(entry).map_err(|error| error.to_string())?;
    fs::create_dir_all(cache).map_err(|error| error.to_string())?;
    let extension = media_type
        .and_then(extension_for_media_type)
        .or_else(|| Path::new(entry).extension().and_then(|e| e.to_str()))
        .unwrap_or("img");
    let output = cache.join(format!("book-{book_id}.{extension}"));
    let mut destination = File::create(&output).map_err(|error| error.to_string())?;
    std::io::copy(&mut image, &mut destination).map_err(|error| error.to_string())?;
    Ok(output)
}
fn extension_for_media_type(media_type: &str) -> Option<&'static str> {
    match media_type {
        "image/jpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;
    use zip::{write::SimpleFileOptions, ZipWriter};

    #[test]
    fn normalizes_japanese_width_and_spacing() {
        assert_eq!(normalize_for_search("  第１巻　 テスト "), "第1巻 テスト");
    }

    #[test]
    fn extracts_epub2_metadata_and_cover_without_changing_source() {
        let directory = tempdir().unwrap();
        let epub_path = directory.path().join("fixture.epub");
        let output = File::create(&epub_path).unwrap();
        let mut zip = ZipWriter::new(output);
        let options = SimpleFileOptions::default();
        zip.start_file("META-INF/container.xml", options).unwrap();
        zip.write_all(
            r#"<container><rootfiles><rootfile full-path="OPS/book.opf"/></rootfiles></container>"#
                .as_bytes(),
        )
        .unwrap();
        zip.start_file("OPS/book.opf", options).unwrap();
        zip.write_all(r#"<package><metadata><dc:title xmlns:dc="x">日本語 １巻</dc:title><dc:creator xmlns:dc="x">著者</dc:creator><meta name="calibre:series" content="シリーズ"/><meta name="cover" content="cover"/></metadata><manifest><item id="cover" href="images/cover.png" media-type="image/png"/></manifest></package>"#.as_bytes()).unwrap();
        zip.start_file("OPS/images/cover.png", options).unwrap();
        zip.write_all(b"not-a-real-png-but-a-cover-fixture")
            .unwrap();
        zip.finish().unwrap();
        let before = fs::read(&epub_path).unwrap();
        let cache = directory.path().join("cover-cache");
        let metadata = extract_epub(&epub_path, 42, &cache);
        assert_eq!(metadata.title.as_deref(), Some("日本語 １巻"));
        assert_eq!(metadata.creator.as_deref(), Some("著者"));
        assert_eq!(metadata.series.as_deref(), Some("シリーズ"));
        assert!(cache.join("book-42.png").is_file());
        assert_eq!(fs::read(&epub_path).unwrap(), before);
    }

    #[test]
    fn malformed_epub_records_an_error_and_uses_filename_title() {
        let directory = tempdir().unwrap();
        let epub_path = directory.path().join("壊れた.epub");
        fs::write(&epub_path, b"not a zip").unwrap();
        let metadata = extract_epub(&epub_path, 1, &directory.path().join("cache"));
        assert_eq!(metadata.title.as_deref(), Some("壊れた"));
        assert!(metadata.extraction_error.is_some());
    }

    #[test]
    fn extracts_epub3_property_cover_and_collection_metadata() {
        let directory = tempdir().unwrap();
        let epub_path = directory.path().join("epub3.epub");
        let mut zip = ZipWriter::new(File::create(&epub_path).unwrap());
        let options = SimpleFileOptions::default();
        zip.start_file("META-INF/container.xml", options).unwrap();
        zip.write_all(
            r#"<container><rootfiles><rootfile full-path="book.opf"/></rootfiles></container>"#
                .as_bytes(),
        )
        .unwrap();
        zip.start_file("book.opf", options).unwrap();
        zip.write_all(r#"<package><metadata><title>EPUB 3</title><meta property="belongs-to-collection">Collection</meta><meta property="group-position">2</meta></metadata><manifest><item id="cover" href="cover.jpg" media-type="image/jpeg" properties="cover-image"/></manifest></package>"#.as_bytes()).unwrap();
        zip.start_file("cover.jpg", options).unwrap();
        zip.write_all(b"fixture").unwrap();
        zip.finish().unwrap();
        let metadata = extract_epub(&epub_path, 7, &directory.path().join("cache"));
        assert_eq!(metadata.title.as_deref(), Some("EPUB 3"));
        assert_eq!(metadata.series.as_deref(), Some("Collection"));
        assert_eq!(metadata.series_index.as_deref(), Some("2"));
        assert!(metadata
            .cover_path
            .as_deref()
            .is_some_and(|path| path.ends_with("book-7.jpg")));
    }

    #[test]
    fn extracts_a_cover_image_referenced_only_by_an_epub2_guide_page() {
        let directory = tempdir().unwrap();
        let epub_path = directory.path().join("guide-cover.epub");
        let mut zip = ZipWriter::new(File::create(&epub_path).unwrap());
        let options = SimpleFileOptions::default();
        zip.start_file("META-INF/container.xml", options).unwrap();
        zip.write_all(br#"<container><rootfiles><rootfile full-path="OPS/package.opf"/></rootfiles></container>"#).unwrap();
        zip.start_file("OPS/package.opf", options).unwrap();
        zip.write_all(br#"<package><metadata><title>Guide cover</title></metadata><manifest><item id="page" href="pages/cover.xhtml" media-type="application/xhtml+xml"/></manifest><guide><reference type="cover" href="pages/cover.xhtml#first"/></guide></package>"#).unwrap();
        zip.start_file("OPS/pages/cover.xhtml", options).unwrap();
        zip.write_all(br#"<html><body><img src="../images/front.webp"/></body></html>"#).unwrap();
        zip.start_file("OPS/images/front.webp", options).unwrap();
        zip.write_all(b"cover").unwrap();
        zip.finish().unwrap();
        let metadata = extract_epub(&epub_path, 99, &directory.path().join("cache"));
        assert!(metadata.cover_path.is_some());
        assert!(directory.path().join("cache/book-99.webp").is_file());
    }
}
