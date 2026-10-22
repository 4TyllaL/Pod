//! Widget News (ADR-0001, W5): catálogo de feeds RSS/Atom permitidos e leitura das respostas.
//! Tudo puro. Como no clima, **quem escolhe a URL é o host**, a partir deste catálogo: o widget só
//! guarda o identificador do feed. O leitor de XML é mínimo e tolerante (RSS 2.0 e Atom), sem entidades
//! externas nem resolução de nada: lê só texto.

/// Feed do catálogo. A URL é fixa e `https`; o host dela está em `net::ALLOWED_HOSTS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Feed {
    pub id: &'static str,
    pub name: &'static str,
    pub url: &'static str,
}

/// Fontes oferecidas (todas verificadas: respondem 200 direto, sem redirecionamento, em < 256 KiB).
pub const FEEDS: [Feed; 6] = [
    Feed { id: "bbc-pt", name: "BBC Brasil", url: "https://feeds.bbci.co.uk/portuguese/rss.xml" },
    Feed { id: "agencia-brasil", name: "Ag\u{ea}ncia Brasil", url: "https://agenciabrasil.ebc.com.br/rss/ultimasnoticias/feed.xml" },
    Feed { id: "uol", name: "UOL Not\u{ed}cias", url: "https://rss.uol.com.br/feed/noticias.xml" },
    Feed { id: "bbc-world", name: "BBC World", url: "https://feeds.bbci.co.uk/news/world/rss.xml" },
    Feed { id: "hacker-news", name: "Hacker News", url: "https://hnrss.org/frontpage" },
    Feed { id: "the-verge", name: "The Verge", url: "https://www.theverge.com/rss/index.xml" },
];

pub const DEFAULT_FEED: &str = "bbc-pt";

pub fn feed_by_id(id: &str) -> Option<&'static Feed> {
    FEEDS.iter().find(|f| f.id == id)
}

/// Máximo de manchetes guardadas por feed e tamanho do título.
pub const MAX_HEADLINES: usize = 10;
pub const MAX_TITLE_CHARS: usize = 140;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Headline {
    pub title: String,
    /// Endereço do artigo. **Fica no host**: o widget só pede "abrir a manchete N" e o host revalida o link.
    pub link: String,
    /// Segundos desde a publicação, quando o feed informa a data.
    pub age_secs: Option<u64>,
}

/// Estado que o widget News recebe do host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewsState {
    /// "Widgets de internet" desligado (padrão): nada foi enviado.
    Off,
    Loading,
    Failed(String),
    Ready { source: String, items: Vec<Headline> },
}

// ---------------------------------------------------------------- texto

/// Bytes da resposta como texto: UTF-8 se válido; senão ISO-8859-1 (alguns feeds antigos, como o do UOL).
pub fn decode_text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|b| *b as char).collect(),
    }
}

/// Decodifica as entidades XML comuns e numéricas (`&amp;`, `&#39;`, `&#x27;`).
pub fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        match tail.find(';').filter(|e| *e <= 10) {
            Some(e) => {
                let name = &tail[1..e];
                let ch = match name {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some(' '),
                    n if n.starts_with("#x") || n.starts_with("#X") => u32::from_str_radix(&n[2..], 16).ok().and_then(char::from_u32),
                    n if n.starts_with('#') => n[1..].parse::<u32>().ok().and_then(char::from_u32),
                    _ => None,
                };
                match ch {
                    Some(c) => {
                        out.push(c);
                        rest = &tail[e + 1..];
                    }
                    None => {
                        out.push('&');
                        rest = &tail[1..];
                    }
                }
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Remove marcações HTML (`<b>`, `<a href=...>`) deixando só o texto.
pub fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0u32;
    for c in s.chars() {
        match c {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Texto limpo de um título: CDATA aberto, entidades, tags e espaços colapsados, cortado por caracteres.
pub fn clean_title(raw: &str) -> String {
    let t = raw.trim();
    let t = t.strip_prefix("<![CDATA[").and_then(|x| x.strip_suffix("]]>")).unwrap_or(t);
    let text = strip_tags(&decode_entities(t));
    let one = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() <= MAX_TITLE_CHARS {
        return one;
    }
    let mut s: String = one.chars().take(MAX_TITLE_CHARS - 1).collect();
    s.push('\u{2026}');
    s
}

// ---------------------------------------------------------------- XML mínimo

/// Conteúdos de todos os `<tag ...>...</tag>` (sem aninhamento do mesmo nome), na ordem do documento.
fn blocks<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let (open, close) = (format!("<{tag}"), format!("</{tag}>"));
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find(&open) {
        let after = &rest[i + open.len()..];
        // "<item" não pode ser "<itemize": o próximo caractere precisa fechar o nome.
        if !matches!(after.chars().next(), Some('>' | ' ' | '\t' | '\r' | '\n' | '/')) {
            rest = after;
            continue;
        }
        let Some(gt) = after.find('>') else { break };
        if after[..gt].ends_with('/') {
            rest = &after[gt + 1..]; // <tag/> vazio
            continue;
        }
        let body = &after[gt + 1..];
        let Some(end) = body.find(&close) else { break };
        out.push(&body[..end]);
        rest = &body[end + close.len()..];
    }
    out
}

/// Texto do primeiro `<tag>` dentro de `block` (conteúdo bruto, antes de limpar).
fn first_text<'a>(block: &'a str, tag: &str) -> Option<&'a str> {
    blocks(block, tag).into_iter().next()
}

/// `href` do primeiro `<link ... href="...">` que não é `rel="self"`/`enclosure` (Atom).
fn atom_link(block: &str) -> Option<String> {
    let mut rest = block;
    while let Some(i) = rest.find("<link") {
        let tag_end = rest[i..].find('>')? + i;
        let tag = &rest[i..tag_end];
        rest = &rest[tag_end..];
        let attr = |name: &str| {
            let key = format!("{name}=\"");
            let s = tag.find(&key)? + key.len();
            Some(&tag[s..s + tag[s..].find('"')?])
        };
        if matches!(attr("rel"), Some("self" | "enclosure" | "replies" | "edit")) {
            continue;
        }
        if let Some(h) = attr("href") {
            return Some(decode_entities(h));
        }
    }
    None
}

// ---------------------------------------------------------------- datas

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// "Sun, 18 Oct 2026 12:00:00 GMT" / "+0000" / "-0300" → segundos desde 1970 (UTC).
pub fn parse_rfc822(s: &str) -> Option<i64> {
    let s = s.trim();
    let s = s.split_once(',').map_or(s, |(_, r)| r).trim();
    let mut it = s.split_whitespace();
    let day: u32 = it.next()?.parse().ok()?;
    let mon = it.next()?;
    let month = MONTHS.iter().position(|m| mon.eq_ignore_ascii_case(m))? as u32 + 1;
    let year: i64 = it.next()?.parse().ok()?;
    let year = if year < 100 { year + 2000 } else { year };
    let time = it.next()?;
    let mut t = time.split(':');
    let (h, mi): (i64, i64) = (t.next()?.parse().ok()?, t.next()?.parse().ok()?);
    let sec: i64 = t.next().map_or(Some(0), |x| x.parse().ok())?;
    let zone = it.next().unwrap_or("GMT");
    let off = match zone {
        "GMT" | "UT" | "UTC" | "Z" => 0,
        z if (z.starts_with('+') || z.starts_with('-')) && z.len() == 5 => {
            let sign = if z.starts_with('-') { -1 } else { 1 };
            let (hh, mm): (i64, i64) = (z[1..3].parse().ok()?, z[3..5].parse().ok()?);
            sign * (hh * 3600 + mm * 60)
        }
        _ => 0,
    };
    if !(1..=31).contains(&day) || h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    Some(crate::usage::days_from_civil(year, month, day) * 86_400 + h * 3600 + mi * 60 + sec - off)
}

/// "2026-10-18T09:00:00-03:00" / "...Z" → segundos desde 1970 (UTC).
pub fn parse_iso_offset(s: &str) -> Option<i64> {
    let s = s.trim();
    let base = crate::usage::parse_iso_utc(s)?;
    let tail = s.get(19..)?;
    let tail = tail.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    let off = match tail.as_bytes().first() {
        Some(b'+') | Some(b'-') if tail.len() >= 6 => {
            let sign = if tail.starts_with('-') { -1 } else { 1 };
            let (hh, mm): (i64, i64) = (tail.get(1..3)?.parse().ok()?, tail.get(4..6)?.parse().ok()?);
            sign * (hh * 3600 + mm * 60)
        }
        _ => 0,
    };
    Some(base - off)
}

// ---------------------------------------------------------------- feed

/// Lê um feed RSS 2.0 (`<item>`) ou Atom (`<entry>`) e devolve até `MAX_HEADLINES` manchetes, na ordem do
/// documento (os feeds já vêm do mais novo para o mais antigo). `now_secs` calcula a idade de cada uma.
pub fn parse_feed(xml: &str, now_secs: i64) -> Vec<Headline> {
    let (items, atom) = {
        let rss = blocks(xml, "item");
        if rss.is_empty() {
            (blocks(xml, "entry"), true)
        } else {
            (rss, false)
        }
    };
    let mut out = Vec::new();
    for it in items {
        let title = first_text(it, "title").map(clean_title).unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        let link = if atom {
            atom_link(it).unwrap_or_default()
        } else {
            first_text(it, "link").map(|l| decode_entities(l.trim().trim_start_matches("<![CDATA[").trim_end_matches("]]>"))).unwrap_or_default()
        };
        let when = if atom {
            first_text(it, "updated").or_else(|| first_text(it, "published")).and_then(parse_iso_offset)
        } else {
            first_text(it, "pubDate").map(|d| d.trim().trim_start_matches("<![CDATA[").trim_end_matches("]]>")).and_then(parse_rfc822)
        };
        let age_secs = when.map(|t| (now_secs - t).max(0) as u64);
        out.push(Headline { title, link: link.trim().to_string(), age_secs });
        if out.len() == MAX_HEADLINES {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_792_324_800; // 2026-10-18 12:00:00 UTC

    const RSS: &str = r#"<?xml version="1.0"?><rss version="2.0"><channel><title>Fonte</title>
<item><title><![CDATA[Governo anuncia <b>nova</b> medida &amp; prazo]]></title><link>https://exemplo.com/a?x=1&amp;y=2</link><pubDate>Sun, 18 Oct 2026 11:00:00 GMT</pubDate></item>
<item><title>Segunda   manchete</title><link>https://exemplo.com/b</link><pubDate>Sun, 18 Oct 2026 08:00:00 -0300</pubDate></item>
<item><title></title><link>https://exemplo.com/c</link></item>
<itemize>nao e item</itemize>
</channel></rss>"#;

    const ATOM: &str = r#"<feed xmlns="http://www.w3.org/2005/Atom"><entry><title type="html">Titulo &#39;Atom&#39;</title>
<link rel="self" href="https://exemplo.com/self"/><link rel="alternate" type="text/html" href="https://exemplo.com/atom?a=1&amp;b=2"/>
<updated>2026-10-18T09:00:00-03:00</updated></entry><entry><title>Sem data</title><link href="https://exemplo.com/x"/></entry></feed>"#;

    #[test]
    fn rss_items_are_read_cleaned_and_dated() {
        let h = parse_feed(RSS, NOW);
        assert_eq!(h.len(), 2, "o item sem titulo e o <itemize> ficam de fora");
        assert_eq!(h[0].title, "Governo anuncia nova medida & prazo");
        assert_eq!(h[0].link, "https://exemplo.com/a?x=1&y=2");
        assert_eq!(h[0].age_secs, Some(3_600), "1 h antes de NOW");
        assert_eq!(h[1].title, "Segunda manchete", "espacos colapsados");
        // 08:00 -0300 = 11:00 UTC.
        assert_eq!(h[1].age_secs, Some(3_600));
    }

    #[test]
    fn atom_entries_use_the_alternate_link_and_the_offset() {
        let h = parse_feed(ATOM, NOW);
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].title, "Titulo 'Atom'");
        assert_eq!(h[0].link, "https://exemplo.com/atom?a=1&b=2", "ignora rel=self");
        assert_eq!(h[0].age_secs, Some(0), "09:00-03:00 = 12:00 UTC = agora");
    }

    #[test]
    fn dates_in_both_formats() {
        assert_eq!(parse_rfc822("Sun, 18 Oct 2026 12:00:00 GMT"), Some(NOW));
        assert_eq!(parse_rfc822("18 Oct 2026 09:00:00 -0300"), Some(NOW));
        assert_eq!(parse_rfc822("Sun, 18 Oct 2026 12:00 +0000"), Some(NOW));
        assert_eq!(parse_rfc822("lixo"), None);
        assert_eq!(parse_rfc822("Sun, 32 Oct 2026 12:00:00 GMT"), None);
        assert_eq!(parse_iso_offset("2026-10-18T12:00:00Z"), Some(NOW));
        assert_eq!(parse_iso_offset("2026-10-18T09:00:00-03:00"), Some(NOW));
        assert_eq!(parse_iso_offset("2026-10-18T12:00:00.123+00:00"), Some(NOW));
        assert_eq!(parse_iso_offset("ontem"), None);
    }

    #[test]
    fn entities_tags_and_titles() {
        assert_eq!(decode_entities("a &amp; b &lt;c&gt; &quot;d&quot; &#233; &#xE9; &fantasma; &"), "a & b <c> \"d\" \u{e9} \u{e9} &fantasma; &");
        assert_eq!(strip_tags("um <b>dois</b> <a href=\"x\">tres</a>"), "um dois tres");
        let long = clean_title(&"\u{e7}".repeat(300));
        assert_eq!(long.chars().count(), MAX_TITLE_CHARS);
        assert!(long.ends_with('\u{2026}'));
        assert_eq!(clean_title("  <![CDATA[ Oi ]]> "), "Oi");
    }

    #[test]
    fn latin1_feeds_are_decoded() {
        let bytes = b"<rss><channel><item><title>\xdaltimas Not\xedcias</title><link>https://x.com/a</link></item></channel></rss>";
        let h = parse_feed(&decode_text(bytes), NOW);
        assert_eq!(h[0].title, "\u{da}ltimas Not\u{ed}cias");
        assert_eq!(decode_text("a\u{e7}\u{e3}o".as_bytes()), "a\u{e7}\u{e3}o", "UTF-8 valido passa direto");
    }

    #[test]
    fn at_most_ten_headlines_and_garbage_gives_none() {
        let many: String = (0..30).map(|i| format!("<item><title>N{i}</title><link>https://x.com/{i}</link></item>")).collect();
        let h = parse_feed(&format!("<rss><channel>{many}</channel></rss>"), NOW);
        assert_eq!(h.len(), MAX_HEADLINES);
        assert_eq!(h[0].title, "N0");
        assert!(parse_feed("<html><body>erro 503</body></html>", NOW).is_empty());
        assert!(parse_feed("", NOW).is_empty());
        assert!(parse_feed("<rss><item><title>sem fecho", NOW).is_empty(), "XML truncado nao quebra");
    }

    #[test]
    fn the_catalog_is_https_and_every_host_is_allowed() {
        assert!(feed_by_id(DEFAULT_FEED).is_some());
        assert!(feed_by_id("inexistente").is_none());
        for f in FEEDS {
            assert!(crate::net::url_allowed(f.url), "{} fora da lista permitida: {}", f.id, f.url);
        }
        let ids: std::collections::HashSet<_> = FEEDS.iter().map(|f| f.id).collect();
        assert_eq!(ids.len(), FEEDS.len(), "ids unicos");
    }
}
