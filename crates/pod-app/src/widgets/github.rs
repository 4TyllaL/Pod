//! Widget GitHub: notificações não lidas, PRs que pedem sua revisão e seus PRs abertos, em 1×1, 2×1 e
//! 2×2. **Usa a internet** (só com "Widgets de internet" ligado) e um token do GitHub guardado no
//! Gerenciador de Credenciais do Windows: o campo "Conectar" é mascarado e o texto vai direto ao cofre,
//! **nunca ao widget** (que não guarda nem salva segredo; o estado durável é vazio).

use pod_model::github::{GithubState, Summary};
use pod_model::ids::WidgetTypeId;
use pod_model::shape::Shape;
use pod_model::view::{Align, Kind, Len, Node, Role, TextStyle, View};
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, InputSpec, Invalidation, LocalId, MenuItem, Outcome, SizeDip, Snapshot,
    StopReason, ViewCtx, Widget, WidgetError,
};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.github");
pub const STATE_VERSION: u32 = 1;

const CONNECT: LocalId = LocalId(1);
const CONNECT_CMD: u32 = 50;
const FORGET_CMD: u32 = 51;

pub fn descriptor() -> Descriptor {
    Descriptor {
        type_id: TYPE_ID,
        state_version: STATE_VERSION,
        default_size: Shape::S2X2.size_dip(),
        min_size: Shape::S1X1.size_dip(),
        max_text_bytes: 0,
        chrome: Chrome::Tile,
        shapes: vec![Shape::S1X1, Shape::S2X1, Shape::S2X2, Shape::S4X1, Shape::S4X2],
        default_shape: Some(Shape::S2X2),
    }
}

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    if let Some(s) = restore {
        if s.version > STATE_VERSION {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION });
        }
    }
    Ok(Box::new(Github { state: GithubState::Loading, stopped: false }))
}

struct Github {
    state: GithubState,
    stopped: bool,
}

fn text(s: &str, style: TextStyle, role: Role, align: Align, lines: u8) -> Node {
    Node::new(Kind::Text { text: s.to_string(), style, role, align, max_lines: lines }).size(Len::Fill(1), Len::Auto)
}

fn unread_text(s: &Summary) -> String {
    if s.unread_capped { format!("{}+", s.unread) } else { s.unread.to_string() }
}

/// Um número grande com legenda embaixo.
fn stat(n: &str, label: &str, role: Role) -> Node {
    Node::column(1.0, 0.0, vec![
        text(n, TextStyle::Headline, role, Align::Center, 1),
        text(label, TextStyle::Caption, Role::Secondary, Align::Center, 1),
    ])
    .size(Len::Fill(1), Len::Auto)
}

fn ready_view(shape: Shape, s: &Summary) -> Node {
    let attention = if s.review_requests > 0 { Role::Accent } else { Role::Primary };
    match shape {
        sh if sh == Shape::S1X1 => Node::column(2.0, 6.0, vec![
            text("GITHUB", TextStyle::Caption, Role::Secondary, Align::Center, 1),
            text(&s.review_requests.to_string(), TextStyle::Headline, attention, Align::Center, 1),
            text("p/ revisar", TextStyle::Caption, Role::Secondary, Align::Center, 1),
        ])
        .centered(),
        sh if sh == Shape::S2X2 => {
            let mut col = vec![
                text(&format!("GitHub \u{b7} {}", s.login), TextStyle::Caption, Role::Secondary, Align::Start, 1),
                Node::row(8.0, 0.0, vec![
                    stat(&unread_text(s), "n\u{e3}o lidas", Role::Primary),
                    stat(&s.review_requests.to_string(), "p/ revisar", attention),
                    stat(&s.my_pulls.to_string(), "meus PRs", Role::Primary),
                ])
                .size(Len::Fill(1), Len::Auto),
            ];
            for p in s.reviews.iter().take(2) {
                col.push(text(&format!("{} \u{b7} {}", p.repo, p.title), TextStyle::Caption, Role::Primary, Align::Start, 1));
            }
            Node::column(6.0, 14.0, col)
        }
        _ => Node::row(8.0, 10.0, vec![
            stat(&unread_text(s), "n\u{e3}o lidas", Role::Primary),
            stat(&s.review_requests.to_string(), "p/ revisar", attention),
            stat(&s.my_pulls.to_string(), "meus PRs", Role::Primary),
        ])
        .centered(),
    }
}

fn message_view(shape: Shape, state: &GithubState) -> Node {
    let (main, hint, connect) = match state {
        GithubState::Off => ("Internet desligada", "Ligue \"Widgets de internet\" no menu da bandeja.", false),
        GithubState::NoToken => ("Conecte ao GitHub", "O token fica no Gerenciador de Credenciais do Windows.", true),
        GithubState::Loading => ("Buscando\u{2026}", "", false),
        GithubState::Rejected => ("Token recusado", "Conecte de novo com um token v\u{e1}lido.", true),
        GithubState::Failed(m) => (m.as_str(), "Tentarei de novo em 1 minuto.", false),
        GithubState::Ready(_) => ("", "", false),
    };
    if shape == Shape::S1X1 {
        return Node::column(2.0, 6.0, vec![
            text("GITHUB", TextStyle::Caption, Role::Secondary, Align::Center, 1),
            text(if connect { "conectar" } else { main }, TextStyle::Caption, Role::Primary, Align::Center, 2),
        ])
        .centered();
    }
    let mut col = vec![text(main, TextStyle::Body, Role::Primary, Align::Center, 2)];
    if !hint.is_empty() && shape == Shape::S2X2 {
        col.push(text(hint, TextStyle::Caption, Role::Secondary, Align::Center, 3));
    }
    if connect {
        col.push(Node::new(Kind::Button { id: CONNECT, label: "Conectar".into() }));
    }
    Node::column(6.0, 12.0, col).centered()
}

fn view_for(shape: Shape, state: &GithubState) -> View {
    let root = match state {
        GithubState::Ready(s) => ready_view(shape, s),
        other => message_view(shape, other),
    };
    View::new(root.fill())
}

impl Github {
    fn connect(&self) -> Outcome {
        Outcome {
            invalidate: Invalidation::NONE,
            effects: vec![Effect::OpenInput(InputSpec {
                title: "GitHub".into(),
                label: "Token de acesso (fica no cofre do Windows)".into(),
                initial: String::new(),
                secret: true,
            })],
        }
    }
}

impl Widget for Github {
    fn handle(&mut self, event: &Event) -> Outcome {
        if self.stopped {
            return Outcome::none();
        }
        match event {
            Event::Started { .. } => Outcome { invalidate: Invalidation::CONTENT, effects: vec![Effect::Github(true)] },
            Event::Github(s) => {
                if *s == self.state {
                    return Outcome::none();
                }
                self.state = s.clone();
                Outcome { invalidate: Invalidation::CONTENT, effects: vec![] }
            }
            Event::Clicked { button } if *button == CONNECT => self.connect(),
            Event::MenuCommand(CONNECT_CMD) => self.connect(),
            Event::MenuCommand(FORGET_CMD) => {
                Outcome { invalidate: Invalidation::CONTENT, effects: vec![Effect::ForgetToken] }
            }
            Event::ShapeChanged(_) => Outcome { invalidate: Invalidation::CONTENT, effects: vec![] },
            _ => Outcome::none(),
        }
    }

    fn view(&self, _available: SizeDip) -> Element {
        Element::Surface(Vec::new())
    }

    fn tile_view(&self, ctx: &ViewCtx) -> Option<View> {
        Some(view_for(ctx.shape, &self.state))
    }

    fn menu_items(&self) -> Vec<MenuItem> {
        vec![
            MenuItem { id: CONNECT_CMD, label: "Conectar / trocar token\u{2026}".into(), checked: false, group: None },
            MenuItem { id: FORGET_CMD, label: "Esquecer token".into(), checked: false, group: None },
        ]
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { version: STATE_VERSION, data: String::new() }
    }

    fn stop(&mut self, _reason: StopReason) -> Vec<Effect> {
        if self.stopped {
            return Vec::new();
        }
        self.stopped = true;
        self.state = GithubState::Loading; // esquece o resumo
        vec![Effect::Github(false)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod_model::github::Pull;

    pub(crate) fn demo() -> GithubState {
        GithubState::Ready(Summary {
            login: "octo".into(),
            unread: 30,
            unread_capped: true,
            review_requests: 2,
            my_pulls: 3,
            reviews: vec![
                Pull { title: "Corrige o vazamento de mem\u{f3}ria".into(), repo: "octo/hello".into() },
                Pull { title: "Atualiza depend\u{ea}ncias".into(), repo: "octo/tools".into() },
            ],
        })
    }

    fn started() -> Box<dyn Widget> {
        let mut w = create(None).unwrap();
        w.handle(&Event::Started { now_ms: 0 });
        w
    }

    #[test]
    fn starting_subscribes_and_stopping_unsubscribes() {
        let mut w = create(None).unwrap();
        assert_eq!(w.handle(&Event::Started { now_ms: 0 }).effects, vec![Effect::Github(true)]);
        assert_eq!(w.stop(StopReason::Removed), vec![Effect::Github(false)]);
        assert!(w.stop(StopReason::Removed).is_empty());
    }

    #[test]
    fn connecting_asks_for_a_secret_that_never_reaches_the_widget() {
        let mut w = started();
        for e in [Event::Clicked { button: CONNECT }, Event::MenuCommand(CONNECT_CMD)] {
            match w.handle(&e).effects.as_slice() {
                [Effect::OpenInput(spec)] => assert!(spec.secret && spec.initial.is_empty()),
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(w.handle(&Event::MenuCommand(FORGET_CMD)).effects, vec![Effect::ForgetToken]);
    }

    #[test]
    fn the_saved_state_is_empty_and_holds_no_secret() {
        let mut w = started();
        w.handle(&Event::Github(demo()));
        assert!(w.snapshot().data.is_empty());
        assert!(create(Some(&Snapshot { version: 2, data: String::new() })).is_err());
    }

    #[test]
    fn every_state_has_a_view_in_every_shape() {
        let states = [
            GithubState::Off,
            GithubState::NoToken,
            GithubState::Loading,
            GithubState::Rejected,
            GithubState::Failed("Sem conex\u{e3}o".into()),
            demo(),
        ];
        for st in &states {
            for shape in [Shape::S1X1, Shape::S2X1, Shape::S2X2] {
                let v = view_for(shape, st);
                assert!(v.validate().is_ok(), "{st:?} {shape:?}");
            }
        }
        let GithubState::Ready(s) = demo() else { unreachable!() };
        assert_eq!(unread_text(&s), "30+");
    }

    #[test]
    fn an_unchanged_state_does_not_repaint() {
        let mut w = started();
        assert_eq!(w.handle(&Event::Github(demo())).invalidate, Invalidation::CONTENT);
        assert_eq!(w.handle(&Event::Github(demo())).invalidate, Invalidation::NONE);
    }
}
