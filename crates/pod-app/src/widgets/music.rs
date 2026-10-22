//! Widget Music: o que está tocando na sessão de mídia do Windows (qualquer app que apareça no
//! painel de volume: Spotify, navegador, Groove...), com anterior, tocar/pausar e próxima.
//! Formatos 1×1, 2×1 e 2×2. Assina `MediaSession` só enquanto visível; não guarda nada em disco.
//! A capa do álbum ainda não é exibida (falta uma primitiva de imagem na view).

use pod_model::ids::WidgetTypeId;
use pod_model::media::{format_clock, subtitle, MediaCommand, MediaSnapshot, MediaStatus};
use pod_model::shape::Shape;
use pod_model::view::{Align, Fill, Kind, Len, Node, Role, TextStyle, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, LocalId, Outcome, SizeDip, Snapshot, StopReason, ViewCtx,
    Widget, WidgetError,
};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.music");
pub const STATE_VERSION: u32 = 1;

const PREV: LocalId = LocalId(1);
const PLAY: LocalId = LocalId(2);
const NEXT: LocalId = LocalId(3);

const G_PREV: &str = "\u{23ee}";
const G_PLAY: &str = "\u{25b6}";
const G_PAUSE: &str = "\u{23f8}";
const G_NEXT: &str = "\u{23ed}";

pub fn descriptor() -> Descriptor {
    Descriptor {
        type_id: TYPE_ID,
        state_version: STATE_VERSION,
        default_size: Shape::S2X1.size_dip(),
        min_size: Shape::S1X1.size_dip(),
        max_text_bytes: 0,
        chrome: Chrome::Tile,
        shapes: vec![Shape::S1X1, Shape::S2X1, Shape::S2X2, Shape::S4X1, Shape::S4X2],
        default_shape: Some(Shape::S2X1),
    }
}

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    if let Some(s) = restore {
        if s.version > STATE_VERSION {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION });
        }
    }
    Ok(Box::new(Music { media: Reading::Waiting, shape: Shape::S2X1, stopped: false }))
}

/// O que o widget sabe da sessão: ainda não leu, nada tocando, ou uma faixa.
#[derive(Debug, Clone, PartialEq)]
enum Reading {
    Waiting,
    Nothing,
    Track(MediaSnapshot),
}

struct Music {
    media: Reading,
    shape: Shape,
    stopped: bool,
}

fn text(s: &str, style: TextStyle, role: Role, align: Align) -> Node {
    Node::new(Kind::Text { text: s.to_string(), style, role, align, max_lines: 1 }).size(Len::Fill(1), Len::Auto)
}

fn button(id: LocalId, label: &str) -> Node {
    Node::new(Kind::Button { id, label: label.to_string() })
}

fn play_glyph(s: &MediaSnapshot) -> &'static str {
    if s.status == MediaStatus::Playing {
        G_PAUSE
    } else {
        G_PLAY
    }
}

/// Botões que a sessão atual aceita (anterior e próxima só aparecem se o aplicativo permite).
fn controls(s: &MediaSnapshot) -> Node {
    let mut row = Vec::new();
    if s.can_prev {
        row.push(button(PREV, G_PREV));
    }
    if s.can_play_pause {
        row.push(button(PLAY, play_glyph(s)));
    }
    if s.can_next {
        row.push(button(NEXT, G_NEXT));
    }
    Node::row(8.0, 0.0, row).centered().size(Len::Fill(1), Len::Auto)
}

/// Cores do cartão tingido pela capa: a cor média escurecida até o texto branco ter contraste
/// confortável (luminância <= 0,14), mais clara no topo e mais escura na base.
fn tint_colors(cover: &pod_model::image::ImageData) -> ([u8; 3], [u8; 3]) {
    let mut top = cover.average_rgb();
    while crate::ui::theme::luminance(top) > 0.14 {
        top = [(top[0] as f32 * 0.88) as u8, (top[1] as f32 * 0.88) as u8, (top[2] as f32 * 0.88) as u8];
    }
    let bottom = [(top[0] as f32 * 0.7) as u8, (top[1] as f32 * 0.7) as u8, (top[2] as f32 * 0.7) as u8];
    (top, bottom)
}

/// Troca os papéis de texto e barra pelos de "sobre preenchimento colorido".
fn on_tint(n: &mut Node) {
    match &mut n.kind {
        Kind::Text { role, .. } | Kind::Bar { role, .. } => {
            *role = match *role {
                Role::Secondary | Role::Tertiary => Role::OnFillDim,
                _ => Role::OnFill,
            }
        }
        Kind::Stack { children, .. } | Kind::Layers { children } => children.iter_mut().for_each(on_tint),
        Kind::Plate { child, .. } => on_tint(child),
        _ => {}
    }
}

fn view_for(shape: Shape, media: &Reading) -> View {
    let root = match media {
        Reading::Waiting => message(shape, "lendo\u{2026}"),
        Reading::Nothing => message(shape, "Nada tocando"),
        Reading::Track(s) => {
            let mut n = track(shape, s);
            match &s.cover {
                // Com capa, o cartão inteiro ganha a cor dela (texto branco por cima).
                Some(c) => {
                    on_tint(&mut n);
                    let (top, bottom) = tint_colors(c);
                    Node::plate(Fill::Gradient { top, bottom }, crate::render::TILE_RADIUS_DIP, 0.0, n.fill()).size(Len::Fill(1), Len::Fill(1))
                }
                None => n,
            }
        }
    };
    View::new(root.fill())
}

fn message(shape: Shape, msg: &str) -> Node {
    if shape == Shape::S1X1 {
        Node::column(2.0, 6.0, vec![
            text("\u{266a}", TextStyle::Headline, Role::Accent, Align::Center),
            text("M\u{fa}sica", TextStyle::Caption, Role::Secondary, Align::Center),
        ])
        .centered()
    } else {
        Node::column(4.0, 14.0, vec![
            text("\u{266a}", TextStyle::Headline, Role::Accent, Align::Center),
            text(msg, TextStyle::Body, Role::Secondary, Align::Center),
        ])
        .centered()
    }
}

fn text_lines(s: &str, style: TextStyle, role: Role, lines: u8) -> Node {
    Node::new(Kind::Text { text: s.to_string(), style, role, align: Align::Start, max_lines: lines }).size(Len::Fill(1), Len::Auto)
}

fn cover_node(s: &MediaSnapshot, size: f32, radius: f32) -> Option<Node> {
    s.cover.as_ref().map(|c| Node::image(c.clone(), size, radius, "Capa do \u{e1}lbum"))
}

/// Tocar/pausar e próxima: o que cabe ao lado da capa no 2×1.
fn controls_compact(s: &MediaSnapshot) -> Node {
    let mut row = Vec::new();
    if s.can_play_pause {
        row.push(button(PLAY, play_glyph(s)));
    }
    if s.can_next {
        row.push(button(NEXT, G_NEXT));
    }
    Node::row(8.0, 0.0, row).size(Len::Fill(1), Len::Auto)
}

fn track(shape: Shape, s: &MediaSnapshot) -> Node {
    let title = if s.title.trim().is_empty() { "Sem t\u{ed}tulo" } else { s.title.trim() };
    let sub = subtitle(&s.artist, &s.album);
    let line2 = if sub.is_empty() { s.app.clone() } else { sub.clone() };
    match shape {
        sh if sh == Shape::S1X1 => {
            if let Some(img) = cover_node(s, 54.0, 10.0) {
                return Node::column(4.0, 6.0, vec![img, text(title, TextStyle::Caption, Role::Primary, Align::Center)]).centered();
            }
            let glyph = if s.can_play_pause { play_glyph(s) } else { "\u{266a}" };
            let main = if s.can_play_pause {
                button(PLAY, glyph)
            } else {
                text(glyph, TextStyle::Headline, Role::Accent, Align::Center)
            };
            Node::column(4.0, 6.0, vec![main, text(title, TextStyle::Caption, Role::Primary, Align::Center)]).centered()
        }
        sh if sh == Shape::S2X2 => {
            let heading = match cover_node(s, 72.0, 10.0) {
                Some(img) => Node::row(10.0, 0.0, vec![
                    img,
                    Node::column(2.0, 0.0, vec![
                        text_lines(title, TextStyle::Body, Role::Primary, 2),
                        text_lines(&line2, TextStyle::Caption, Role::Secondary, 2),
                    ])
                    .size(Len::Fill(1), Len::Auto),
                ])
                .size(Len::Fill(1), Len::Auto),
                None => {
                    let mut col = vec![text(title, TextStyle::Title, Role::Primary, Align::Start), text(&line2, TextStyle::Body, Role::Secondary, Align::Start)];
                    if !s.app.is_empty() && !sub.is_empty() {
                        col.push(text(&s.app, TextStyle::Caption, Role::Tertiary, Align::Start));
                    }
                    Node::column(2.0, 0.0, col).size(Len::Fill(1), Len::Auto)
                }
            };
            let mut col = vec![heading, Node::spacer()];
            match s.progress() {
                Some(p) => {
                    col.push(Node::bar(p, 6.0, Role::Accent));
                    col.push(Node::row(0.0, 0.0, vec![
                        text(&format_clock(s.position_secs), TextStyle::Caption, Role::Secondary, Align::Start),
                        text(&format_clock(s.duration_secs), TextStyle::Caption, Role::Secondary, Align::End),
                    ])
                    .size(Len::Fill(1), Len::Auto));
                }
                None => col.push(Node::spacer().size(Len::Fill(1), Len::Fixed(6.0))),
            }
            col.push(controls(s));
            Node::column(6.0, 14.0, col)
        }
        _ => {
            if let Some(img) = cover_node(s, 68.0, 10.0) {
                return Node::row(10.0, 10.0, vec![
                    img,
                    Node::column(4.0, 0.0, vec![
                        text(title, TextStyle::Body, Role::Primary, Align::Start),
                        text(&line2, TextStyle::Caption, Role::Secondary, Align::Start),
                        Node::spacer(),
                        controls_compact(s),
                    ])
                    .size(Len::Fill(1), Len::Fill(1)),
                ]);
            }
            Node::column(4.0, 10.0, vec![
                text(title, TextStyle::Title, Role::Primary, Align::Start),
                text(&line2, TextStyle::Caption, Role::Secondary, Align::Start),
                Node::spacer(),
                controls(s),
            ])
        }
    }
}

impl Widget for Music {
    fn handle(&mut self, event: &Event) -> Outcome {
        if self.stopped {
            return Outcome::none();
        }
        match event {
            Event::Started { .. } => Outcome { invalidate: Invalidation::CONTENT, effects: vec![Effect::Media(true)] },
            Event::Media(snap) => {
                let next = match snap {
                    Some(s) => Reading::Track(s.clone()),
                    None => Reading::Nothing,
                };
                if next == self.media {
                    return Outcome::none();
                }
                // A posição muda a cada segundo, mas só o 2×2 mostra progresso: nos outros formatos
                // só uma mudança de faixa ou de estado gera quadro.
                let visible_change = match (&self.media, &next) {
                    (Reading::Track(a), Reading::Track(b)) => !a.same_track(b) || (self.shape == Shape::S2X2 && a != b),
                    _ => true,
                };
                self.media = next;
                if visible_change {
                    Outcome { invalidate: Invalidation::CONTENT, effects: vec![] }
                } else {
                    Outcome::none()
                }
            }
            Event::Clicked { button } => {
                let cmd = match *button {
                    PREV => MediaCommand::Previous,
                    PLAY => MediaCommand::PlayPause,
                    NEXT => MediaCommand::Next,
                    _ => return Outcome::none(),
                };
                Outcome { invalidate: Invalidation::NONE, effects: vec![Effect::MediaCommand(cmd)] }
            }
            Event::ShapeChanged(sh) => {
                self.shape = *sh;
                Outcome { invalidate: Invalidation::CONTENT, effects: vec![] }
            }
            _ => Outcome::none(),
        }
    }

    fn view(&self, _available: SizeDip) -> Element {
        Element::Surface(Vec::new())
    }

    fn tile_view(&self, ctx: &ViewCtx) -> Option<View> {
        Some(view_for(ctx.shape, &self.media))
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { version: STATE_VERSION, data: String::new() }
    }

    fn stop(&mut self, _reason: StopReason) -> Vec<Effect> {
        if self.stopped {
            return Vec::new();
        }
        self.stopped = true;
        self.media = Reading::Waiting; // nao guarda o que estava tocando
        vec![Effect::Media(false)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod_model::layout::{layout, Rect, TextMeasure};
    use pod_model::time::{weekday_of, TimeInfo};

    struct Fake;
    impl TextMeasure for Fake {
        fn measure(&self, text: &str, style: TextStyle, max_w: Option<f32>, _l: u8) -> (f32, f32) {
            let w = text.chars().count() as f32 * style.size_dip() * 0.55;
            (w.min(max_w.unwrap_or(w)), style.line_dip())
        }
    }

    fn ctx(shape: Shape) -> ViewCtx {
        ViewCtx {
            shape,
            time: TimeInfo { year: 2026, month: 10, day: 18, weekday: weekday_of(2026, 10, 18), hour: 10, minute: 0, second: 0, millis: 0 },
        }
    }

    pub(crate) fn demo() -> MediaSnapshot {
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
            cover: None,
        }
    }

    fn media(s: Option<MediaSnapshot>) -> Event {
        Event::Media(s)
    }

    #[test]
    fn tint_is_dark_enough_for_white_text_even_with_a_bright_cover() {
        for (a, b) in [([255u8, 255, 255], [250u8, 250, 240]), ([255, 220, 40], [255, 180, 0]), ([10, 10, 10], [0, 0, 0])] {
            let cover = pod_model::image::ImageData::gradient(32, 32, a, b);
            let (top, bottom) = tint_colors(&cover);
            assert!(crate::ui::theme::contrast(top, [255, 255, 255]) >= 4.5, "{a:?}: {top:?}");
            assert!(crate::ui::theme::luminance(bottom) <= crate::ui::theme::luminance(top));
        }
    }

    #[test]
    fn subscribes_to_media_and_stop_unsubscribes_and_forgets_the_track() {
        let mut w = Music { media: Reading::Waiting, shape: Shape::S2X1, stopped: false };
        assert_eq!(w.handle(&Event::Started { now_ms: 0 }).effects, vec![Effect::Media(true)]);
        w.handle(&media(Some(demo())));
        assert_eq!(w.stop(StopReason::Removed), vec![Effect::Media(false)]);
        assert_eq!(w.media, Reading::Waiting, "nao guarda a faixa");
        assert!(w.stop(StopReason::Removed).is_empty());
        assert_eq!(w.handle(&media(Some(demo()))), Outcome::none());
    }

    #[test]
    fn buttons_become_media_commands() {
        let mut w = Music { media: Reading::Track(demo()), shape: Shape::S2X1, stopped: false };
        for (id, cmd) in [(PREV, MediaCommand::Previous), (PLAY, MediaCommand::PlayPause), (NEXT, MediaCommand::Next)] {
            let o = w.handle(&Event::Clicked { button: id });
            assert_eq!(o.effects, vec![Effect::MediaCommand(cmd)]);
            assert_eq!(o.invalidate, Invalidation::NONE, "o estado novo vem da sessao, nao do clique");
        }
        assert_eq!(w.handle(&Event::Clicked { button: LocalId(9) }), Outcome::none());
    }

    #[test]
    fn position_only_redraws_the_big_format() {
        let tick = |p: u32| {
            let mut s = demo();
            s.position_secs = Some(p);
            media(Some(s))
        };
        let mut small = Music { media: Reading::Track(demo()), shape: Shape::S2X1, stopped: false };
        assert_eq!(small.handle(&tick(95)), Outcome::none(), "2x1 ignora a posicao");
        let mut big = Music { media: Reading::Track(demo()), shape: Shape::S2X2, stopped: false };
        assert_eq!(big.handle(&tick(95)).invalidate, Invalidation::CONTENT, "2x2 mostra o progresso");
        // Mudar de faixa ou pausar redesenha em qualquer formato.
        let mut paused = demo();
        paused.status = MediaStatus::Paused;
        assert_eq!(small.handle(&media(Some(paused))).invalidate, Invalidation::CONTENT);
        assert_eq!(small.handle(&media(None)).invalidate, Invalidation::CONTENT, "parou de tocar");
        assert_eq!(small.handle(&media(None)), Outcome::none(), "igual ao anterior");
    }

    #[test]
    fn unavailable_controls_are_left_out_and_the_toggle_follows_the_state() {
        let mut s = demo();
        s.can_prev = false;
        s.can_next = false;
        let v = view_for(Shape::S2X1, &Reading::Track(s.clone()));
        let ids = |v: &View| {
            fn walk(n: &Node, out: &mut Vec<u32>) {
                if let Kind::Button { id, .. } = &n.kind {
                    out.push(id.0);
                }
                n.children().iter().for_each(|c| walk(c, out));
            }
            let mut out = Vec::new();
            walk(&v.root, &mut out);
            out
        };
        assert_eq!(ids(&v), vec![2], "so tocar/pausar");
        let full = view_for(Shape::S2X1, &Reading::Track(demo()));
        assert_eq!(ids(&full), vec![1, 2, 3]);
        assert_eq!(play_glyph(&demo()), G_PAUSE);
        s.status = MediaStatus::Paused;
        assert_eq!(play_glyph(&s), G_PLAY);
    }

    #[test]
    fn the_cover_shows_in_every_shape_with_alt_text_and_the_compact_controls() {
        fn walk(n: &Node, imgs: &mut usize, btns: &mut Vec<u32>) {
            match &n.kind {
                Kind::Image { label, .. } => {
                    assert_eq!(label, "Capa do \u{e1}lbum");
                    *imgs += 1;
                }
                Kind::Button { id, .. } => btns.push(id.0),
                _ => {}
            }
            n.children().iter().for_each(|c| walk(c, imgs, btns));
        }
        let mut s = demo();
        s.cover = Some(pod_model::image::ImageData::gradient(32, 32, [200, 50, 80], [30, 20, 90]));
        for (shape, buttons) in [(Shape::S1X1, vec![]), (Shape::S2X1, vec![2, 3]), (Shape::S2X2, vec![1, 2, 3])] {
            let v = view_for(shape, &Reading::Track(s.clone()));
            assert_eq!(v.validate(), Ok(()));
            let (mut imgs, mut btns) = (0, Vec::new());
            walk(&v.root, &mut imgs, &mut btns);
            assert_eq!(imgs, 1, "{shape}: uma capa");
            assert_eq!(btns, buttons, "{shape}");
        }
        // Sem capa nao ha imagem.
        let (mut imgs, mut btns) = (0, Vec::new());
        walk(&view_for(Shape::S2X2, &Reading::Track(demo())).root, &mut imgs, &mut btns);
        assert_eq!(imgs, 0);
    }

    #[test]
    fn every_state_and_shape_fits_the_tile() {
        let mut long = demo();
        long.title = "Um t\u{ed}tulo realmente muito longo de faixa que n\u{e3}o cabe".into();
        long.artist = "Artista com nome comprido demais".into();
        let mut bare = demo();
        bare.title = String::new();
        bare.artist = String::new();
        bare.album = String::new();
        bare.position_secs = None;
        bare.duration_secs = None;
        bare.can_prev = false;
        let mut covered = demo();
        covered.cover = Some(pod_model::image::ImageData::gradient(32, 32, [200, 50, 80], [30, 20, 90]));
        let mut covered_long = long.clone();
        covered_long.cover = covered.cover.clone();
        let readings = [
            Reading::Waiting,
            Reading::Nothing,
            Reading::Track(demo()),
            Reading::Track(long),
            Reading::Track(bare),
            Reading::Track(covered),
            Reading::Track(covered_long),
        ];
        for r in readings {
            for shape in descriptor().shapes {
                let v = view_for(shape, &r);
                assert_eq!(v.validate(), Ok(()), "{shape} {r:?}");
                let s = shape.size_dip();
                let laid = layout(&v, Rect::new(0.0, 0.0, s.w, s.h), &Fake);
                for l in &laid {
                    assert!(laid[0].rect.contains(&l.rect), "{shape} {r:?}: {:?}", l.rect);
                }
            }
        }
        let w = Music { media: Reading::Waiting, shape: Shape::S2X1, stopped: false };
        assert!(w.tile_view(&ctx(Shape::S2X1)).is_some());
    }

    #[test]
    fn future_state_is_refused() {
        assert!(matches!(
            create(Some(&Snapshot { version: 2, data: String::new() })),
            Err(WidgetError::UnsupportedVersion { .. })
        ));
    }
}
