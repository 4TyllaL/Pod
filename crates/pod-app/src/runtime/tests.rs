use super::*;
use crate::widgets::{self, test_card};
use pod_model::ids::WidgetId;
use pod_model::widget::{Event, Phase, Snapshot, StopReason, WidgetError};

/// IDs sequenciais; `repeat_first` força uma colisão para testar o laço de unicidade.
struct SeqIds {
    next: u128,
    repeat_first: bool,
    emitted_first: bool,
}

impl SeqIds {
    fn new() -> Self {
        Self { next: 1, repeat_first: false, emitted_first: false }
    }
}

impl IdSource for SeqIds {
    fn next_id(&mut self) -> WidgetId {
        if self.repeat_first && self.emitted_first && self.next == 2 {
            self.next += 1;
            return WidgetId(1); // colide com o primeiro
        }
        self.emitted_first = true;
        let id = WidgetId(self.next);
        self.next += 1;
        id
    }
}

fn runtime() -> Runtime {
    let mut rt = Runtime::new(Box::new(SeqIds::new()));
    widgets::register_all(&mut rt);
    rt
}

fn running(rt: &mut Runtime, now: u64) -> WidgetId {
    let id = rt.create(&test_card::TYPE_ID, None).unwrap();
    rt.mount(id).unwrap();
    rt.start(id, now).unwrap();
    id
}

fn click() -> Event {
    Event::Clicked { button: test_card::BUTTON }
}

#[test]
fn lifecycle_requires_mount_before_running() {
    let mut rt = runtime();
    let id = rt.create(&test_card::TYPE_ID, None).unwrap();
    assert_eq!(rt.phase(id), Some(Phase::Loaded));
    assert!(matches!(rt.start(id, 0), Err(RuntimeError::WrongPhase { phase: Phase::Loaded, .. })));
    rt.mount(id).unwrap();
    rt.start(id, 0).unwrap();
    assert_eq!(rt.phase(id), Some(Phase::Running));
    assert!(rt.mount(id).is_err(), "não volta de Running para Mounted");
}

#[test]
fn events_before_running_are_rejected() {
    let mut rt = runtime();
    let id = rt.create(&test_card::TYPE_ID, None).unwrap();
    assert!(matches!(rt.dispatch(id, &click()), Err(RuntimeError::WrongPhase { .. })));
    rt.mount(id).unwrap();
    assert!(matches!(rt.dispatch(id, &click()), Err(RuntimeError::WrongPhase { .. })));
}

#[test]
fn unknown_type_and_failed_factory_leave_nothing_behind() {
    let mut rt = runtime();
    assert!(matches!(
        rt.create(&pod_model::ids::WidgetTypeId("pod.nope"), None),
        Err(RuntimeError::UnknownType(_))
    ));
    let bad = Snapshot { version: 1, data: "não é número".into() };
    assert!(matches!(
        rt.create(&test_card::TYPE_ID, Some(&bad)),
        Err(RuntimeError::Widget(WidgetError::InvalidState(_)))
    ));
    let newer = Snapshot { version: 99, data: "3".into() };
    assert!(matches!(
        rt.create(&test_card::TYPE_ID, Some(&newer)),
        Err(RuntimeError::Widget(WidgetError::UnsupportedVersion { found: 99, supported: 1 }))
    ));
    assert_eq!(rt.len(), 0);
}

#[test]
fn ids_are_never_duplicated_even_when_the_source_collides() {
    let mut rt = Runtime::new(Box::new(SeqIds { next: 1, repeat_first: true, emitted_first: false }));
    widgets::register_all(&mut rt);
    let a = rt.create(&test_card::TYPE_ID, None).unwrap();
    let b = rt.create(&test_card::TYPE_ID, None).unwrap();
    assert_ne!(a, b);
    assert_eq!(rt.len(), 2);
}

#[test]
fn random_ids_are_distinct() {
    let mut src = RandomIds::new();
    let ids: std::collections::BTreeSet<_> = (0..10_000).map(|_| src.next_id()).collect();
    assert_eq!(ids.len(), 10_000);
}

#[test]
fn an_event_for_one_instance_never_changes_another() {
    let mut rt = runtime();
    let a = running(&mut rt, 0);
    let b = running(&mut rt, 0);
    for _ in 0..3 {
        rt.dispatch(a, &click()).unwrap();
    }
    assert_eq!(rt.snapshot(a).unwrap().1.data, "3");
    assert_eq!(rt.snapshot(b).unwrap().1.data, "0");
    // A invalidação também é por instância.
    let inv: Vec<_> = rt.take_invalidated().into_iter().map(|(id, _)| id).collect();
    assert!(inv.contains(&a));
    assert!(rt.take_invalidated().is_empty(), "take_invalidated esvazia o acumulado");
}

#[test]
fn state_changed_effect_reaches_the_shell_with_the_right_id() {
    let mut rt = runtime();
    let a = running(&mut rt, 0);
    let b = running(&mut rt, 0);
    assert_eq!(rt.dispatch(b, &click()).unwrap(), vec![Command::StateChanged(b)]);
    assert_eq!(rt.dispatch(a, &click()).unwrap(), vec![Command::StateChanged(a)]);
}

#[test]
fn restore_roundtrips_durable_state_but_not_transient() {
    let mut rt = runtime();
    let a = running(&mut rt, 0);
    rt.dispatch(a, &click()).unwrap();
    rt.tick(1000); // ticks são transitórios
    let (_, snap) = rt.snapshot(a).unwrap();
    let b = rt.create(&test_card::TYPE_ID, Some(&snap)).unwrap();
    assert_eq!(rt.snapshot(b).unwrap().1, snap);
    assert_ne!(a, b, "restaurar cria ID novo de instância na sessão do teste");
}

#[test]
fn timers_fire_once_at_their_deadline() {
    let mut rt = runtime();
    let a = running(&mut rt, 100); // agenda para 1100
    assert_eq!(rt.next_deadline(), Some(1100));
    rt.tick(1099);
    assert_eq!(rt.snapshot(a).unwrap().1.data, "0");
    assert_eq!(rt.next_deadline(), Some(1100), "ainda não venceu");
    rt.tick(1100);
    assert_eq!(rt.next_deadline(), Some(2100), "o widget reagendou a partir de now");
}

#[test]
fn resume_after_long_suspension_fires_once_not_a_storm() {
    let mut rt = runtime();
    running(&mut rt, 0);
    // Uma hora depois: um único disparo, não 3600.
    rt.tick(3_600_000);
    assert_eq!(rt.pending_timers(), 1);
    assert_eq!(rt.next_deadline(), Some(3_601_000));
}

#[test]
fn removing_cancels_timers_and_late_results_are_ignored() {
    let mut rt = runtime();
    let a = running(&mut rt, 0);
    assert_eq!(rt.pending_timers(), 1);
    rt.remove(a, StopReason::Removed);
    assert_eq!(rt.pending_timers(), 0);
    assert!(rt.tick(10_000).is_empty());
    assert!(matches!(rt.dispatch(a, &click()), Err(RuntimeError::UnknownInstance(_))));
}

#[test]
fn a_timer_from_a_removed_instance_never_reaches_a_new_one() {
    let mut rt = runtime();
    let a = running(&mut rt, 0);
    // Simula um resultado atrasado: o prazo da geração antiga continua no agendador.
    let old_generation = 1;
    rt.scheduler.schedule(a, old_generation, 7, 500);
    rt.remove(a, StopReason::Removed);
    rt.scheduler.schedule(a, old_generation, 7, 500); // reaparece depois da remoção
    let b = running(&mut rt, 0);
    rt.tick(2000);
    // b só recebeu o próprio tick (token 1); nada do token 7 da geração antiga.
    assert_eq!(rt.snapshot(b).unwrap().1.data, "0");
    assert_eq!(rt.pending_timers(), 1, "só o prazo vivo de b permanece");
}

#[test]
fn remove_is_idempotent() {
    let mut rt = runtime();
    let a = running(&mut rt, 0);
    rt.remove(a, StopReason::Removed);
    assert!(rt.remove(a, StopReason::Removed).is_empty());
    assert_eq!(rt.len(), 0);
}

#[test]
fn hiding_is_a_flag_not_a_phase_and_keeps_state() {
    let mut rt = runtime();
    let a = running(&mut rt, 0);
    rt.dispatch(a, &click()).unwrap();
    rt.set_visible(a, false).unwrap();
    assert_eq!(rt.phase(a), Some(Phase::Running));
    assert_eq!(rt.is_visible(a), Some(false));
    rt.dispatch(a, &click()).unwrap(); // continua recebendo eventos
    rt.set_visible(a, true).unwrap();
    assert_eq!(rt.snapshot(a).unwrap().1.data, "2");
}

#[test]
fn repeated_create_and_remove_leaves_no_residue() {
    let mut rt = runtime();
    let keep = running(&mut rt, 0);
    for round in 0..2_000u64 {
        let id = running(&mut rt, round);
        rt.dispatch(id, &click()).unwrap();
        rt.remove(id, StopReason::Removed);
    }
    assert_eq!(rt.len(), 1);
    assert_eq!(rt.pending_timers(), 1, "só o timer da instância que ficou");
    assert!(rt.take_invalidated().iter().all(|(id, _)| *id == keep));
}

#[test]
fn shutdown_stops_everything() {
    let mut rt = runtime();
    for _ in 0..5 {
        running(&mut rt, 0);
    }
    rt.shutdown();
    assert_eq!(rt.len(), 0);
    assert_eq!(rt.pending_timers(), 0);
}

#[test]
fn view_uses_only_supported_elements() {
    let mut rt = runtime();
    let a = running(&mut rt, 0);
    let view = rt.view(a, pod_model::widget::SizeDip { w: 320.0, h: 240.0 }).unwrap();
    assert!(matches!(view, pod_model::widget::Element::Surface(ref kids) if kids.len() == 3));
}

#[test]
fn restore_keeps_the_persisted_id_and_refuses_duplicates() {
    let mut rt = runtime();
    let snap = Snapshot { version: 1, data: "5".into() };
    rt.restore(WidgetId(77), &test_card::TYPE_ID, &snap).unwrap();
    assert_eq!(rt.snapshot(WidgetId(77)).unwrap().1.data, "5");
    assert_eq!(rt.restore(WidgetId(77), &test_card::TYPE_ID, &snap), Err(RuntimeError::DuplicateId(WidgetId(77))));
    assert_eq!(rt.len(), 1);
    // Um ID restaurado nunca colide com os gerados depois.
    let fresh = rt.create(&test_card::TYPE_ID, None).unwrap();
    assert_ne!(fresh, WidgetId(77));
}

#[test]
fn find_type_resolves_only_registered_names() {
    let rt = runtime();
    assert_eq!(rt.find_type("pod.test-card"), Some(test_card::TYPE_ID));
    assert_eq!(rt.find_type("pod.from-the-future"), None);
}

mod time_subscriptions {
    use super::*;
    use pod_model::time::{weekday_of, Granularity, TimeInfo};
    use pod_model::widget::{Chrome, Descriptor, Effect, Invalidation, Outcome, SizeDip, Widget};
    use pod_model::ids::WidgetTypeId;

    const TYPE: WidgetTypeId = WidgetTypeId("test.time");

    struct Sub {
        g: Granularity,
        seen: u32,
    }
    impl Widget for Sub {
        fn handle(&mut self, e: &Event) -> Outcome {
            match e {
                Event::Started { .. } => Outcome { effects: vec![Effect::TimeTicks(Some(self.g))], ..Default::default() },
                Event::Time(_) => {
                    self.seen += 1;
                    Outcome { invalidate: Invalidation::CONTENT, effects: vec![] }
                }
                _ => Outcome::none(),
            }
        }
        fn view(&self, _: SizeDip) -> pod_model::widget::Element {
            pod_model::widget::Element::Label(self.seen.to_string())
        }
        fn snapshot(&self) -> Snapshot {
            Snapshot { version: 1, data: self.seen.to_string() }
        }
        fn stop(&mut self, _: StopReason) -> Vec<Effect> {
            vec![]
        }
    }

    fn rt() -> Runtime {
        let mut rt = Runtime::new(Box::new(SeqIds::new()));
        rt.register(
            Descriptor {
                type_id: TYPE,
                state_version: 1,
                default_size: SizeDip { w: 88.0, h: 88.0 },
                min_size: SizeDip { w: 88.0, h: 88.0 },
                max_text_bytes: 0,
                chrome: Chrome::Tile,
                shapes: vec![],
                default_shape: None,
            },
            |_| Ok(Box::new(Sub { g: Granularity::Minute, seen: 0 })),
        );
        rt
    }

    fn at(minute: u8, second: u8, millis: u16) -> TimeInfo {
        TimeInfo { year: 2026, month: 10, day: 18, weekday: weekday_of(2026, 10, 18), hour: 9, minute, second, millis }
    }

    fn start(rt: &mut Runtime) -> WidgetId {
        let id = rt.create(&TYPE, None).unwrap();
        rt.mount(id).unwrap();
        rt.start(id, 0).unwrap();
        id
    }

    #[test]
    fn subscribers_get_time_and_hidden_ones_do_not() {
        let mut rt = rt();
        let (a, b) = (start(&mut rt), start(&mut rt));
        rt.set_visible(b, false).unwrap();
        rt.tick_time(at(1, 0, 0));
        let seen = |rt: &Runtime, id| rt.snapshot(id).unwrap().1.data;
        assert_eq!((seen(&rt, a).as_str(), seen(&rt, b).as_str()), ("1", "0"));
        // Ao reaparecer, o shell entrega a hora atual uma vez.
        rt.deliver_time(b, at(1, 0, 0));
        assert_eq!(seen(&rt, b), "1");
    }

    #[test]
    fn wait_follows_the_visible_subscribers_and_removal_clears_it() {
        let mut rt = rt();
        assert_eq!(rt.next_time_wait_ms(&at(0, 30, 250)), None, "sem assinantes não há timer");
        let a = start(&mut rt);
        assert_eq!(rt.next_time_wait_ms(&at(0, 30, 250)), Some(29_750));
        rt.set_visible(a, false).unwrap();
        assert_eq!(rt.next_time_wait_ms(&at(0, 30, 250)), None, "oculto não acorda o processo");
        rt.set_visible(a, true).unwrap();
        rt.remove(a, StopReason::Removed);
        assert_eq!(rt.next_time_wait_ms(&at(0, 30, 250)), None);
    }
}

mod metrics_subscriptions {
    use super::*;
    use pod_model::metrics::{MetricsNeeds, Sample};

    fn sub(rt: &mut Runtime, type_id: &pod_model::ids::WidgetTypeId) -> WidgetId {
        let id = rt.create(type_id, None).unwrap();
        rt.mount(id).unwrap();
        rt.start(id, 0).unwrap();
        id
    }

    #[test]
    fn needs_follow_visible_subscribers_only() {
        let mut rt = runtime();
        assert!(rt.metrics_needs().is_empty(), "sem assinantes, nada a ler");
        let cpu = sub(&mut rt, &widgets::system::CPU);
        let disk = sub(&mut rt, &widgets::system::DISK);
        assert_eq!(rt.metrics_needs(), MetricsNeeds::CPU.union(MetricsNeeds::DISK));
        rt.set_visible(cpu, false).unwrap();
        assert_eq!(rt.metrics_needs(), MetricsNeeds::DISK, "oculto não pede leitura");
        rt.set_visible(disk, false).unwrap();
        assert!(rt.metrics_needs().is_empty());
        rt.set_visible(cpu, true).unwrap();
        rt.remove(cpu, StopReason::Removed);
        assert!(rt.metrics_needs().is_empty(), "remover cancela a assinatura");
    }

    #[test]
    fn readings_reach_visible_subscribers_and_invalidate_only_on_change() {
        let mut rt = runtime();
        let id = sub(&mut rt, &widgets::system::CPU);
        let _ = rt.take_invalidated();
        let s = Sample { cpu: Some(0.5), ..Default::default() };
        rt.tick_metrics(s);
        assert_eq!(rt.take_invalidated().len(), 1);
        rt.tick_metrics(s);
        assert!(rt.take_invalidated().is_empty(), "mesmo valor: sem invalidação");
        rt.set_visible(id, false).unwrap();
        rt.tick_metrics(Sample { cpu: Some(0.9), ..Default::default() });
        assert!(rt.take_invalidated().is_empty(), "oculto não recebe");
    }
}
