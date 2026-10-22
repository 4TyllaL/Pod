//! Provedor `MediaSession`: lê a sessão de mídia atual do Windows (GSMTC, a mesma do painel de
//! volume) numa thread própria e entrega o último estado ao shell. A thread só existe enquanto
//! algum widget visível assina; sem assinante ela é encerrada (nada é lido).
//!
//! As chamadas WinRT esperam resposta (`.get()`), por isso nunca rodam na thread de UI.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Playback,
};
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

use windows::Graphics::Imaging::{
    BitmapAlphaMode, BitmapDecoder, BitmapPixelFormat, BitmapTransform, ColorManagementMode, ExifOrientationMode,
};
use windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties as Props;

use pod_model::image::{content_id, fit_within, ImageData};
use pod_model::media::{app_name, MediaCommand, MediaSnapshot, MediaStatus};

/// Lado maior da capa guardada, em pixels (160x160 BGRA = 100 KB, bem abaixo do limite de 512 KiB).
const COVER_SIDE: u32 = 160;

/// Capa da faixa: decodifica o thumbnail da sessão e reduz. `None` se o app não informa capa.
fn fetch_cover(props: &Props) -> Option<std::sync::Arc<ImageData>> {
    let step = |what: &str, e: windows::core::Error| {
        crate::host::log(&format!("media: capa: {what} falhou: {e}"));
    };
    let thumb = match props.Thumbnail() {
        Ok(t) => t,
        Err(e) => { step("Thumbnail", e); return None; }
    };
    let stream = match thumb.OpenReadAsync().and_then(|a| a.join()) {
        Ok(s) => s,
        Err(e) => { step("abrir o stream", e); return None; }
    };
    let decoder = match BitmapDecoder::CreateAsync(&stream).and_then(|a| a.join()) {
        Ok(d) => d,
        Err(e) => { step("decodificar", e); return None; }
    };
    let (w, h) = (decoder.PixelWidth().ok()?, decoder.PixelHeight().ok()?);
    let (tw, th) = fit_within(w, h, COVER_SIDE);
    let transform = BitmapTransform::new().ok()?;
    transform.SetScaledWidth(tw).ok()?;
    transform.SetScaledHeight(th).ok()?;
    let pixels = match decoder
        .GetPixelDataTransformedAsync(
            BitmapPixelFormat::Bgra8,
            BitmapAlphaMode::Premultiplied,
            &transform,
            ExifOrientationMode::IgnoreExifOrientation,
            ColorManagementMode::DoNotColorManage,
        )
        .and_then(|a| a.join())
    {
        Ok(p) => p,
        Err(e) => { step("ler os pixels", e); return None; }
    };
    let bgra = pixels.DetachPixelData().ok()?.to_vec();
    let img = ImageData { id: content_id(&bgra), width: tw, height: th, bgra };
    crate::host::log(&format!("media: capa {}x{} ({} bytes)", img.width, img.height, img.bgra.len()));
    img.is_valid().then(|| std::sync::Arc::new(img))
}

/// Capa da última faixa lida: só se busca de novo quando a faixa muda.
#[derive(Default)]
struct CoverCache {
    key: String,
    cover: Option<std::sync::Arc<ImageData>>,
    /// Tentativas sem capa para a faixa atual: muitos apps publicam a capa um pouco depois do título.
    misses: u32,
}

/// Quantas leituras seguidas (1 s cada) esperam a capa de uma faixa antes de desistir.
const MAX_COVER_MISSES: u32 = 8;

static READS: AtomicU64 = AtomicU64::new(0);

/// Leituras da sessão feitas desde o início (para provar que sem assinante nada é lido).
pub fn reads() -> u64 {
    READS.load(Ordering::Relaxed)
}

const POLL: Duration = Duration::from_millis(1000);

fn secs(t: windows::Foundation::TimeSpan) -> u32 {
    (t.Duration.max(0) / 10_000_000).min(u32::MAX as i64) as u32
}

fn snapshot_of(session: &Session, cache: &mut CoverCache) -> windows::core::Result<MediaSnapshot> {
    let props = session.TryGetMediaPropertiesAsync()?.join()?;
    let info = session.GetPlaybackInfo()?;
    let controls = info.Controls()?;
    let status = match info.PlaybackStatus()? {
        Playback::Playing => MediaStatus::Playing,
        Playback::Paused => MediaStatus::Paused,
        _ => MediaStatus::Stopped,
    };
    let (mut position, mut duration) = (None, None);
    if let Ok(tl) = session.GetTimelineProperties() {
        if let (Ok(start), Ok(end), Ok(pos)) = (tl.StartTime(), tl.EndTime(), tl.Position()) {
            let len = secs(end).saturating_sub(secs(start));
            if len > 0 {
                duration = Some(len);
                position = Some(secs(pos).saturating_sub(secs(start)).min(len));
            }
        }
    }
    let (title, artist, album) = (props.Title()?.to_string(), props.Artist()?.to_string(), props.AlbumTitle()?.to_string());
    let app = app_name(&session.SourceAppUserModelId()?.to_string());
    let key = format!("{app}|{title}|{artist}|{album}");
    if key != cache.key {
        cache.key = key;
        cache.misses = 0;
        cache.cover = None;
    }
    if cache.cover.is_none() && cache.misses < MAX_COVER_MISSES {
        cache.cover = fetch_cover(&props);
        if cache.cover.is_none() {
            cache.misses += 1;
        }
    }
    Ok(MediaSnapshot {
        title,
        artist,
        album,
        app,
        status,
        can_prev: controls.IsPreviousEnabled()?,
        can_next: controls.IsNextEnabled()?,
        can_play_pause: controls.IsPlayPauseToggleEnabled()?,
        position_secs: position,
        duration_secs: duration,
        cover: cache.cover.clone(),
    })
}

/// Sessão atual, ou `None` se nada está tocando ou o gerenciador não responde.
fn current(manager: &Manager) -> Option<Session> {
    manager.GetCurrentSession().ok()
}

fn run_command(manager: &Manager, cmd: MediaCommand) {
    let Some(session) = current(manager) else { return };
    let _ = match cmd {
        MediaCommand::PlayPause => session.TryTogglePlayPauseAsync().and_then(|a| a.join()),
        MediaCommand::Next => session.TrySkipNextAsync().and_then(|a| a.join()),
        MediaCommand::Previous => session.TrySkipPreviousAsync().and_then(|a| a.join()),
    };
}

fn worker(stop: Arc<AtomicBool>, tx: Sender<Option<MediaSnapshot>>, commands: Receiver<MediaCommand>) {
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }
    let manager = Manager::RequestAsync().and_then(|a| a.join());
    crate::host::log(&format!("media: gerenciador {}", if manager.is_ok() { "ok" } else { "falhou" }));
    let manager = manager.ok();
    let mut cache = CoverCache::default();
    while !stop.load(Ordering::Relaxed) {
        if let Some(m) = &manager {
            READS.fetch_add(1, Ordering::Relaxed);
            let snap = current(m).and_then(|s| snapshot_of(&s, &mut cache).ok());
            if tx.send(snap).is_err() {
                return;
            }
        } else if tx.send(None).is_err() {
            return;
        }
        // Espera até o próximo ciclo, mas acorda na hora se chegar um comando.
        match commands.recv_timeout(POLL) {
            Ok(cmd) => {
                if let Some(m) = &manager {
                    run_command(m, cmd);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// Dono da thread de leitura. Vive na thread de UI; só comunica por canais.
pub struct Provider {
    handle: Option<JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    rx: Option<Receiver<Option<MediaSnapshot>>>,
    cmd_tx: Option<Sender<MediaCommand>>,
}

impl Default for Provider {
    fn default() -> Self {
        Self { handle: None, stop: Arc::new(AtomicBool::new(false)), rx: None, cmd_tx: None }
    }
}

impl Provider {
    pub fn running(&self) -> bool {
        self.handle.is_some()
    }

    pub fn start(&mut self) {
        if self.running() {
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let (cmd_tx, cmd_rx) = channel();
        let s = stop.clone();
        self.handle = std::thread::Builder::new().name("pod-media".into()).spawn(move || worker(s, tx, cmd_rx)).ok();
        if self.handle.is_some() {
            self.stop = stop;
            self.rx = Some(rx);
            self.cmd_tx = Some(cmd_tx);
        }
    }

    /// Pede o fim da thread e a solta: ela termina sozinha no próximo ciclo (a UI não espera).
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.handle = None;
        self.rx = None;
        self.cmd_tx = None;
    }

    /// O estado mais recente desde a última chamada. `None` = nada novo; `Some(None)` = nada tocando.
    pub fn take_latest(&mut self) -> Option<Option<MediaSnapshot>> {
        let rx = self.rx.as_ref()?;
        let mut latest = None;
        while let Ok(s) = rx.try_recv() {
            latest = Some(s);
        }
        latest
    }

    pub fn send(&self, cmd: MediaCommand) {
        if let Some(tx) = &self.cmd_tx {
            let _ = tx.send(cmd);
        }
    }
}

impl Drop for Provider {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timespan_is_converted_from_hundreds_of_nanoseconds() {
        assert_eq!(secs(windows::Foundation::TimeSpan { Duration: 1_870_000_000 }), 187);
        assert_eq!(secs(windows::Foundation::TimeSpan { Duration: -5 }), 0, "negativo vira zero");
    }

    #[test]
    fn provider_starts_and_stops_without_subscribers_reading_anything_else() {
        let mut p = Provider::default();
        assert!(!p.running());
        assert_eq!(p.take_latest(), None, "parado: nada a entregar");
        let before = reads();
        p.start();
        assert!(p.running());
        // A thread responde com um estado (`None` se nada toca neste PC).
        let mut got = None;
        for _ in 0..60 {
            if let Some(s) = p.take_latest() {
                got = Some(s);
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(got.is_some(), "a thread entregou um estado em 6 s");
        assert!(reads() > before);
        p.stop();
        assert!(!p.running());
        std::thread::sleep(Duration::from_millis(1300));
        let after_stop = reads();
        std::thread::sleep(Duration::from_millis(1300));
        assert_eq!(reads(), after_stop, "parou de ler depois do stop");
    }
}

/// Teste de integração dos comandos, com uma sessão de mídia **criada pelo próprio teste** (um
/// `MediaPlayer` silencioso). Fica `#[ignore]` porque mexe na sessão de mídia do Windows: rode com
/// `cargo test -p pod-app media_commands -- --ignored --nocapture`. Por segurança só envia comandos se
/// a sessão atual for a do teste (pelo título); se o usuário estiver tocando outra coisa, aborta.
#[cfg(test)]
mod command_tests {
    use super::*;
    use std::sync::mpsc::channel;
    use windows::Foundation::{TypedEventHandler, Uri};
    use windows::Media::Core::MediaSource;
    use windows::Media::Playback::MediaPlayer;
    use windows::Media::{
        MediaPlaybackStatus, MediaPlaybackType, SystemMediaTransportControls, SystemMediaTransportControlsButton,
        SystemMediaTransportControlsButtonPressedEventArgs,
    };
    use windows::core::HSTRING;

    const TITLE: &str = "Faixa de Teste do Pod (comandos)";

    fn silent_wav() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("pod-silent-{}.wav", std::process::id()));
        let data_len: u32 = 8000 * 2 * 30; // 30 s mono 16 bits a 8 kHz
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data_len).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes()); // PCM
        b.extend_from_slice(&1u16.to_le_bytes()); // mono
        b.extend_from_slice(&8000u32.to_le_bytes());
        b.extend_from_slice(&16000u32.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&data_len.to_le_bytes());
        b.resize(b.len() + data_len as usize, 0);
        std::fs::write(&path, b).unwrap();
        path
    }

    fn name_of(b: SystemMediaTransportControlsButton) -> &'static str {
        match b {
            SystemMediaTransportControlsButton::Play => "Play",
            SystemMediaTransportControlsButton::Pause => "Pause",
            SystemMediaTransportControlsButton::Next => "Next",
            SystemMediaTransportControlsButton::Previous => "Previous",
            _ => "Outro",
        }
    }

    fn wait_for_title(p: &mut Provider, want: &str, secs: u32) -> Option<MediaSnapshot> {
        for _ in 0..secs * 10 {
            if let Some(Some(s)) = p.take_latest() {
                if s.title == want {
                    return Some(s);
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        None
    }

    /// Sessão de teste que **obedece** aos botões, para ser usada com o Pod rodando ao lado: grava cada botão
    /// recebido em `POD_TEST_EVENTS` (um por linha) e reage (Pause/Play mudam o estado; Next/Previous mudam
    /// a faixa). Dura `POD_TEST_HOLD_SECS` segundos (padrão 40).
    #[test]
    #[ignore = "sessao de teste para uso manual com o Pod; rode com --ignored"]
    fn media_test_session_for_manual_runs() {
        use std::io::Write;
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        let hold: u64 = std::env::var("POD_TEST_HOLD_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(40);
        let events = std::env::var("POD_TEST_EVENTS").ok();
        let wav = silent_wav();
        let player = MediaPlayer::new().unwrap();
        player.CommandManager().unwrap().SetIsEnabled(false).unwrap();
        let uri = Uri::CreateUri(&HSTRING::from(format!("file:///{}", wav.to_string_lossy().replace('\\', "/")))).unwrap();
        player.SetSource(&MediaSource::CreateFromUri(&uri).unwrap()).unwrap();
        let smtc = player.SystemMediaTransportControls().unwrap();
        smtc.SetIsEnabled(true).unwrap();
        for f in [
            SystemMediaTransportControls::SetIsPlayEnabled,
            SystemMediaTransportControls::SetIsPauseEnabled,
            SystemMediaTransportControls::SetIsNextEnabled,
            SystemMediaTransportControls::SetIsPreviousEnabled,
        ] {
            f(&smtc, true).unwrap();
        }
        let publish = |smtc: &SystemMediaTransportControls, n: u32| {
            let du = smtc.DisplayUpdater().unwrap();
            du.SetType(MediaPlaybackType::Music).unwrap();
            let props = du.MusicProperties().unwrap();
            props.SetTitle(&HSTRING::from(format!("Faixa {n} do Teste"))).unwrap();
            props.SetArtist(&HSTRING::from("Artista Teste")).unwrap();
            du.Update().unwrap();
        };
        publish(&smtc, 1);
        smtc.SetPlaybackStatus(MediaPlaybackStatus::Playing).unwrap();
        player.Play().unwrap();
        let track = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(1));
        let t2 = track.clone();
        let handler = TypedEventHandler::<SystemMediaTransportControls, SystemMediaTransportControlsButtonPressedEventArgs>::new(
            move |sender, args| {
                let (Some(smtc), Some(a)) = (sender.as_ref(), args.as_ref()) else { return Ok(()) };
                let b = a.Button()?;
                if let Some(path) = &events {
                    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                        let _ = writeln!(f, "{}", name_of(b));
                    }
                }
                match b {
                    SystemMediaTransportControlsButton::Pause => smtc.SetPlaybackStatus(MediaPlaybackStatus::Paused)?,
                    SystemMediaTransportControlsButton::Play => smtc.SetPlaybackStatus(MediaPlaybackStatus::Playing)?,
                    SystemMediaTransportControlsButton::Next | SystemMediaTransportControlsButton::Previous => {
                        let n = if b == SystemMediaTransportControlsButton::Next { 2 } else { 1 };
                        t2.store(n, std::sync::atomic::Ordering::Relaxed);
                        let du = smtc.DisplayUpdater()?;
                        du.MusicProperties()?.SetTitle(&HSTRING::from(format!("Faixa {n} do Teste")))?;
                        du.Update()?;
                    }
                    _ => {}
                }
                Ok(())
            },
        );
        smtc.ButtonPressed(&handler).unwrap();
        eprintln!("sessao de teste ativa por {hold} s");
        std::thread::sleep(Duration::from_secs(hold));
        player.Close().ok();
        let _ = std::fs::remove_file(&wav);
        let _ = track;
    }

    #[test]
    #[ignore = "mexe na sessao de midia do Windows; rode com --ignored"]
    fn media_commands_reach_the_session() {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        let wav = silent_wav();
        let player = MediaPlayer::new().unwrap();
        player.CommandManager().unwrap().SetIsEnabled(false).unwrap();
        let uri = Uri::CreateUri(&HSTRING::from(format!("file:///{}", wav.to_string_lossy().replace('\\', "/")))).unwrap();
        player.SetSource(&MediaSource::CreateFromUri(&uri).unwrap()).unwrap();
        let smtc = player.SystemMediaTransportControls().unwrap();
        smtc.SetIsEnabled(true).unwrap();
        for f in [
            SystemMediaTransportControls::SetIsPlayEnabled,
            SystemMediaTransportControls::SetIsPauseEnabled,
            SystemMediaTransportControls::SetIsNextEnabled,
            SystemMediaTransportControls::SetIsPreviousEnabled,
        ] {
            f(&smtc, true).unwrap();
        }
        let (tx, rx) = channel::<&'static str>();
        let handler = TypedEventHandler::<SystemMediaTransportControls, SystemMediaTransportControlsButtonPressedEventArgs>::new(
            move |_, args| {
                if let Some(a) = args.as_ref() {
                    let _ = tx.send(name_of(a.Button()?));
                }
                Ok(())
            },
        );
        smtc.ButtonPressed(&handler).unwrap();
        let du = smtc.DisplayUpdater().unwrap();
        du.SetType(MediaPlaybackType::Music).unwrap();
        let props = du.MusicProperties().unwrap();
        props.SetTitle(&HSTRING::from(TITLE)).unwrap();
        props.SetArtist(&HSTRING::from("Artista Teste")).unwrap();
        du.Update().unwrap();
        smtc.SetPlaybackStatus(MediaPlaybackStatus::Playing).unwrap();
        player.Play().unwrap();

        let mut p = Provider::default();
        p.start();
        let snap = wait_for_title(&mut p, TITLE, 10);
        let Some(snap) = snap else {
            p.stop();
            player.Close().ok();
            let _ = std::fs::remove_file(&wav);
            eprintln!("ABORTADO: a sessao atual nao e a do teste (outro player ativo?); nenhum comando foi enviado");
            return;
        };
        assert_eq!(snap.status, MediaStatus::Playing);
        assert!(snap.can_prev && snap.can_next && snap.can_play_pause, "controles habilitados: {snap:?}");

        // Pausa pedida pelo provedor (como o botao do widget): o player de teste recebe Pause.
        p.send(MediaCommand::PlayPause);
        let got = rx.recv_timeout(Duration::from_secs(5)).expect("evento de botao apos PlayPause");
        assert_eq!(got, "Pause", "tocando + tocar/pausar = Pause");
        smtc.SetPlaybackStatus(MediaPlaybackStatus::Paused).unwrap(); // o "player" obedece
        let paused = (0..60).find_map(|_| {
            std::thread::sleep(Duration::from_millis(100));
            p.take_latest().flatten().filter(|s| s.title == TITLE && s.status == MediaStatus::Paused)
        });
        assert!(paused.is_some(), "o provedor passou a ver 'pausado'");

        p.send(MediaCommand::PlayPause);
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), "Play", "pausado + tocar/pausar = Play");
        smtc.SetPlaybackStatus(MediaPlaybackStatus::Playing).unwrap();

        p.send(MediaCommand::Next);
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), "Next");
        p.send(MediaCommand::Previous);
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), "Previous");
        eprintln!("comandos entregues ao player de teste: Pause, Play, Next, Previous");

        p.stop();
        player.Close().ok();
        let _ = std::fs::remove_file(&wav);
    }
}
