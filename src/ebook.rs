use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading(String),
    Para(String),
    Image(String),
}

pub struct Chapter {
    pub title: String,
    pub blocks: Vec<Block>,
    pub chars: usize,
}

enum Source {
    Zip(PathBuf),
    Memory(HashMap<String, Vec<u8>>),
}

pub struct Book {
    pub title: String,
    pub author: String,
    pub chapters: Vec<Chapter>,
    pub cover: Option<Vec<u8>>,
    source: Source,
}

impl Book {
    pub fn total_chars(&self) -> usize {
        self.chapters.iter().map(|c| c.chars).sum::<usize>().max(1)
    }

    pub fn progress_at(&self, chapter: usize, frac: f32) -> f32 {
        let before: usize = self.chapters.iter().take(chapter).map(|c| c.chars).sum();
        let cur = self.chapters.get(chapter).map_or(0, |c| c.chars) as f32;
        ((before as f32 + cur * frac.clamp(0.0, 1.0)) / self.total_chars() as f32).clamp(0.0, 1.0)
    }

    pub fn image_bytes(&self, key: &str) -> Option<Vec<u8>> {
        match &self.source {
            Source::Memory(m) => m.get(key).cloned(),
            Source::Zip(path) => {
                let mut zip = zip::ZipArchive::new(std::fs::File::open(path).ok()?).ok()?;
                read_bytes(&mut zip, key)
            }
        }
    }
}

pub fn load(path: &Path) -> Result<Book> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let mut book = match ext.as_str() {
        "epub" => load_epub(path)?,
        "mobi" | "azw" | "azw3" | "prc" => load_mobi(path)?,
        _ => return Err(anyhow!("unsupported format .{ext}")),
    };
    if book.chapters.is_empty() {
        return Err(anyhow!("no readable content found in this book"));
    }
    if book.title.trim().is_empty() {
        book.title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_string();
    }
    Ok(book)
}

fn read_entry(zip: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Result<String> {
    let bytes = read_bytes(zip, name).with_context(|| format!("missing {name} in archive"))?;
    Ok(String::from_utf8_lossy(&bytes)
        .trim_start_matches('\u{feff}')
        .to_string())
}

fn read_bytes(zip: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Option<Vec<u8>> {
    let mut file = zip.by_name(name).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hi = (b[i + 1] as char).to_digit(16);
            let lo = (b[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn resolve(base: &str, href: &str) -> String {
    let href = percent_decode(href.split('#').next().unwrap_or(""));
    let mut parts: Vec<&str> = base.split('/').filter(|p| !p.is_empty()).collect();
    for seg in href.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn load_epub(path: &Path) -> Result<Book> {
    let file = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(file).context("not a valid EPUB (zip) file")?;

    let container = read_entry(&mut zip, "META-INF/container.xml")?;
    let cdoc = roxmltree::Document::parse(&container)?;
    let opf_path = cdoc
        .descendants()
        .find(|n| n.tag_name().name() == "rootfile")
        .and_then(|n| n.attribute("full-path"))
        .ok_or_else(|| anyhow!("container.xml has no rootfile"))?
        .to_string();

    let opf = read_entry(&mut zip, &opf_path)?;
    let odoc = roxmltree::Document::parse(&opf)?;
    let meta = |name: &str| {
        odoc.descendants()
            .find(|n| n.is_element() && n.tag_name().name() == name)
            .and_then(|n| n.text())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let title = meta("title").unwrap_or_default();
    let author = meta("creator").unwrap_or_else(|| "Unknown author".to_string());

    let base = opf_path
        .rsplit_once('/')
        .map(|(d, _)| d.to_string())
        .unwrap_or_default();

    let manifest: HashMap<&str, &str> = odoc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "item")
        .filter_map(|n| Some((n.attribute("id")?, n.attribute("href")?)))
        .collect();
    let spine: Vec<&str> = odoc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "itemref")
        .filter_map(|n| n.attribute("idref"))
        .collect();

    let mut chapters = Vec::new();
    for (i, idref) in spine.iter().enumerate() {
        let Some(href) = manifest.get(idref) else { continue };
        let chapter_path = resolve(&base, href);
        let Ok(html) = read_entry(&mut zip, &chapter_path) else { continue };
        let dir = chapter_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        let blocks = html_to_blocks(&html)
            .into_iter()
            .filter_map(|b| match b {
                Block::Image(src) if src.starts_with("data:") => None,
                Block::Image(src) => Some(Block::Image(resolve(dir, &src))),
                other => Some(other),
            })
            .collect();
        if let Some(c) = make_chapter(blocks, &format!("Section {}", i + 1)) {
            chapters.push(c);
        }
    }

    let items = || odoc.descendants().filter(|n| n.is_element() && n.tag_name().name() == "item");
    let cover_meta = odoc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "meta" && n.attribute("name") == Some("cover"))
        .and_then(|n| n.attribute("content"));
    let is_image = |n: &roxmltree::Node| n.attribute("media-type").map_or(false, |m| m.starts_with("image/"));
    let cover_href = items()
        .find(|n| n.attribute("properties").map_or(false, |p| p.split_whitespace().any(|x| x == "cover-image")))
        .or_else(|| items().find(|n| cover_meta.is_some() && n.attribute("id") == cover_meta && is_image(n)))
        .or_else(|| items().find(|n| is_image(n) && n.attribute("id").map_or(false, |i| i.to_lowercase().contains("cover"))))
        .and_then(|n| n.attribute("href"));
    let cover = cover_href.and_then(|h| read_bytes(&mut zip, &resolve(&base, h)));

    Ok(Book { title, author, chapters, cover, source: Source::Zip(path.to_path_buf()) })
}

fn load_mobi(path: &Path) -> Result<Book> {
    let m = mobi::Mobi::from_path(path).map_err(|e| anyhow!("cannot read MOBI/AZW: {e}"))?;
    let title = m.title();
    let author = m.author().unwrap_or_else(|| "Unknown author".to_string());
    let html = m.content_as_string_lossy();
    let blocks = html_to_blocks(&html);

    let images: HashMap<String, Vec<u8>> = m
        .image_records()
        .iter()
        .enumerate()
        .map(|(i, r)| (format!("r{}", i + 1), r.content.to_vec()))
        .collect();
    let cover = images.get("r1").cloned();

    let mut chapters = Vec::new();
    let mut section: Vec<Block> = Vec::new();
    let mut n = 0;
    for b in blocks {
        if matches!(b, Block::Heading(_)) && !section.is_empty() {
            n += 1;
            if let Some(c) = make_chapter(std::mem::take(&mut section), &format!("Section {n}")) {
                chapters.push(c);
            }
        }
        section.push(b);
    }
    if let Some(c) = make_chapter(section, "Section") {
        chapters.push(c);
    }

    Ok(Book { title, author, chapters, cover, source: Source::Memory(images) })
}

fn make_chapter(blocks: Vec<Block>, fallback: &str) -> Option<Chapter> {
    if blocks.is_empty() {
        return None;
    }
    let title = blocks
        .iter()
        .find_map(|b| match b {
            Block::Heading(h) => Some(h.clone()),
            _ => None,
        })
        .unwrap_or_else(|| fallback.to_string());
    let chars = blocks
        .iter()
        .map(|b| match b {
            Block::Heading(t) | Block::Para(t) => t.chars().count(),
            Block::Image(_) => 300,
        })
        .sum();
    Some(Chapter { title, blocks, chars })
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        if let Some(j) = after.find(';').filter(|j| *j <= 8) {
            let ent = &after[..j];
            let rep = match ent {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                "hellip" => Some('\u{2026}'),
                "mdash" => Some('\u{2014}'),
                "ndash" => Some('\u{2013}'),
                "lsquo" => Some('\u{2018}'),
                "rsquo" => Some('\u{2019}'),
                "ldquo" => Some('\u{201c}'),
                "rdquo" => Some('\u{201d}'),
                "laquo" => Some('\u{ab}'),
                "raquo" => Some('\u{bb}'),
                e if e.starts_with("#x") || e.starts_with("#X") => {
                    u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32)
                }
                e if e.starts_with('#') => e[1..].parse::<u32>().ok().and_then(char::from_u32),
                _ => None,
            };
            if let Some(c) = rep {
                out.push(c);
                rest = &after[j + 1..];
                continue;
            }
        }
        out.push('&');
        rest = after;
    }
    out.push_str(rest);
    out
}

fn flush(cur: &mut String, blocks: &mut Vec<Block>, heading: bool) {
    let text = decode_entities(cur).split_whitespace().collect::<Vec<_>>().join(" ");
    cur.clear();
    if text.is_empty() {
        return;
    }
    blocks.push(if heading { Block::Heading(text) } else { Block::Para(text) });
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(name) {
        let at = from + i;
        let before_ok = at > 0 && lower.as_bytes()[at - 1].is_ascii_whitespace();
        let rest = tag[at + name.len()..].trim_start();
        if before_ok && rest.starts_with('=') {
            let v = rest[1..].trim_start();
            let q = v.chars().next()?;
            if q == '"' || q == '\'' {
                let inner = &v[1..];
                return Some(inner[..inner.find(q)?].to_string());
            }
            return v
                .split(|c: char| c.is_whitespace() || c == '/' || c == '>')
                .next()
                .map(|s| s.to_string());
        }
        from = at + name.len();
    }
    None
}

pub fn html_to_blocks(html: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut cur = String::new();
    let mut heading = false;
    let mut skip: Option<String> = None;
    let mut i = 0;

    while i < html.len() {
        let Some(off) = html[i..].find('<') else {
            if skip.is_none() {
                cur.push_str(&html[i..]);
            }
            break;
        };
        if skip.is_none() {
            cur.push_str(&html[i..i + off]);
        }
        let start = i + off;
        if html[start..].starts_with("<!--") {
            i = html[start..].find("-->").map(|e| start + e + 3).unwrap_or(html.len());
            continue;
        }
        let Some(end_off) = html[start..].find('>') else { break };
        let tag = &html[start + 1..start + end_off];
        i = start + end_off + 1;

        let closing = tag.starts_with('/');
        let self_closing = tag.trim_end().ends_with('/');
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_lowercase();

        if skip.is_some() {
            if closing && skip.as_deref() == Some(name.as_str()) {
                skip = None;
            }
            continue;
        }

        match name.as_str() {
            "script" | "style" | "head" if !closing && !self_closing => skip = Some(name),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                if closing {
                    flush(&mut cur, &mut blocks, true);
                    heading = false;
                } else {
                    flush(&mut cur, &mut blocks, heading);
                    heading = true;
                }
            }
            "img" | "image" if !closing => {
                let src = attr(tag, "recindex")
                    .and_then(|r| r.trim().parse::<u32>().ok())
                    .map(|n| format!("r{n}"))
                    .or_else(|| attr(tag, "src"))
                    .or_else(|| attr(tag, "xlink:href"))
                    .or_else(|| attr(tag, "href"));
                if let Some(src) = src {
                    flush(&mut cur, &mut blocks, heading);
                    blocks.push(Block::Image(src));
                }
            }
            "p" | "div" | "br" | "li" | "tr" | "blockquote" | "section" | "article" | "hr"
            | "ul" | "ol" | "table" | "pre" | "body" | "mbp" => flush(&mut cur, &mut blocks, heading),
            _ => {}
        }
    }
    flush(&mut cur, &mut blocks, heading);
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_markup_and_finds_images() {
        let b = html_to_blocks(
            "<html><head><title>x</title><script/></head><body><h1>Chapter &amp; One</h1>\
             <p>Hello <b>big</b>   world&#8217;s</p><img src=\"../img/a b.png\" alt=\"x\"/>\
             <script>var a=1;</script><p>Two</p></body></html>",
        );
        assert_eq!(
            b,
            vec![
                Block::Heading("Chapter & One".into()),
                Block::Para("Hello big world\u{2019}s".into()),
                Block::Image("../img/a b.png".into()),
                Block::Para("Two".into()),
            ]
        );
    }

    #[test]
    fn loads_real_book_when_path_given() {
        let Ok(p) = std::env::var("EBOOK_TEST_PATH") else { return };
        let b = load(Path::new(&p)).unwrap();
        let imgs: Vec<&String> = b
            .chapters
            .iter()
            .flat_map(|c| c.blocks.iter())
            .filter_map(|x| if let Block::Image(k) = x { Some(k) } else { None })
            .collect();
        let loadable = imgs.iter().filter(|k| b.image_bytes(k).is_some()).count();
        println!(
            "{} | {} chapters | {} chars | {} image refs, {} resolvable",
            b.title, b.chapters.len(), b.total_chars(), imgs.len(), loadable
        );
        assert!(imgs.is_empty() || loadable > 0);
    }
}
