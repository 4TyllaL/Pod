//! Galeria offscreen: renderiza cada widget × formato × tema com dados de demonstração fixos,
//! em WARP, sem janela. Serve de harness de regressão visual (hashes dos pixels) e de vitrine.
//!
//! `pod-app --gallery <pasta>` grava um PNG por combinação, `hashes.txt` e `gallery.log`.

use std::fmt::Write as _;
use std::path::Path;

use windows::core::Result;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;

use crate::gfx::Gfx;
use crate::render::{self, DwMeasure, PaintStyle};
use crate::ui::theme::{Mode, Theme};
use crate::widgets;
use pod_model::ids::WidgetTypeId;
use pod_model::layout::{layout, Rect};
use pod_model::shape::Shape;
use pod_model::time::{weekday_of, TimeInfo};
use pod_model::view::{Node, View};
use pod_model::widget::{Event, Snapshot, ViewCtx};

/// Escala dos PNGs (192 DPI): nítidos para conferir a olho.
pub const SCALE: f32 = 2.0;

/// Domingo, 18 de outubro de 2026, 10:42:07: dia 291, ~44% do dia, 0% da semana... fixo de propósito.
pub fn demo_time() -> TimeInfo {
    TimeInfo { year: 2026, month: 10, day: 18, weekday: weekday_of(2026, 10, 18), hour: 10, minute: 42, second: 7, millis: 0 }
}

/// Leitura de demonstração fixa das métricas: CPU 37%, 8,4 de 16 GB, disco 58%, bateria 63%.
pub fn demo_sample() -> pod_model::metrics::Sample {
    use pod_model::metrics::{Battery, NetRate, Sample};
    Sample {
        net: Some(NetRate { down: 1_258_291.0, up: 348_160.0 }),
        cpu: Some(0.37),
        memory: Some((9_000_000_000, 17_179_869_184)),
        disk: Some((300 * 1024 * 1024 * 1024, 512 * 1024 * 1024 * 1024)),
        battery: Some(Battery::from_status(0, 1, 63, 7_800, false)),
    }
}

/// Resumo do GitHub de demonstração (fictício; a galeria nunca acessa a internet nem o cofre).
pub fn demo_github() -> pod_model::github::GithubState {
    use pod_model::github::{GithubState, Pull, Summary};
    GithubState::Ready(Summary {
        login: "octo".into(),
        unread: 12,
        unread_capped: false,
        review_requests: 2,
        my_pulls: 3,
        reviews: vec![
            Pull { title: "Corrige o vazamento de memória".into(), repo: "octo/hello".into() },
            Pull { title: "Atualiza dependências".into(), repo: "octo/tools".into() },
        ],
    })
}

/// Cotações de demonstração (fictícias; a galeria nunca acessa a internet).
pub fn demo_quotes() -> pod_model::quotes::QuoteState {
    use pod_model::quotes::{Quote, QuoteState, Source};
    let q = |l: &str, p: f64, c: f64| Quote { label: l.into(), price: p, change_pct: c };
    QuoteState::Ready {
        source: Source::Awesome,
        quotes: vec![
            q("D\u{f3}lar", 5.0402, -0.48),
            q("Euro", 5.6482, -0.84),
            q("BTC", 413_798.0, 0.07),
            q("Libra", 6.71, 1.35),
            q("ETH", 14_220.0, 2.9),
            q("Iene", 0.0338, -0.1),
            q("SOL", 812.5, 5.2),
            q("Peso", 0.0036, -1.1),
        ],
    }
}

/// Manchetes de demonstração (fictícias; a galeria nunca acessa a internet).
pub fn demo_news() -> pod_model::news::NewsState {
    use pod_model::news::{Headline, NewsState};
    let h = |t: &str, age| Headline { title: t.into(), link: "https://exemplo.com/materia".into(), age_secs: age };
    NewsState::Ready {
        source: "BBC Brasil".into(),
        items: vec![
            h("Governo anuncia pacote de medidas para conter a alta dos alimentos nos próximos meses", Some(1_800)),
            h("Cientistas descobrem novo planeta potencialmente habitável a 40 anos-luz", Some(7_300)),
            h("Seleção brasileira divulga lista de convocados para os próximos amistosos", Some(20_000)),
        ],
    }
}

/// Clima de demonstração (fixo; a galeria nunca acessa a internet).
pub fn demo_weather() -> pod_model::net::WeatherState {
    use pod_model::net::{WeatherReport, WeatherState};
    WeatherState::Ready {
        place: "Recife, Brasil".into(),
        report: WeatherReport { temp_c: 28.4, feels_c: 31.2, humidity: 74, wind_kmh: 14.7, code: 2, is_day: true, hi_c: 30.1, lo_c: 23.6, hourly: (13..19u8).map(|h| pod_model::net::HourPoint { hour: h, temp_c: 29.0 - (h as f32 - 13.0) * 0.8, code: [2, 2, 3, 61, 61, 3][(h - 13) as usize], is_day: h < 18 }).collect() },
    }
}

/// Uso do Claude Code de demonstração (números fictícios; a galeria nunca lê os arquivos do usuário).
pub fn demo_usage() -> pod_model::usage::UsageSnapshot {
    use pod_model::usage::{Tokens, UsageSnapshot};
    let mut u = UsageSnapshot::empty();
    for (i, out) in [180_000u64, 420_000, 90_000, 760_000, 1_300_000, 640_000, 1_050_000].iter().enumerate() {
        u.days[i] = Tokens { input: out / 20, output: *out, cache_read: out * 40, cache_create: out / 4 };
    }
    u.messages_today = 38;
    u.files = 12;
    u
}

/// Notificações de demonstração (fictícias; a galeria nunca lê as do usuário).
pub fn demo_notifications() -> pod_model::notifications::NotifSnapshot {
    use pod_model::notifications::{NotifAccess, NotifItem, NotifSnapshot};
    let it = |app: &str, t: &str, age| NotifItem { app: app.into(), title: t.into(), age_secs: age };
    NotifSnapshot {
        access: NotifAccess::Allowed,
        items: vec![
            it("Mensagens", "Ana: vamos almoçar?", 40),
            it("Calendário", "Reunião em 10 minutos", 600),
            it("Mail", "Fatura de outubro disponível", 7_300),
            it("Atualizações", "Reinicie para concluir", 90_000),
        ],
        total: 7,
    }
}

/// Faixa de demonstração fixa para o widget Music.
pub fn demo_media() -> pod_model::media::MediaSnapshot {
    use pod_model::media::{MediaSnapshot, MediaStatus};
    MediaSnapshot {
        title: "Bohemian Rhapsody".into(),
        artist: "Queen".into(),
        album: "A Night at the Opera".into(),
        app: "Spotify".into(),
        status: MediaStatus::Playing,
        can_prev: true,
        can_next: true,
        can_play_pause: true,
        position_secs: Some(94),
        duration_secs: Some(355),
        cover: Some(pod_model::image::ImageData::gradient(96, 96, [0xE8, 0x5D, 0x75], [0x3A, 0x1C, 0x71])),
    }
}

/// Pastas e área de transferência de demonstração (nomes fictícios, sem ler o disco).
pub fn demo_local() -> pod_model::local::LocalSnapshot {
    use pod_model::local::{Clip, FileEntry, FolderListing, LocalSnapshot};
    let f = |n: &str, size, age| FileEntry { name: n.into(), size, age_secs: age };
    let mut l = FolderListing::from_files(vec![
        f("Relatorio-final-2026.pdf", 2_400_000, 120),
        f("foto-viagem.png", 5_800_000, 3_700),
        f("instalador.exe", 88_000_000, 90_000),
        f("nota.txt", 900, 30),
        f("planilha-orcamento-q4.xlsx", 310_000, 400),
    ]);
    l.count = 42;
    LocalSnapshot {
        downloads: Some(l.clone()),
        screenshots: Some(l),
        clipboard: Some(Clip::Text("Reuni\u{e3}o na quinta \u{e0}s 14h, sala 3. Levar o relat\u{f3}rio impresso e o notebook.".into())),
    }
}

/// Eventos e tarefas de demonstração (a data de demonstração é 18/10/2026).
pub fn demo_shared() -> pod_model::shared::SharedData {
    use pod_model::shared::{Date, SharedData};
    let mut s = SharedData::default();
    let d = |day, month| Date::new(2026, month, day).unwrap();
    s.add_event(d(20, 10), "Reuni\u{e3}o com a equipe").unwrap();
    s.add_event(d(31, 10), "Entrega do projeto").unwrap();
    s.add_event(d(25, 12), "Natal").unwrap();
    for t in ["Comprar p\u{e3}o", "Revisar o relat\u{f3}rio", "Ligar para o contador", "Pagar a internet", "Marcar o dentista", "Estudar Rust"] {
        s.add_task(t).unwrap();
    }
    let first = s.tasks()[0].id;
    s.toggle_task(first);
    for (t, u) in [("Banco", "banco.com.br"), ("Not\u{ed}cias", "noticias.com.br"), ("Email", "email.com"), ("Mapas", "mapas.com"), ("V\u{ed}deos", "videos.com")] {
        s.add_link(t, u).unwrap();
    }
    let p = s.add_parcel("Fone novo", "AA123456789BR").unwrap();
    s.add_parcel("Livro de Rust", "BB987654321BR").unwrap();
    s.add_parcel("Teclado", "CC112233445BR").unwrap();
    s.toggle_parcel(p);
    s
}

pub struct Case {
    pub name: String,
    pub type_id: WidgetTypeId,
    pub state: Option<&'static str>,
    pub shape: Shape,
}

pub fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    let mut add = |label: &str, type_id: WidgetTypeId, state: Option<&'static str>, shapes: &[Shape]| {
        for s in shapes {
            out.push(Case { name: format!("{label}-{}", s.key()), type_id: type_id.clone(), state, shape: *s });
        }
    };
    let all = [Shape::S1X1, Shape::S2X1, Shape::S2X2];
    add("clock", widgets::clock::TYPE_ID, None, &all);
    add("clock-12h-seconds", widgets::clock::TYPE_ID, Some(r#"{"h24":false,"seconds":true}"#), &all);
    add("progress-day", widgets::time_progress::TYPE_ID, Some(r#"{"period":"day"}"#), &all);
    add("progress-week", widgets::time_progress::TYPE_ID, Some(r#"{"period":"week"}"#), &all);
    add("progress-month", widgets::time_progress::TYPE_ID, Some(r#"{"period":"month"}"#), &all);
    add("progress-year", widgets::time_progress::TYPE_ID, Some(r#"{"period":"year"}"#), &all);
    add("water", widgets::water::TYPE_ID, Some(r#"{"goal_ml":2000,"glass_ml":250,"day":null,"drunk_ml":1250}"#), &all);
    add("water-done", widgets::water::TYPE_ID, Some(r#"{"goal_ml":1500,"glass_ml":500,"day":null,"drunk_ml":1500}"#), &[Shape::S2X2]);
    add("converter", widgets::converter::TYPE_ID, Some(r#"{"category":"length","from":3,"to":7,"value":10}"#), &all);
    add("converter-temp", widgets::converter::TYPE_ID, Some(r#"{"category":"temperature","from":0,"to":1,"value":36.5}"#), &all);
    add("favorites", widgets::favorites::TYPE_ID, None, &all);
    add("parcels", widgets::parcels::TYPE_ID, None, &all);
    add("calendar", widgets::calendar::TYPE_ID, None, &all);
    add("calendar", widgets::calendar::TYPE_ID, None, &[Shape::S2X3, Shape::S2X4]);
    add("countdown", widgets::countdown::TYPE_ID, None, &all);
    add("reminders", widgets::reminders::TYPE_ID, None, &all);
    add("reminders", widgets::reminders::TYPE_ID, None, &[Shape::S2X3, Shape::S2X4]);
    add("timer", widgets::timer::TYPE_ID, Some(r#"{"duration_min":25,"remaining_ms":1125000,"sound":true}"#), &all);
    add("timer-idle", widgets::timer::TYPE_ID, Some(r#"{"duration_min":10,"remaining_ms":null,"sound":true}"#), &[Shape::S2X2]);
    add("stopwatch", widgets::stopwatch::TYPE_ID, Some(r#"{"elapsed_ms":754000,"laps":[200000,400000,754000]}"#), &all);
    add("pomodoro", widgets::pomodoro::TYPE_ID, Some(r#"{"focus_min":25,"short_min":5,"long_min":15,"rounds":4,"phase":"focus","round":2,"remaining_ms":1125000,"sound":true}"#), &all);
    add("pomodoro-break", widgets::pomodoro::TYPE_ID, Some(r#"{"focus_min":25,"short_min":5,"long_min":15,"rounds":4,"phase":"short_break","round":2,"remaining_ms":240000,"sound":true}"#), &all);
    add("cpu", widgets::system::CPU, None, &all);
    add("memory", widgets::system::MEMORY, None, &all);
    add("disk", widgets::system::DISK, None, &all);
    add("battery", widgets::system::BATTERY, None, &all);
    add("overview", widgets::overview::TYPE_ID, None, &all);
    add("network", widgets::network::TYPE_ID, None, &all);
    add("music", widgets::music::TYPE_ID, None, &all);
    add("notifications", widgets::notifications::TYPE_ID, None, &all);
    add("claude-usage", widgets::claude_usage::TYPE_ID, None, &all);
    add("weather", widgets::weather::TYPE_ID, Some(r#"{"city":"Recife"}"#), &all);
    add("news", widgets::news::TYPE_ID, None, &all);
    add("news", widgets::news::TYPE_ID, None, &[Shape::S2X3, Shape::S2X4]);
    add("news", widgets::news::TYPE_ID, None, &[Shape::S4X1, Shape::S4X2]);
    add("calendar", widgets::calendar::TYPE_ID, None, &[Shape::S4X1, Shape::S4X2]);
    add("weather", widgets::weather::TYPE_ID, Some(r#"{"city":"Recife"}"#), &[Shape::S4X1, Shape::S4X2]);
    add("calendar", widgets::calendar::TYPE_ID, None, &[Shape::S4X4]);
    add("github", widgets::github::TYPE_ID, None, &all);
    add("agenda", widgets::agenda::TYPE_ID, None, &[Shape::S1X1, Shape::S2X1, Shape::S2X2, Shape::S2X3, Shape::S4X1]);
    add("launcher", widgets::launcher::TYPE_ID, Some(r#"{"items":[{"name":"Chrome","path":"C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe","folder":false},{"name":"Bloco de Notas","path":"C:\\Windows\\notepad.exe","folder":false},{"name":"Projetos","path":"C:\\Projetos\\","folder":true},{"name":"Downloads","path":"C:\\Users\\voce\\Downloads\\","folder":true},{"name":"Calculadora","path":"C:\\Windows\\calc.exe","folder":false}]}"#), &[Shape::S1X1, Shape::S2X1, Shape::S2X2, Shape::S2X3, Shape::S4X1]);
    add("worldclock", widgets::worldclock::TYPE_ID, None, &[Shape::S1X1, Shape::S2X1, Shape::S2X2, Shape::S4X1]);
    add("holidays", widgets::holidays::TYPE_ID, None, &[Shape::S1X1, Shape::S2X1, Shape::S2X2, Shape::S2X3]);
    add("stocks", widgets::stocks::TYPE_ID, None, &[Shape::S1X1, Shape::S2X1, Shape::S2X2, Shape::S2X3, Shape::S2X4, Shape::S4X1, Shape::S4X2]);
    add("habits", widgets::habits::TYPE_ID, Some(r#"{"habits":[{"name":"Ler 15 min","days":[20740,20741,20742,20743,20744]},{"name":"Beber 2 L de água","days":[20743,20744]},{"name":"Caminhar","days":[20743]},{"name":"Meditar","days":[]}]}"#), &[Shape::S1X1, Shape::S2X1, Shape::S2X2, Shape::S2X3]);
    add("downloads", widgets::files::DOWNLOADS, None, &all);
    add("screenshots", widgets::files::SCREENSHOTS, None, &all);
    add("clipboard", widgets::clipboard::TYPE_ID, None, &all);
    out
}

pub fn themes() -> [(&'static str, Theme); 7] {
    let hc = Theme::high_contrast([0, 0, 0], [255, 255, 255], [0x3F, 0xF2, 0x3F], [0x1A, 0xEB, 0xFF], [0, 0, 0]);
    [
        ("light", Theme::light()),
        ("dark", Theme::dark()),
        ("hc", hc),
        ("glass-light", Theme::light().glass()),
        ("glass-dark", Theme::dark().glass()),
        ("muted-light", Theme::light().muted()),
        ("muted-dark", Theme::dark().muted()),
    ]
}

/// Constrói a view do caso com o tempo de demonstração.
pub fn view_of(case: &Case) -> Option<View> {
    let mut rt = crate::runtime::Runtime::new(Box::new(crate::runtime::RandomIds::new()));
    widgets::register_all(&mut rt);
    let snap = case.state.map(|d| Snapshot { version: 1, data: d.to_string() });
    let id = rt.create(&case.type_id, snap.as_ref()).ok()?;
    rt.mount(id).ok()?;
    rt.start(id, 0).ok()?;
    rt.set_shared(demo_shared());
    // Uma subida de download antes da leitura final dá forma ao gráfico da Rede; os demais ignoram.
    for i in 0..40 {
        let mut s = demo_sample();
        s.net = Some(pod_model::metrics::NetRate { down: 60_000.0 + (i as f32 * 0.4).sin().abs() * 900_000.0, up: 20_000.0 });
        let _ = rt.dispatch(id, &Event::Metrics(s));
    }
    let _ = rt.dispatch(id, &Event::Metrics(demo_sample())); // ignorado por quem não assina
    let _ = rt.dispatch(id, &Event::Local(demo_local()));
    let _ = rt.dispatch(id, &Event::Media(Some(demo_media())));
    let _ = rt.dispatch(id, &Event::Notifications(demo_notifications()));
    let _ = rt.dispatch(id, &Event::ClaudeUsage(demo_usage()));
    let _ = rt.dispatch(id, &Event::Weather(demo_weather()));
    let _ = rt.dispatch(id, &Event::News(demo_news()));
    let _ = rt.dispatch(id, &Event::Github(demo_github()));
    let _ = rt.dispatch(id, &Event::Quotes(demo_quotes()));
    let z = |h, m, d| Some(pod_model::worldclock::ZoneTime { hour: h, minute: m, day_diff: d });
    let _ = rt.dispatch(id, &Event::Zones(vec![z(9, 42, 0), z(13, 42, 0), z(22, 42, 0), z(23, 12, 1)]));
    rt.tile_view(id, &ViewCtx { shape: case.shape, time: demo_time() }).ok().flatten()
}

pub struct Image {
    pub width: u32,
    pub height: u32,
    /// RGBA com alfa direto (não pré-multiplicado).
    pub rgba: Vec<u8>,
}

/// Renderiza a view em um bitmap offscreen.
pub fn render_view(gfx: &Gfx, view: &View, shape: Shape, theme: &Theme, accent_index: usize) -> Result<Image> {
    let size = shape.size_dip();
    let st = PaintStyle { theme, accent: theme.accent(accent_index) };
    let laid = layout(view, Rect::new(0.0, 0.0, size.w, size.h), &DwMeasure(gfx));
    render_custom(gfx, size.w, size.h, |ctx| {
        render::paint_tile_background(ctx, (size.w, size.h), &st)?;
        render::paint_view(ctx, gfx, view, &laid, &st)
    })
}

/// Desenha qualquer coisa (`paint`, em DIP) num bitmap offscreen de `w` x `h` DIP.
pub fn render_custom(gfx: &Gfx, w: f32, h: f32, paint: impl FnOnce(&ID2D1DeviceContext) -> Result<()>) -> Result<Image> {
    let (wpx, hpx) = ((w * SCALE).round() as u32, (h * SCALE).round() as u32);
    unsafe {
        let ctx = gfx.d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
        let fmt = D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED };
        let dims = D2D_SIZE_U { width: wpx, height: hpx };
        let target = ctx.CreateBitmap(
            dims,
            None,
            0,
            &D2D1_BITMAP_PROPERTIES1 { pixelFormat: fmt, dpiX: 96.0, dpiY: 96.0, bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET, ..Default::default() },
        )?;
        let staging = ctx.CreateBitmap(
            dims,
            None,
            0,
            &D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: fmt,
                dpiX: 96.0,
                dpiY: 96.0,
                bitmapOptions: D2D1_BITMAP_OPTIONS_CPU_READ | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                ..Default::default()
            },
        )?;
        ctx.SetTarget(&target);
        ctx.SetDpi(96.0 * SCALE, 96.0 * SCALE);
        ctx.BeginDraw();
        ctx.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
        let drawn = paint(&ctx);
        ctx.EndDraw(None, None)?;
        drawn?;
        staging.CopyFromBitmap(None, &target, None)?;
        let mapped = staging.Map(D2D1_MAP_OPTIONS_READ)?;
        let mut rgba = vec![0u8; (wpx * hpx * 4) as usize];
        for y in 0..hpx as usize {
            let row = std::slice::from_raw_parts(mapped.bits.add(y * mapped.pitch as usize), wpx as usize * 4);
            for x in 0..wpx as usize {
                let (b, g, r, a) = (row[x * 4], row[x * 4 + 1], row[x * 4 + 2], row[x * 4 + 3]);
                let un = |c: u8| if a == 0 { 0 } else { ((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8 };
                let o = (y * wpx as usize + x) * 4;
                rgba[o..o + 4].copy_from_slice(&[un(r), un(g), un(b), a]);
            }
        }
        staging.Unmap()?;
        Ok(Image { width: wpx, height: hpx, rgba })
    }
}

/// Hash FNV-1a 64 dos pixels: identifica o desenho exato (nesta máquina/fontes/WARP).
pub fn hash_pixels(img: &Image) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in img.width.to_le_bytes().iter().chain(img.height.to_le_bytes().iter()).chain(img.rgba.iter()) {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (n, slot) in table.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 };
        }
        *slot = c;
    }
    !data.iter().fold(!0u32, |c, b| table[((c ^ *b as u32) & 0xff) as usize] ^ (c >> 8))
}

/// PNG RGBA 8 bits sem compressão (blocos "stored"): simples e sem dependências.
pub fn encode_png(img: &Image) -> Vec<u8> {
    let mut raw = Vec::with_capacity((img.width as usize * 4 + 1) * img.height as usize);
    for row in img.rgba.chunks(img.width as usize * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    let mut chunks = raw.chunks(65_535).peekable();
    while let Some(c) = chunks.next() {
        z.push(u8::from(chunks.peek().is_none()));
        z.extend_from_slice(&(c.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(c.len() as u16)).to_le_bytes());
        z.extend_from_slice(c);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65_521;
        b = (b + a) % 65_521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());

    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut chunk = |kind: &[u8; 4], data: &[u8]| {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc32(&body).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&img.width.to_be_bytes());
    ihdr.extend_from_slice(&img.height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    out
}

/// Renderiza tudo: um PNG por combinação, `hashes.txt` e `gallery.log`.
pub fn run(dir: &Path) -> std::result::Result<(), String> {
    crate::render::MARQUEE_FROZEN.store(true, std::sync::atomic::Ordering::Relaxed);
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let gfx = Gfx::new(true).map_err(|e| format!("WARP: {e}"))?;
    let mut hashes = String::new();
    let mut log = String::new();
    for case in cases() {
        let Some(view) = view_of(&case) else {
            let _ = writeln!(log, "{}: sem view", case.name);
            continue;
        };
        for (tname, theme) in themes() {
            let key = format!("{}-{tname}", case.name);
            match render_view(&gfx, &view, case.shape, &theme, widgets::accent_for(&case.type_id).unwrap_or(0)) {
                Ok(img) => {
                    let _ = writeln!(hashes, "{key} {:016x}", hash_pixels(&img));
                    std::fs::write(dir.join(format!("{key}.png")), encode_png(&img)).map_err(|e| e.to_string())?;
                }
                Err(e) => {
                    let _ = writeln!(log, "{key}: {e}");
                }
            }
        }
    }
    // Pod Studio em alguns estados, claro e escuro (só PNG; sem hash dourado).
    for (tname, theme) in themes().into_iter().filter(|(n, _)| *n == "light" || *n == "dark") {
        let states: [(&str, usize, &str); 4] = [("todos", 0, ""), ("tempo", 1, ""), ("busca", 0, "rel"), ("ajustes", crate::edit::settings_section(), "")];
        for (name, section, query) in states {
            let mut st = crate::edit::Studio::new(&gfx);
            st.set_section(section);
            for c in query.chars() {
                st.type_char(c);
            }
            st.glass = true;
            if name == "todos" {
                st.hover = Some(crate::edit::Hit::Card(0));
            }
            match render_custom(&gfx, crate::edit::WIN_W, crate::edit::WIN_H, |ctx| crate::edit::paint_studio(ctx, &gfx, &st, &theme)) {
                Ok(img) => std::fs::write(dir.join(format!("studio-{name}-{tname}.png")), encode_png(&img)).map_err(|e| e.to_string())?,
                Err(e) => {
                    let _ = writeln!(log, "studio-{name}-{tname}: {e}");
                }
            }
        }
    }
    let _ = writeln!(log, "fonte: {}", crate::gfx::ui_font_family());
    std::fs::write(dir.join("hashes.txt"), hashes).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("gallery.log"), log).map_err(|e| e.to_string())?;
    Ok(())
}

/// Tema usado só para dar nome aos modos em mensagens.
#[allow(dead_code)]
fn mode_name(m: Mode) -> &'static str {
    match m {
        Mode::Light => "light",
        Mode::Dark => "dark",
        Mode::HighContrast => "hc",
    }
}

#[allow(dead_code)]
fn empty_view() -> View {
    View::new(Node::spacer())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_the_reference_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF43926);
    }

    #[test]
    fn png_has_signature_and_ends_with_iend() {
        let img = Image { width: 2, height: 1, rgba: vec![255, 0, 0, 255, 0, 0, 255, 128] };
        let png = encode_png(&img);
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
        assert_eq!(&png[12..16], b"IHDR");
    }

    /// Todas as combinações renderizam, têm cantos transparentes e conteúdo, e batem com os hashes
    /// gravados. Os hashes dependem de fonte e WARP desta máquina: após mudar o visual de propósito
    /// (ou numa máquina nova), `POD_UPDATE_GOLDEN=1 cargo test -p pod-app visual` os regrava.
    #[test]
    fn visual_regression_matches_the_golden_hashes() {
        let gfx = Gfx::new(true).expect("WARP");
        let mut got = String::new();
        for case in cases() {
            let view = view_of(&case).expect("view");
            for (tname, theme) in themes() {
                let key = format!("{}-{tname}", case.name);
                let img = render_view(&gfx, &view, case.shape, &theme, widgets::accent_for(&case.type_id).unwrap_or(0)).expect(&key);
                assert_eq!(img.rgba[3], 0, "{key}: canto (0,0) deveria ser transparente");
                // Vidro: a superfície tem alfa ~0,8, então "quase opaco" é alfa >= 200.
                let opaque = img.rgba.chunks(4).filter(|p| p[3] >= 200).count();
                assert!(opaque > img.rgba.len() / 4 / 2, "{key}: cartão quase vazio ({opaque} px opacos)");
                let distinct: std::collections::HashSet<[u8; 4]> =
                    img.rgba.chunks(4).map(|p| [p[0], p[1], p[2], p[3]]).collect();
                assert!(distinct.len() > 8, "{key}: sem conteúdo desenhado");
                let _ = writeln!(got, "{key} {:016x}", hash_pixels(&img));
            }
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/gallery-hashes.txt");
        if std::env::var_os("POD_UPDATE_GOLDEN").is_some() {
            std::fs::write(&path, &got).unwrap();
            return;
        }
        let want = std::fs::read_to_string(&path).expect("tests/golden/gallery-hashes.txt");
        let diff: Vec<&str> = got.lines().filter(|l| !want.lines().any(|w| w == *l)).collect();
        assert!(diff.is_empty(), "visual mudou em {} casos, p.ex. {:?}", diff.len(), &diff[..diff.len().min(3)]);
    }

    #[test]
    fn every_case_has_a_valid_view() {
        for c in cases() {
            let v = view_of(&c).unwrap_or_else(|| panic!("{} sem view", c.name));
            assert_eq!(v.validate(), Ok(()), "{}", c.name);
        }
    }
}

/// Prévia de um widget para a galeria "Adicionar widget": formato padrão e view com dados de
/// demonstração. `None` se o tipo não existe ou não é um cartão.
pub fn preview(type_id: &WidgetTypeId) -> Option<(Shape, View)> {
    let mut rt = crate::runtime::Runtime::new(Box::new(crate::runtime::RandomIds::new()));
    widgets::register_all(&mut rt);
    let shape = rt.descriptor(type_id)?.default_shape?;
    let case = Case { name: String::new(), type_id: type_id.clone(), state: None, shape };
    view_of(&case).map(|v| (shape, v))
}
