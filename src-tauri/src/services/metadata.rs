use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use image::{codecs::jpeg::JpegEncoder, DynamicImage, RgbImage};
use quick_xml::{events::Event, Reader};
use unicode_normalization::UnicodeNormalization;
use zip::ZipArchive;

use crate::models::book::ExtractedBookMetadata;
use crate::services::performance::{self, Stage};

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
    match performance::measure(Stage::EpubExtraction, || {
        extract_epub_inner(path, book_id, cache_directory)
    }) {
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
        match performance::measure(Stage::CoverExtraction, || {
            extract_cover(
                &mut archive,
                &entry_name,
                media_type.as_deref(),
                cache_directory,
                book_id,
            )
        }) {
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
            .and_then(|element| {
                element
                    .attributes
                    .get("src")
                    .or_else(|| element.attributes.get("href"))
                    .cloned()
            });
        if let Some(image_href) = image_href {
            let image_name = resolve_archive_path(&page_name, &image_href);
            match performance::measure(Stage::CoverExtraction, || {
                extract_cover(&mut archive, &image_name, None, cache_directory, book_id)
            }) {
                Ok(cover) => metadata.cover_path = Some(cover.to_string_lossy().into_owned()),
                Err(error) => {
                    metadata.extraction_error = Some(format!(
                        "Metadata read succeeded, but guide cover extraction failed: {error}"
                    ))
                }
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
            elements
                .iter()
                .find(|element| {
                    element.name == "item"
                        && element
                            .attributes
                            .get("media-type")
                            .is_some_and(|type_| type_.starts_with("image/"))
                        && (element
                            .attributes
                            .get("id")
                            .is_some_and(|id| id.to_ascii_lowercase().contains("cover"))
                            || element
                                .attributes
                                .get("href")
                                .is_some_and(|href| href.to_ascii_lowercase().contains("cover")))
                })
                .and_then(|element| {
                    element
                        .attributes
                        .get("href")
                        .cloned()
                        .map(|href| (href, element.attributes.get("media-type").cloned()))
                })
        })
}

fn guide_cover_page(elements: &[XmlElement]) -> Option<String> {
    elements
        .iter()
        .find(|element| {
            element.name == "reference"
                && element
                    .attributes
                    .get("type")
                    .is_some_and(|type_| type_.eq_ignore_ascii_case("cover"))
        })
        .and_then(|element| element.attributes.get("href").cloned())
}

fn resolve_archive_path(opf: &str, href: &str) -> String {
    let base = Path::new(opf).parent().unwrap_or_else(|| Path::new(""));
    let mut parts = Vec::new();
    let archive_href = href.split(['#', '?']).next().unwrap_or_default();
    let joined = base.join(archive_href).to_string_lossy().replace('\\', "/");
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
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
    let mut bytes = Vec::with_capacity(image.size().min(8 * 1024 * 1024) as usize);
    image
        .by_ref()
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err("Cover image exceeds the 64 MiB decode limit.".into());
    }

    let decoded = image::load_from_memory(&bytes).map_err(|error| {
        format!(
            "Could not decode cover image{}: {error}",
            media_type
                .map(|kind| format!(" ({kind})"))
                .unwrap_or_default()
        )
    })?;
    let thumbnail = decoded.thumbnail(320, 480);
    let rgb = flatten_to_rgb(&thumbnail);
    let output = cache.join(format!("book-{book_id}.jpg"));
    write_jpeg_atomically(&rgb, &output)?;
    // Phase 5 standardized the regenerable cache on JPEG. Remove only known
    // legacy cache names for this book, never arbitrary files or source data.
    for extension in ["jpeg", "png", "gif", "webp", "img"] {
        let legacy = cache.join(format!("book-{book_id}.{extension}"));
        if legacy != output {
            match fs::remove_file(&legacy) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(format!("Could not remove legacy cover cache file: {error}"))
                }
            }
        }
    }
    Ok(output)
}

fn flatten_to_rgb(image: &DynamicImage) -> RgbImage {
    let rgba = image.to_rgba8();
    RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let pixel = rgba.get_pixel(x, y).0;
        let alpha = u16::from(pixel[3]);
        image::Rgb([
            ((u16::from(pixel[0]) * alpha + 255 * (255 - alpha)) / 255) as u8,
            ((u16::from(pixel[1]) * alpha + 255 * (255 - alpha)) / 255) as u8,
            ((u16::from(pixel[2]) * alpha + 255 * (255 - alpha)) / 255) as u8,
        ])
    })
}

fn write_jpeg_atomically(image: &RgbImage, output: &Path) -> Result<(), String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let temporary = output.with_extension(format!("jpg.tmp-{}-{nonce}", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        JpegEncoder::new_with_quality(&mut file, 82)
            .encode_image(image)
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        drop(file);
        if output.exists() {
            fs::remove_file(output).map_err(|error| error.to_string())?;
        }
        fs::rename(&temporary, output).map_err(|error| error.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{codecs::png::PngEncoder, ColorType, GenericImageView, ImageEncoder};
    use std::io::Write;
    use std::time::Instant;
    use tempfile::tempdir;
    use zip::{write::SimpleFileOptions, ZipWriter};

    fn fixture_png(width: u32, height: u32) -> Vec<u8> {
        let pixels: Vec<u8> = (0..width * height)
            .flat_map(|index| {
                let x = index % width;
                let y = index / width;
                let noise =
                    index.wrapping_mul(2_654_435_761) ^ x.rotate_left(7) ^ y.rotate_left(13);
                [noise as u8, (noise >> 8) as u8, (noise >> 16) as u8, 220]
            })
            .collect();
        let mut bytes = Vec::new();
        PngEncoder::new(&mut bytes)
            .write_image(&pixels, width, height, ColorType::Rgba8.into())
            .unwrap();
        bytes
    }

    #[test]
    #[ignore = "performance baseline; run explicitly with --ignored --nocapture"]
    fn performance_baseline_representative_epub_fixture_set() {
        const BOOKS: i64 = 100;
        let directory = tempdir().unwrap();
        let epub_path = directory.path().join("representative.epub");
        let mut zip = ZipWriter::new(File::create(&epub_path).unwrap());
        let options = SimpleFileOptions::default();
        zip.start_file("META-INF/container.xml", options).unwrap();
        zip.write_all(br#"<container><rootfiles><rootfile full-path="OPS/book.opf"/></rootfiles></container>"#).unwrap();
        zip.start_file("OPS/book.opf", options).unwrap();
        zip.write_all(r#"<package><metadata><title>進撃の巨人 １巻</title><creator>諫山創</creator><language>ja</language><meta name="calibre:series" content="進撃の巨人"/><meta name="cover" content="cover"/></metadata><manifest><item id="cover" href="cover.png" media-type="image/png"/></manifest></package>"#.as_bytes()).unwrap();
        zip.start_file("OPS/cover.png", options).unwrap();
        let source_cover = fixture_png(600, 900);
        zip.write_all(&source_cover).unwrap();
        zip.finish().unwrap();

        performance::start();
        let started = Instant::now();
        for book_id in 1..=BOOKS {
            let extracted = extract_epub(&epub_path, book_id, &directory.path().join("covers"));
            assert!(extracted.extraction_error.is_none());
        }
        let total = started.elapsed();
        let timings = performance::finish();
        let cached_bytes: u64 = (1..=BOOKS)
            .map(|book_id| {
                fs::metadata(directory.path().join(format!("covers/book-{book_id}.jpg")))
                    .unwrap()
                    .len()
            })
            .sum();
        println!("PERF_EPUB_BASELINE books={BOOKS} total_ms={:.3} metadata_parse_ms={} cover_extract_encode_ms={} throughput_books_per_second={:.2} source_cover_bytes={} average_thumbnail_bytes={:.1} size_reduction_percent={:.1}",
            total.as_secs_f64() * 1000.0,
            performance::millis(timings.metadata_parsing()),
            performance::millis(timings.cover_extraction),
            BOOKS as f64 / total.as_secs_f64(),
            source_cover.len(),
            cached_bytes as f64 / BOOKS as f64,
            (1.0 - cached_bytes as f64 / BOOKS as f64 / source_cover.len() as f64) * 100.0,
        );
    }

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
        zip.write_all(&fixture_png(800, 1200)).unwrap();
        zip.finish().unwrap();
        let before = fs::read(&epub_path).unwrap();
        let cache = directory.path().join("cover-cache");
        let metadata = extract_epub(&epub_path, 42, &cache);
        assert_eq!(metadata.title.as_deref(), Some("日本語 １巻"));
        assert_eq!(metadata.creator.as_deref(), Some("著者"));
        assert_eq!(metadata.series.as_deref(), Some("シリーズ"));
        let thumbnail = cache.join("book-42.jpg");
        assert!(thumbnail.is_file());
        let decoded = image::open(thumbnail).unwrap();
        assert_eq!(decoded.dimensions(), (320, 480));
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
        zip.write_all(&fixture_png(240, 360)).unwrap();
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
        zip.write_all(br#"<html><body><img src="../images/front.webp"/></body></html>"#)
            .unwrap();
        zip.start_file("OPS/images/front.webp", options).unwrap();
        zip.write_all(&fixture_png(160, 240)).unwrap();
        zip.finish().unwrap();
        let metadata = extract_epub(&epub_path, 99, &directory.path().join("cache"));
        assert!(metadata.cover_path.is_some());
        assert!(directory.path().join("cache/book-99.jpg").is_file());
    }

    #[test]
    fn invalid_cover_is_a_per_book_error_without_partial_cache_file() {
        let directory = tempdir().unwrap();
        let epub_path = directory.path().join("bad-cover.epub");
        let mut zip = ZipWriter::new(File::create(&epub_path).unwrap());
        let options = SimpleFileOptions::default();
        zip.start_file("META-INF/container.xml", options).unwrap();
        zip.write_all(
            br#"<container><rootfiles><rootfile full-path="book.opf"/></rootfiles></container>"#,
        )
        .unwrap();
        zip.start_file("book.opf", options).unwrap();
        zip.write_all(br#"<package><metadata><title>Valid metadata</title></metadata><manifest><item id="cover" href="cover.png" media-type="image/png" properties="cover-image"/></manifest></package>"#).unwrap();
        zip.start_file("cover.png", options).unwrap();
        zip.write_all(b"not an image").unwrap();
        zip.finish().unwrap();

        let cache = directory.path().join("cache");
        let metadata = extract_epub(&epub_path, 12, &cache);
        assert_eq!(metadata.title.as_deref(), Some("Valid metadata"));
        assert!(metadata
            .extraction_error
            .as_deref()
            .is_some_and(|error| error.contains("decode")));
        assert!(metadata.cover_path.is_none());
        assert!(!cache.join("book-12.jpg").exists());
        assert!(fs::read_dir(cache).unwrap().next().is_none());
    }

    #[test]
    fn failed_cache_write_preserves_the_previous_thumbnail_and_cleans_temporary_files() {
        let directory = tempdir().unwrap();
        let output = directory.path().join("book-5.jpg");
        let previous = b"previous-valid-cache-entry";
        fs::write(&output, previous).unwrap();

        // A zero-sized image fails during encoding, before the atomic replace.
        let invalid = RgbImage::new(0, 0);
        assert!(write_jpeg_atomically(&invalid, &output).is_err());
        assert_eq!(fs::read(&output).unwrap(), previous);
        assert_eq!(
            fs::read_dir(directory.path()).unwrap().count(),
            1,
            "an interrupted cache write must not leave a temp file"
        );
    }
}
