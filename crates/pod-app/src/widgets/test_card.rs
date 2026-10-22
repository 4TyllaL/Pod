//! Widget fictício de teste: um contador clicável com um relógio de 1 s. Existe para provar que o
//! host e o runtime funcionam sem nenhuma lógica de Quick Notes.

use pod_model::ids::WidgetTypeId;
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, LocalId, Outcome, SizeDip, Snapshot, StopReason,
    Widget, WidgetError,
};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.test-card");
pub const STATE_VERSION: u32 = 1;
pub const BUTTON: LocalId = LocalId(1);
const TICK_TOKEN: u64 = 1;
const TICK_MS: u64 = 1000;

pub fn descriptor() -> Descriptor {
    Descriptor {
        type_id: TYPE_ID,
        state_version: STATE_VERSION,
        default_size: SizeDip { w: 320.0, h: 240.0 },
        min_size: SizeDip { w: 160.0, h: 120.0 },
        chrome: Chrome::Note,
        shapes: Vec::new(),
        default_shape: None,
        max_text_bytes: 0,
    }
}

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    let clicks = match restore {
        None => 0,
        Some(s) if s.version > STATE_VERSION => {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION })
        }
        Some(s) => s
            .data
            .parse::<u32>()
            .map_err(|e| WidgetError::InvalidState(format!("contador inválido: {e}")))?,
    };
    Ok(Box::new(TestCard { clicks, ticks: 0, stopped: false }))
}

struct TestCard {
    /// Durável.
    clicks: u32,
    /// Transitório: não entra no snapshot.
    ticks: u32,
    stopped: bool,
}

impl Widget for TestCard {
    fn handle(&mut self, event: &Event) -> Outcome {
        if self.stopped {
            return Outcome::none();
        }
        match event {
            Event::Started { now_ms } => Outcome {
                invalidate: Invalidation::CONTENT,
                effects: vec![Effect::ScheduleAt { deadline_ms: now_ms + TICK_MS, token: TICK_TOKEN }],
            },
            Event::Clicked { button } if *button == BUTTON => {
                self.clicks += 1;
                Outcome { invalidate: Invalidation::CONTENT, effects: vec![Effect::StateChanged] }
            }
            Event::Timer { token: TICK_TOKEN, now_ms } => {
                self.ticks += 1;
                Outcome {
                    invalidate: Invalidation::CONTENT,
                    effects: vec![Effect::ScheduleAt { deadline_ms: now_ms + TICK_MS, token: TICK_TOKEN }],
                }
            }
            _ => Outcome::none(),
        }
    }

    fn view(&self, _available: SizeDip) -> Element {
        Element::Surface(vec![
            Element::Label(format!("cliques: {}", self.clicks)),
            Element::Label(format!("segundos: {}", self.ticks)),
            Element::Button { id: BUTTON, label: "+1".to_string() },
        ])
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { version: STATE_VERSION, data: self.clicks.to_string() }
    }

    fn stop(&mut self, _reason: StopReason) -> Vec<Effect> {
        if self.stopped {
            return Vec::new();
        }
        self.stopped = true;
        vec![Effect::CancelTimer { token: TICK_TOKEN }]
    }
}
