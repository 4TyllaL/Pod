//! Quick Notes: uma nota de texto simples por instância. Não conhece janelas, arquivos nem
//! editor: recebe `TextChanged`, devolve estado e efeitos. Persistência e movimentação são do
//! runtime e do shell.

use pod_model::ids::WidgetTypeId;
use pod_model::widget::{
    Chrome, Descriptor, Effect, Element, Event, Invalidation, LocalId, Outcome, SizeDip, Snapshot, StopReason,
    Widget, WidgetError,
};

pub const TYPE_ID: WidgetTypeId = WidgetTypeId("pod.quick-notes");
pub const STATE_VERSION: u32 = 1;
pub const FIELD: LocalId = LocalId(1);
/// Teto de texto UTF-8 por nota (ARCHITECTURE §7). Política ajustável, não requisito do formato.
pub const MAX_TEXT_BYTES: usize = 256 * 1024;

pub fn descriptor() -> Descriptor {
    Descriptor {
        type_id: TYPE_ID,
        state_version: STATE_VERSION,
        default_size: SizeDip { w: 320.0, h: 240.0 },
        min_size: SizeDip { w: 160.0, h: 120.0 },
        chrome: Chrome::Note,
        shapes: Vec::new(),
        default_shape: None,
        max_text_bytes: MAX_TEXT_BYTES,
    }
}

pub fn create(restore: Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError> {
    let text = match restore {
        None => String::new(),
        Some(s) if s.version > STATE_VERSION => {
            return Err(WidgetError::UnsupportedVersion { found: s.version, supported: STATE_VERSION })
        }
        Some(s) if s.data.len() > MAX_TEXT_BYTES => {
            return Err(WidgetError::InvalidState(format!(
                "texto de {} bytes acima do limite de {MAX_TEXT_BYTES}",
                s.data.len()
            )))
        }
        Some(s) => s.data.clone(),
    };
    Ok(Box::new(QuickNotes { text, limit_hit: false, stopped: false }))
}

struct QuickNotes {
    /// Durável.
    text: String,
    /// Transitório: a última edição foi recusada por exceder o limite.
    limit_hit: bool,
    stopped: bool,
}

impl Widget for QuickNotes {
    fn handle(&mut self, event: &Event) -> Outcome {
        if self.stopped {
            return Outcome::none();
        }
        match event {
            Event::TextChanged { field, text } if *field == FIELD => {
                if text.len() > MAX_TEXT_BYTES {
                    // Sem truncamento silencioso: o texto anterior volta ao campo e o erro aparece.
                    self.limit_hit = true;
                    return Outcome {
                        invalidate: Invalidation::LAYOUT,
                        effects: vec![Effect::SetText { field: FIELD, text: self.text.clone() }],
                    };
                }
                if *text == self.text && !self.limit_hit {
                    return Outcome::none(); // nada mudou de fato
                }
                let layout_changed = self.limit_hit;
                self.limit_hit = false;
                self.text = text.clone();
                Outcome {
                    invalidate: if layout_changed { Invalidation::LAYOUT } else { Invalidation::CONTENT },
                    effects: vec![Effect::StateChanged],
                }
            }
            _ => Outcome::none(),
        }
    }

    fn view(&self, _available: SizeDip) -> Element {
        let mut children = vec![Element::TextField { id: FIELD, text: self.text.clone() }];
        if self.limit_hit {
            children.push(Element::Label(format!(
                "Limite de {} KiB: edição recusada.",
                MAX_TEXT_BYTES / 1024
            )));
        }
        Element::Surface(children)
    }

    fn has_content(&self) -> bool {
        !self.text.trim().is_empty()
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { version: STATE_VERSION, data: self.text.clone() }
    }

    fn stop(&mut self, _reason: StopReason) -> Vec<Effect> {
        self.stopped = true;
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(text: &str) -> Box<dyn Widget> {
        create(Some(&Snapshot { version: 1, data: text.into() })).unwrap()
    }

    fn typed(w: &mut Box<dyn Widget>, text: &str) -> Outcome {
        w.handle(&Event::TextChanged { field: FIELD, text: text.into() })
    }

    #[test]
    fn starts_empty_and_without_content() {
        let w = create(None).unwrap();
        assert_eq!(w.snapshot().data, "");
        assert!(!w.has_content());
    }

    #[test]
    fn typing_updates_the_snapshot_and_asks_to_persist() {
        let mut w = create(None).unwrap();
        let out = typed(&mut w, "Olá, mundo 😀");
        assert_eq!(w.snapshot().data, "Olá, mundo 😀");
        assert_eq!(out.effects, vec![Effect::StateChanged]);
        assert_eq!(out.invalidate, Invalidation::CONTENT);
        assert!(w.has_content());
    }

    #[test]
    fn identical_text_does_nothing() {
        let mut w = note("igual");
        assert_eq!(typed(&mut w, "igual"), Outcome::none());
    }

    #[test]
    fn whitespace_only_is_not_content_worth_confirming() {
        let w = note("  \n\t ");
        assert!(!w.has_content());
    }

    #[test]
    fn over_the_limit_is_rejected_not_truncated() {
        let mut w = note("antes");
        let too_big = "x".repeat(MAX_TEXT_BYTES + 1);
        let out = typed(&mut w, &too_big);
        assert_eq!(w.snapshot().data, "antes", "estado não muda");
        assert_eq!(out.effects, vec![Effect::SetText { field: FIELD, text: "antes".into() }]);
        assert!(!out.effects.contains(&Effect::StateChanged), "nada a persistir");
        // O erro fica visível na view.
        let Element::Surface(kids) = w.view(SizeDip { w: 320.0, h: 240.0 }) else { panic!() };
        assert!(kids.iter().any(|k| matches!(k, Element::Label(t) if t.contains("Limite"))));
    }

    #[test]
    fn the_limit_counts_utf8_bytes_not_characters() {
        let mut w = create(None).unwrap();
        let exactly = "é".repeat(MAX_TEXT_BYTES / 2); // 2 bytes cada
        assert_eq!(exactly.len(), MAX_TEXT_BYTES);
        assert_eq!(typed(&mut w, &exactly).effects, vec![Effect::StateChanged]);
        let one_more = format!("{exactly}é");
        assert!(matches!(typed(&mut w, &one_more).effects[0], Effect::SetText { .. }));
    }

    #[test]
    fn a_valid_edit_clears_the_error() {
        let mut w = note("a");
        typed(&mut w, &"x".repeat(MAX_TEXT_BYTES + 1));
        let out = typed(&mut w, "b");
        assert_eq!(out.invalidate, Invalidation::LAYOUT, "a linha de erro some");
        let Element::Surface(kids) = w.view(SizeDip { w: 1.0, h: 1.0 }) else { panic!() };
        assert_eq!(kids.len(), 1);
    }

    #[test]
    fn restore_validates_version_and_size() {
        assert_eq!(
            create(Some(&Snapshot { version: 9, data: String::new() })).err(),
            Some(WidgetError::UnsupportedVersion { found: 9, supported: 1 })
        );
        let huge = Snapshot { version: 1, data: "x".repeat(MAX_TEXT_BYTES + 1) };
        assert!(matches!(create(Some(&huge)).err(), Some(WidgetError::InvalidState(_))));
    }

    #[test]
    fn ignores_events_for_other_fields_and_after_stop() {
        let mut w = note("a");
        assert_eq!(
            w.handle(&Event::TextChanged { field: LocalId(99), text: "z".into() }),
            Outcome::none()
        );
        w.stop(StopReason::Removed);
        assert_eq!(typed(&mut w, "depois do stop"), Outcome::none());
        assert_eq!(w.snapshot().data, "a");
    }
}
