//! Serviço `MediaSession` (ADR-0001, W4): o que o widget Music recebe da sessão de mídia do Windows
//! (a mesma que o painel de volume mostra) e os comandos que ele pode devolver. Tipos e regras
//! puras; o app lê o Windows em uma thread própria e entrega um `MediaSnapshot`.
//!
//! Privacidade: só metadados de reprodução (título, artista, álbum, aplicativo, estado, posição). Não
//! há nada gravado em disco e nada sai do computador.

use std::sync::Arc;

use crate::image::ImageData;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaStatus {
    Playing,
    Paused,
    /// Há sessão, mas parada (ou ainda abrindo/mudando de faixa).
    Stopped,
}

/// O que o botão físico faria. O widget só pede; o host executa na sessão atual.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaCommand {
    PlayPause,
    Next,
    Previous,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaSnapshot {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Nome do aplicativo que toca (por exemplo "Spotify"), sem o caminho do pacote.
    pub app: String,
    pub status: MediaStatus,
    pub can_prev: bool,
    pub can_next: bool,
    pub can_play_pause: bool,
    /// Posição e duração em segundos, quando o aplicativo informa.
    pub position_secs: Option<u32>,
    pub duration_secs: Option<u32>,
    /// Capa do album ja reduzida pelo host (`None` = o app nao informou ou nao foi possivel ler).
    pub cover: Option<Arc<ImageData>>,
}

impl MediaSnapshot {
    /// Fração tocada (0..=1) se posição e duração são conhecidas e a duração é positiva.
    pub fn progress(&self) -> Option<f32> {
        match (self.position_secs, self.duration_secs) {
            (Some(p), Some(d)) if d > 0 => Some((p as f32 / d as f32).clamp(0.0, 1.0)),
            _ => None,
        }
    }

    /// Mesmo conteúdo visível, ignorando a posição (que muda a cada segundo): decide se o cartão
    /// precisa de um quadro por mudança de faixa ou de estado.
    pub fn same_track(&self, other: &Self) -> bool {
        self.title == other.title
            && self.artist == other.artist
            && self.album == other.album
            && self.app == other.app
            && self.status == other.status
            && self.can_prev == other.can_prev
            && self.can_next == other.can_next
            && self.can_play_pause == other.can_play_pause
            && self.cover == other.cover
    }
}

/// "3:07", "62:05" (minutos sem limite) e "—" se não há tempo.
pub fn format_clock(secs: Option<u32>) -> String {
    match secs {
        Some(s) => format!("{}:{:02}", s / 60, s % 60),
        None => "\u{2014}".to_string(),
    }
}

/// "Artista · Álbum" sem separador sobrando quando falta um dos dois.
pub fn subtitle(artist: &str, album: &str) -> String {
    match (artist.trim().is_empty(), album.trim().is_empty()) {
        (true, true) => String::new(),
        (false, true) => artist.trim().to_string(),
        (true, false) => album.trim().to_string(),
        (false, false) => format!("{} \u{b7} {}", artist.trim(), album.trim()),
    }
}

/// Limpa o identificador do aplicativo da sessão ("Spotify.exe" → "Spotify",
/// "Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic" → "ZuneMusic"). Texto vazio se nada sobrar.
pub fn app_name(raw: &str) -> String {
    let id = raw.split('!').next().unwrap_or(raw);
    let id = id.split('_').next().unwrap_or(id);
    let id = id.rsplit(['\\', '/']).next().unwrap_or(id);
    let id = id.strip_suffix(".exe").or_else(|| id.strip_suffix(".EXE")).unwrap_or(id);
    let last = id.rsplit('.').next().unwrap_or(id);
    last.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap() -> MediaSnapshot {
        MediaSnapshot {
            title: "Faixa".into(),
            artist: "Artista".into(),
            album: "Álbum".into(),
            app: "Spotify".into(),
            status: MediaStatus::Playing,
            can_prev: true,
            can_next: true,
            can_play_pause: true,
            position_secs: Some(62),
            duration_secs: Some(200),
            cover: None,
        }
    }

    #[test]
    fn progress_needs_both_times_and_clamps() {
        assert_eq!(snap().progress(), Some(0.31));
        let mut s = snap();
        s.position_secs = Some(500);
        assert_eq!(s.progress(), Some(1.0), "posição além da duração");
        s.duration_secs = Some(0);
        assert_eq!(s.progress(), None, "duração zero");
        s.duration_secs = None;
        assert_eq!(s.progress(), None);
    }

    #[test]
    fn same_track_ignores_only_the_position() {
        let a = snap();
        let mut b = snap();
        b.position_secs = Some(63);
        assert!(a.same_track(&b));
        b.status = MediaStatus::Paused;
        assert!(!a.same_track(&b));
        let mut c = snap();
        c.title = "Outra".into();
        assert!(!a.same_track(&c));
    }

    #[test]
    fn a_new_cover_is_a_new_track_even_with_the_same_text() {
        let a = snap();
        let mut b = snap();
        b.cover = Some(ImageData::gradient(4, 4, [1, 2, 3], [4, 5, 6]));
        assert!(!a.same_track(&b));
        let mut c = snap();
        c.cover = Some(ImageData::gradient(4, 4, [1, 2, 3], [4, 5, 6]));
        assert!(b.same_track(&c), "mesma capa (mesmo id)");
    }

    #[test]
    fn clock_and_subtitle_in_portuguese_style() {
        assert_eq!(format_clock(Some(187)), "3:07");
        assert_eq!(format_clock(Some(3725)), "62:05");
        assert_eq!(format_clock(None), "\u{2014}");
        assert_eq!(subtitle("A", "B"), "A \u{b7} B");
        assert_eq!(subtitle("A", " "), "A");
        assert_eq!(subtitle("", "B"), "B");
        assert_eq!(subtitle("", ""), "");
    }

    #[test]
    fn app_names_are_cleaned() {
        assert_eq!(app_name("Spotify.exe"), "Spotify");
        assert_eq!(app_name("C:\\Program Files\\VLC\\vlc.exe"), "vlc");
        assert_eq!(app_name("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic"), "ZuneMusic");
        assert_eq!(app_name(""), "");
    }
}
