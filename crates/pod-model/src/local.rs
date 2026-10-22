//! Serviço `Local` (ADR-0001, W3): pastas conhecidas (Downloads, Capturas) e área de
//! transferência. Tipos e regras puras; o app lê o Windows e entrega um `LocalSnapshot`.
//!
//! Privacidade: o widget só recebe nomes de arquivo, tamanhos e idades (nunca caminhos) e uma
//! prévia curta de texto da área de transferência; conteúdo que o próprio Windows marca como
//! privado (gerenciadores de senha) nunca é lido.

/// O que um widget quer receber. O provedor só lê o que alguém pediu.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LocalNeeds {
    pub downloads: bool,
    pub screenshots: bool,
    pub clipboard: bool,
}

impl LocalNeeds {
    pub const DOWNLOADS: Self = Self { downloads: true, screenshots: false, clipboard: false };
    pub const SCREENSHOTS: Self = Self { downloads: false, screenshots: true, clipboard: false };
    pub const CLIPBOARD: Self = Self { downloads: false, screenshots: false, clipboard: true };

    pub fn is_empty(self) -> bool {
        self == Self::default()
    }

    pub fn union(self, o: Self) -> Self {
        Self {
            downloads: self.downloads || o.downloads,
            screenshots: self.screenshots || o.screenshots,
            clipboard: self.clipboard || o.clipboard,
        }
    }

    /// 1 s para a área de transferência (só consulta um número de sequência; o texto é lido
    /// apenas quando ele muda); 10 s para pastas.
    pub fn interval_ms(self) -> u64 {
        if self.clipboard {
            1_000
        } else {
            10_000
        }
    }
}

/// Quantos arquivos recentes uma listagem leva.
pub const MAX_RECENT: usize = 5;
/// Tamanho máximo, em caracteres, da prévia de texto.
pub const MAX_PREVIEW_CHARS: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub name: String,
    pub size: u64,
    /// Segundos desde a última modificação.
    pub age_secs: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderListing {
    /// Total de arquivos (não conta subpastas).
    pub count: usize,
    /// Do mais novo para o mais antigo, no máximo `MAX_RECENT`.
    pub recent: Vec<FileEntry>,
}

impl FolderListing {
    /// Monta a listagem a partir de `(nome, tamanho, idade)` em qualquer ordem.
    pub fn from_files(mut files: Vec<FileEntry>) -> Self {
        let count = files.len();
        files.sort_by(|a, b| a.age_secs.cmp(&b.age_secs).then_with(|| a.name.cmp(&b.name)));
        files.truncate(MAX_RECENT);
        Self { count, recent: files }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Clip {
    /// Área de transferência vazia.
    Empty,
    /// Texto (prévia já normalizada e limitada).
    Text(String),
    /// Há conteúdo, mas não é texto (imagem, arquivos).
    Other,
    /// O aplicativo de origem pediu que o conteúdo não seja monitorado: nada foi lido.
    Private,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalSnapshot {
    pub downloads: Option<FolderListing>,
    pub screenshots: Option<FolderListing>,
    pub clipboard: Option<Clip>,
}

/// Prévia de texto: espaços em branco colapsados em um, sem bordas, limitada por caracteres
/// (com reticências quando corta). Nunca corta no meio de um caractere.
pub fn preview_text(raw: &str) -> String {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= MAX_PREVIEW_CHARS {
        return collapsed;
    }
    let mut out: String = collapsed.chars().take(MAX_PREVIEW_CHARS - 1).collect();
    out.push('\u{2026}');
    out
}

/// "agora", "há 5 min", "há 3 h", "há 2 d".
pub fn format_age(secs: u64) -> String {
    match secs {
        0..=59 => "agora".to_string(),
        60..=3_599 => format!("h\u{e1} {} min", secs / 60),
        3_600..=86_399 => format!("h\u{e1} {} h", secs / 3_600),
        _ => format!("h\u{e1} {} d", secs / 86_400),
    }
}

/// Reduz um nome de arquivo para caber em `max` caracteres mantendo a extensão:
/// "Relatorio-final-2026.pdf" -> "Relatorio-f….pdf".
pub fn shorten_name(name: &str, max: usize) -> String {
    let len = name.chars().count();
    if len <= max || max < 2 {
        return name.to_string();
    }
    let ext = name.rfind('.').filter(|i| *i > 0).map(|i| &name[i..]).filter(|e| e.chars().count() <= 6).unwrap_or("");
    let keep = max.saturating_sub(ext.chars().count() + 1).max(1);
    let head: String = name.chars().take(keep).collect();
    format!("{head}\u{2026}{ext}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(name: &str, age: u64) -> FileEntry {
        FileEntry { name: name.into(), size: 1, age_secs: age }
    }

    #[test]
    fn needs_union_and_interval() {
        assert!(LocalNeeds::default().is_empty());
        let u = LocalNeeds::DOWNLOADS.union(LocalNeeds::CLIPBOARD);
        assert!(u.downloads && u.clipboard && !u.screenshots);
        assert_eq!(u.interval_ms(), 1_000);
        assert_eq!(LocalNeeds::SCREENSHOTS.interval_ms(), 10_000);
    }

    #[test]
    fn listing_keeps_the_newest_and_counts_all() {
        let files: Vec<_> = (0..9).map(|i| f(&format!("f{i}.txt"), (9 - i) * 100)).collect();
        let l = FolderListing::from_files(files);
        assert_eq!(l.count, 9);
        assert_eq!(l.recent.len(), MAX_RECENT);
        assert_eq!(l.recent[0].name, "f8.txt", "o mais novo primeiro");
        assert!(l.recent.windows(2).all(|w| w[0].age_secs <= w[1].age_secs));
        assert_eq!(FolderListing::from_files(vec![]), FolderListing::default());
    }

    #[test]
    fn preview_collapses_whitespace_and_truncates_by_chars() {
        assert_eq!(preview_text("  ol\u{e1}\n\n  mundo\t!  "), "ol\u{e1} mundo !");
        assert_eq!(preview_text(""), "");
        let long = "\u{e7}".repeat(500);
        let p = preview_text(&long);
        assert_eq!(p.chars().count(), MAX_PREVIEW_CHARS);
        assert!(p.ends_with('\u{2026}'));
        let exact = "a".repeat(MAX_PREVIEW_CHARS);
        assert_eq!(preview_text(&exact), exact, "no limite não corta");
    }

    #[test]
    fn age_in_portuguese() {
        assert_eq!(format_age(0), "agora");
        assert_eq!(format_age(59), "agora");
        assert_eq!(format_age(60), "h\u{e1} 1 min");
        assert_eq!(format_age(3_599), "h\u{e1} 59 min");
        assert_eq!(format_age(3_600), "h\u{e1} 1 h");
        assert_eq!(format_age(86_400 * 3), "h\u{e1} 3 d");
    }

    #[test]
    fn names_keep_their_extension() {
        assert_eq!(shorten_name("a.txt", 12), "a.txt");
        assert_eq!(shorten_name("Relatorio-final-2026.pdf", 14), "Relatorio\u{2026}.pdf");
        assert_eq!(shorten_name("Relatorio-final-2026.pdf", 14).chars().count(), 14, "cabe exatamente em max");
        assert_eq!(shorten_name("semextensaomuitolonga", 8), "semexte\u{2026}");
        assert_eq!(shorten_name("x.tar.gz.muitolonga", 6).chars().next(), Some('x'));
    }
}
