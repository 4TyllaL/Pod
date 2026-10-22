//! Runtime: registro de tipos, instâncias, lifecycle, eventos, efeitos e prazos.
//!
//! Não conhece HWND nem recursos gráficos. O shell chama os comandos e consome o que o runtime
//! devolve (`Command`, invalidações). Tudo roda na thread de UI e é síncrono.

mod ids;
mod scheduler;

pub use ids::{IdSource, RandomIds};
use scheduler::Scheduler;

use std::collections::BTreeMap;

use pod_model::ids::{WidgetId, WidgetTypeId};
use pod_model::local::{LocalNeeds, LocalSnapshot};
use pod_model::metrics::{MetricsNeeds, Sample};
use pod_model::shared::SharedData;
use pod_model::time::{Granularity, TimeInfo};
use pod_model::view::View;
use pod_model::widget::{
    Descriptor, Effect, Element, Event, ViewCtx, Factory, Invalidation, Phase, SizeDip, Snapshot, StopReason,
    Widget, WidgetError,
};

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeError {
    UnknownType(String),
    UnknownInstance(WidgetId),
    DuplicateId(WidgetId),
    /// A operação não é permitida na fase atual da instância.
    WrongPhase { id: WidgetId, phase: Phase },
    Widget(WidgetError),
}

impl From<WidgetError> for RuntimeError {
    fn from(e: WidgetError) -> Self {
        RuntimeError::Widget(e)
    }
}

/// Pedidos ao shell resultantes de efeitos de um widget.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// O estado durável de `id` mudou: agendar snapshot.
    StateChanged(WidgetId),
    /// O widget pediu para substituir o texto de um campo.
    SetText { id: WidgetId, field: pod_model::widget::LocalId, text: String },
    /// O widget pediu o aviso sonoro.
    Chime(WidgetId),
    /// Os dados compartilhados mudaram: persistir.
    SharedChanged,
    /// O widget pediu o painel de edição.
    OpenPanel(WidgetId, pod_model::widget::PanelKind),
    OpenUri(u64),
    OpenInput(WidgetId, pod_model::widget::InputSpec),
    InputError(WidgetId, String),
    /// Nova assinatura de hora: entregar a hora atual já, sem esperar a próxima fronteira.
    TimeNow(WidgetId),
    /// O widget pediu um comando de midia (tocar/pausar, proxima, anterior).
    Media(pod_model::media::MediaCommand),
    /// O widget pediu para abrir a manchete `n` do seu feed.
    OpenNews(WidgetId, usize),
    /// O widget GitHub pediu para esquecer o token do cofre.
    ForgetToken,
    PickPath(WidgetId, bool),
    Launch(String, bool),
}

struct Registration {
    descriptor: Descriptor,
    factory: Factory,
}

struct Instance {
    type_id: WidgetTypeId,
    /// Muda a cada instância criada; resultados atrasados com geração antiga são descartados.
    generation: u64,
    phase: Phase,
    /// Preferência de visibilidade: ortogonal à fase.
    visible: bool,
    widget: Box<dyn Widget>,
}

pub struct Runtime {
    registry: BTreeMap<WidgetTypeId, Registration>,
    instances: BTreeMap<WidgetId, Instance>,
    scheduler: Scheduler,
    ids: Box<dyn IdSource>,
    next_generation: u64,
    invalidated: BTreeMap<WidgetId, Invalidation>,
    /// Assinaturas de hora (Effect::TimeTicks) por instância.
    time_subs: BTreeMap<WidgetId, Granularity>,
    /// Assinaturas de métricas do sistema (Effect::Metrics) por instância.
    metrics_subs: BTreeMap<WidgetId, MetricsNeeds>,
    /// Assinaturas de pastas e área de transferência (Effect::Local) por instância.
    local_subs: BTreeMap<WidgetId, LocalNeeds>,
    /// Instancias que assinam a sessao de midia (Effect::Media).
    media_subs: std::collections::BTreeSet<WidgetId>,
    /// Instancias que assinam as notificacoes (Effect::Notifications).
    notif_subs: std::collections::BTreeSet<WidgetId>,
    /// Instancias que assinam o uso do Claude Code (Effect::ClaudeUsage).
    usage_subs: std::collections::BTreeSet<WidgetId>,
    /// Instancias que assinam o clima de uma cidade (Effect::Weather).
    weather_subs: BTreeMap<WidgetId, String>,
    /// Instancias que assinam um feed (Effect::News), pelo id do feed.
    news_subs: BTreeMap<WidgetId, String>,
    /// Instancias que assinam o resumo do GitHub.
    github_subs: std::collections::BTreeSet<WidgetId>,
    /// Fusos pedidos por instância (relógio mundial).
    zone_subs: BTreeMap<WidgetId, Vec<String>>,
    /// Pedido de cotações por instância (widget Bolsa).
    quote_subs: BTreeMap<WidgetId, pod_model::quotes::QuoteRequest>,
    /// Último relógio monotônico informado pelo shell.
    now_ms: u64,
    shared: SharedData,
}

impl Runtime {
    pub fn new(ids: Box<dyn IdSource>) -> Self {
        Self {
            registry: BTreeMap::new(),
            instances: BTreeMap::new(),
            scheduler: Scheduler::default(),
            ids,
            next_generation: 1,
            invalidated: BTreeMap::new(),
            time_subs: BTreeMap::new(),
            metrics_subs: BTreeMap::new(),
            local_subs: BTreeMap::new(),
            media_subs: std::collections::BTreeSet::new(),
            notif_subs: std::collections::BTreeSet::new(),
            usage_subs: std::collections::BTreeSet::new(),
            weather_subs: BTreeMap::new(),
            news_subs: BTreeMap::new(),
            github_subs: Default::default(),
            zone_subs: BTreeMap::new(),
            quote_subs: BTreeMap::new(),
            now_ms: 0,
            shared: SharedData::default(),
        }
    }

    pub fn register(&mut self, descriptor: Descriptor, factory: Factory) {
        self.registry.insert(descriptor.type_id.clone(), Registration { descriptor, factory });
    }

    pub fn descriptor_of(&self, id: WidgetId) -> Option<Descriptor> {
        let inst = self.instances.get(&id)?;
        self.registry.get(&inst.type_id).map(|r| r.descriptor.clone())
    }

    pub fn descriptor(&self, type_id: &WidgetTypeId) -> Option<&Descriptor> {
        self.registry.get(type_id).map(|r| &r.descriptor)
    }

    pub fn len(&self) -> usize {
        self.instances.len()
    }

    pub fn ids(&self) -> impl Iterator<Item = WidgetId> + '_ {
        self.instances.keys().copied()
    }

    pub fn phase(&self, id: WidgetId) -> Option<Phase> {
        self.instances.get(&id).map(|i| i.phase)
    }

    pub fn is_visible(&self, id: WidgetId) -> Option<bool> {
        self.instances.get(&id).map(|i| i.visible)
    }

    /// Cria uma instância nova (ID novo) a partir de defaults, ou restaura de um snapshot.
    /// A instância nasce `Loaded`. Se a factory falhar, nada é registrado.
    pub fn create(
        &mut self,
        type_id: &WidgetTypeId,
        restore: Option<&Snapshot>,
    ) -> Result<WidgetId, RuntimeError> {
        let widget = self.build(type_id, restore)?;
        let id = loop {
            let candidate = self.ids.next_id();
            if !self.instances.contains_key(&candidate) {
                break candidate;
            }
        };
        self.insert(id, type_id, widget);
        Ok(id)
    }

    /// Restaura uma instância persistida mantendo o ID gravado. Recusa ID já em uso.
    pub fn restore(
        &mut self,
        id: WidgetId,
        type_id: &WidgetTypeId,
        snapshot: &Snapshot,
    ) -> Result<(), RuntimeError> {
        if self.instances.contains_key(&id) {
            return Err(RuntimeError::DuplicateId(id));
        }
        let widget = self.build(type_id, Some(snapshot))?;
        self.insert(id, type_id, widget);
        Ok(())
    }

    /// Tipo registrado com este nome, se existir (para resolver o texto de um arquivo).
    pub fn find_type(&self, name: &str) -> Option<WidgetTypeId> {
        self.registry.keys().find(|k| k.0 == name).cloned()
    }

    fn build(
        &self,
        type_id: &WidgetTypeId,
        restore: Option<&Snapshot>,
    ) -> Result<Box<dyn Widget>, RuntimeError> {
        let reg = self
            .registry
            .get(type_id)
            .ok_or_else(|| RuntimeError::UnknownType(type_id.0.to_string()))?;
        Ok((reg.factory)(restore)?)
    }

    fn insert(&mut self, id: WidgetId, type_id: &WidgetTypeId, widget: Box<dyn Widget>) {
        let mut widget = widget;
        widget.set_shared(&self.shared);
        let generation = self.next_generation;
        self.next_generation += 1;
        self.instances.insert(
            id,
            Instance { type_id: type_id.clone(), generation, phase: Phase::Loaded, visible: true, widget },
        );
    }

    /// O host de janela e o editor foram conectados.
    pub fn mount(&mut self, id: WidgetId) -> Result<(), RuntimeError> {
        self.transition(id, Phase::Mounted)
    }

    /// Apta a receber eventos. Entrega `Event::Started` já na fase `Running`.
    pub fn start(&mut self, id: WidgetId, now_ms: u64) -> Result<Vec<Command>, RuntimeError> {
        self.now_ms = now_ms;
        self.transition(id, Phase::Running)?;
        self.dispatch(id, &Event::Started { now_ms })
    }

    fn transition(&mut self, id: WidgetId, next: Phase) -> Result<(), RuntimeError> {
        let inst = self.instances.get_mut(&id).ok_or(RuntimeError::UnknownInstance(id))?;
        if !inst.phase.can_become(next) {
            return Err(RuntimeError::WrongPhase { id, phase: inst.phase });
        }
        inst.phase = next;
        Ok(())
    }

    pub fn set_visible(&mut self, id: WidgetId, visible: bool) -> Result<(), RuntimeError> {
        let inst = self.instances.get_mut(&id).ok_or(RuntimeError::UnknownInstance(id))?;
        inst.visible = visible;
        Ok(())
    }

    /// Entrega um evento. Só instâncias `Running` recebem; ocultas continuam recebendo.
    pub fn dispatch(&mut self, id: WidgetId, event: &Event) -> Result<Vec<Command>, RuntimeError> {
        let inst = self.instances.get_mut(&id).ok_or(RuntimeError::UnknownInstance(id))?;
        if inst.phase != Phase::Running {
            return Err(RuntimeError::WrongPhase { id, phase: inst.phase });
        }
        let generation = inst.generation;
        inst.widget.set_now(self.now_ms);
        let outcome = inst.widget.handle(event);
        Ok(self.apply(id, generation, outcome.invalidate, outcome.effects))
    }

    fn apply(
        &mut self,
        id: WidgetId,
        generation: u64,
        invalidate: Invalidation,
        effects: Vec<Effect>,
    ) -> Vec<Command> {
        if !invalidate.is_none() {
            let entry = self.invalidated.entry(id).or_default();
            *entry = entry.merge(invalidate);
        }
        let mut commands = Vec::new();
        for effect in effects {
            match effect {
                Effect::ScheduleAt { deadline_ms, token } => {
                    self.scheduler.schedule(id, generation, token, deadline_ms)
                }
                Effect::CancelTimer { token } => self.scheduler.cancel(id, generation, token),
                Effect::StateChanged => commands.push(Command::StateChanged(id)),
                Effect::SetText { field, text } => commands.push(Command::SetText { id, field, text }),
                Effect::TimeTicks(Some(g)) => {
                    self.time_subs.insert(id, g);
                    commands.push(Command::TimeNow(id));
                }
                Effect::TimeTicks(None) => {
                    self.time_subs.remove(&id);
                }
                Effect::Chime => commands.push(Command::Chime(id)),
                Effect::SharedOp(op) => {
                    if self.shared.apply(op) {
                        self.notify_shared();
                        commands.push(Command::SharedChanged);
                    }
                }
                Effect::OpenPanel(kind) => commands.push(Command::OpenPanel(id, kind)),
                Effect::OpenUri(link) => commands.push(Command::OpenUri(link)),
                Effect::OpenInput(spec) => commands.push(Command::OpenInput(id, spec)),
                Effect::InputError(msg) => commands.push(Command::InputError(id, msg)),
                Effect::Metrics(Some(n)) => {
                    self.metrics_subs.insert(id, n);
                }
                Effect::Metrics(None) => {
                    self.metrics_subs.remove(&id);
                }
                Effect::Local(Some(n)) => {
                    self.local_subs.insert(id, n);
                }
                Effect::Local(None) => {
                    self.local_subs.remove(&id);
                }
                Effect::Media(true) => {
                    self.media_subs.insert(id);
                }
                Effect::Media(false) => {
                    self.media_subs.remove(&id);
                }
                Effect::MediaCommand(cmd) => commands.push(Command::Media(cmd)),
                Effect::Notifications(true) => {
                    self.notif_subs.insert(id);
                }
                Effect::Notifications(false) => {
                    self.notif_subs.remove(&id);
                }
                Effect::ClaudeUsage(true) => {
                    self.usage_subs.insert(id);
                }
                Effect::ClaudeUsage(false) => {
                    self.usage_subs.remove(&id);
                }
                Effect::Weather(Some(city)) => {
                    self.weather_subs.insert(id, city);
                }
                Effect::Weather(None) => {
                    self.weather_subs.remove(&id);
                }
                Effect::News(Some(feed)) => {
                    self.news_subs.insert(id, feed);
                }
                Effect::News(None) => {
                    self.news_subs.remove(&id);
                }
                Effect::OpenNews(n) => commands.push(Command::OpenNews(id, n)),
                Effect::Github(true) => {
                    self.github_subs.insert(id);
                }
                Effect::Github(false) => {
                    self.github_subs.remove(&id);
                }
                Effect::ForgetToken => commands.push(Command::ForgetToken),
                Effect::Quotes(Some(req)) => {
                    self.quote_subs.insert(id, req);
                }
                Effect::Quotes(None) => {
                    self.quote_subs.remove(&id);
                }
                Effect::Zones(keys) if keys.is_empty() => {
                    self.zone_subs.remove(&id);
                }
                Effect::Zones(keys) => {
                    self.zone_subs.insert(id, keys);
                }
                Effect::PickPath { folder } => commands.push(Command::PickPath(id, folder)),
                Effect::Launch { path, folder } => commands.push(Command::Launch(path, folder)),
            }
        }
        commands
    }

    /// Entrega os prazos vencidos. Cada prazo dispara uma vez: ao voltar de suspensão não há
    /// repetição de ticks perdidos; o widget reagenda a partir de `now_ms`.
    pub fn tick(&mut self, now_ms: u64) -> Vec<Command> {
        self.now_ms = now_ms;
        let mut commands = Vec::new();
        for due in self.scheduler.take_due(now_ms) {
            let live = self.instances.get(&due.id).is_some_and(|i| {
                i.generation == due.generation && i.phase == Phase::Running
            });
            if !live {
                continue; // instância removida ou outra geração: resultado antigo, descartado
            }
            let event = Event::Timer { token: due.token, now_ms };
            if let Ok(mut c) = self.dispatch(due.id, &event) {
                commands.append(&mut c);
            }
        }
        commands
    }

    /// Informa o relógio monotônico atual (o shell chama antes de despachar eventos sem tempo).
    pub fn set_now(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
    }

    pub fn next_deadline(&self) -> Option<u64> {
        self.scheduler.next_deadline()
    }

    /// Invalidações acumuladas desde a última chamada.
    pub fn take_invalidated(&mut self) -> Vec<(WidgetId, Invalidation)> {
        std::mem::take(&mut self.invalidated).into_iter().collect()
    }

    pub fn view(&self, id: WidgetId, available: SizeDip) -> Result<Element, RuntimeError> {
        let inst = self.instances.get(&id).ok_or(RuntimeError::UnknownInstance(id))?;
        Ok(inst.widget.view(available))
    }

    pub fn tile_view(&self, id: WidgetId, ctx: &ViewCtx) -> Result<Option<View>, RuntimeError> {
        let inst = self.instances.get(&id).ok_or(RuntimeError::UnknownInstance(id))?;
        // Formatos largos: só quem tem layout próprio os recebe; os demais desenham o de 2 colunas
        // com a mesma altura, esticado.
        let mut c = *ctx;
        if !self.descriptor_of(id).is_some_and(|d| crate::widgets::wide_aware(&d.type_id)) {
            c.shape = c.shape.design();
        }
        Ok(inst.widget.tile_view(&c))
    }

    /// Entrega a hora às instâncias assinantes, em execução e visíveis. Janelas ocultas não
    /// gastam nada; `deliver_time` as atualiza ao reaparecerem.
    pub fn tick_time(&mut self, time: TimeInfo) -> Vec<Command> {
        let ids: Vec<WidgetId> = self
            .time_subs
            .keys()
            .copied()
            .filter(|id| self.instances.get(id).is_some_and(|i| i.visible && i.phase == Phase::Running))
            .collect();
        let mut commands = Vec::new();
        for id in ids {
            if let Ok(mut c) = self.dispatch(id, &Event::Time(time)) {
                commands.append(&mut c);
            }
        }
        commands
    }

    fn live_quote_subs(&self) -> impl Iterator<Item = (&WidgetId, &pod_model::quotes::QuoteRequest)> {
        self.quote_subs
            .iter()
            .filter(|(id, _)| self.instances.get(id).is_some_and(|i| i.visible && i.phase == Phase::Running))
    }

    /// Pedidos distintos dos widgets Bolsa visíveis.
    pub fn quote_requests(&self) -> Vec<pod_model::quotes::QuoteRequest> {
        let mut v: Vec<pod_model::quotes::QuoteRequest> = Vec::new();
        for (_, r) in self.live_quote_subs() {
            if !v.contains(r) {
                v.push(r.clone());
            }
        }
        v
    }

    /// Entrega a cada widget Bolsa visível o estado do seu pedido.
    pub fn tick_quotes(&mut self, state_of: &dyn Fn(&pod_model::quotes::QuoteRequest) -> pod_model::quotes::QuoteState) -> Vec<Command> {
        let subs: Vec<(WidgetId, pod_model::quotes::QuoteRequest)> = self.live_quote_subs().map(|(i, r)| (*i, r.clone())).collect();
        let mut commands = Vec::new();
        for (id, req) in subs {
            if let Ok(mut c) = self.dispatch(id, &Event::Quotes(state_of(&req))) {
                commands.append(&mut c);
            }
        }
        commands
    }

    /// Entrega a cada relógio mundial visível a hora de cada fuso que ele pediu.
    pub fn tick_zones(&mut self, resolve: &dyn Fn(&str) -> Option<pod_model::worldclock::ZoneTime>) -> Vec<Command> {
        let subs: Vec<(WidgetId, Vec<String>)> = self
            .zone_subs
            .iter()
            .filter(|(id, _)| self.instances.get(id).is_some_and(|i| i.visible && i.phase == Phase::Running))
            .map(|(id, k)| (*id, k.clone()))
            .collect();
        let mut commands = Vec::new();
        for (id, keys) in subs {
            let times: Vec<_> = keys.iter().map(|k| resolve(k)).collect();
            if let Ok(mut c) = self.dispatch(id, &Event::Zones(times)) {
                commands.append(&mut c);
            }
        }
        commands
    }

    fn live_metrics_subs(&self) -> impl Iterator<Item = (&WidgetId, &MetricsNeeds)> {
        self.metrics_subs
            .iter()
            .filter(|(id, _)| self.instances.get(id).is_some_and(|i| i.visible && i.phase == Phase::Running))
    }

    /// Soma do que as instâncias visíveis em execução pedem; vazio = o provedor não deve ler nada.
    pub fn metrics_needs(&self) -> MetricsNeeds {
        self.live_metrics_subs().fold(MetricsNeeds::default(), |a, (_, n)| a.union(*n))
    }

    /// Entrega uma leitura às instâncias assinantes visíveis.
    pub fn tick_metrics(&mut self, sample: Sample) -> Vec<Command> {
        let ids: Vec<WidgetId> = self.live_metrics_subs().map(|(id, _)| *id).collect();
        let mut commands = Vec::new();
        for id in ids {
            if let Ok(mut c) = self.dispatch(id, &Event::Metrics(sample)) {
                commands.append(&mut c);
            }
        }
        commands
    }

    fn live_local_subs(&self) -> impl Iterator<Item = (&WidgetId, &LocalNeeds)> {
        self.local_subs
            .iter()
            .filter(|(id, _)| self.instances.get(id).is_some_and(|i| i.visible && i.phase == Phase::Running))
    }

    /// Soma do que as instâncias visíveis em execução pedem; vazio = o provedor não deve ler nada.
    pub fn local_needs(&self) -> LocalNeeds {
        self.live_local_subs().fold(LocalNeeds::default(), |a, (_, n)| a.union(*n))
    }

    /// Entrega uma leitura de pastas/área de transferência às instâncias assinantes visíveis.
    pub fn tick_local(&mut self, snap: LocalSnapshot) -> Vec<Command> {
        let ids: Vec<WidgetId> = self.live_local_subs().map(|(id, _)| *id).collect();
        let mut commands = Vec::new();
        for id in ids {
            if let Ok(mut c) = self.dispatch(id, &Event::Local(snap.clone())) {
                commands.append(&mut c);
            }
        }
        commands
    }

    fn live_media_subs(&self) -> impl Iterator<Item = &WidgetId> {
        self.media_subs
            .iter()
            .filter(|id| self.instances.get(id).is_some_and(|i| i.visible && i.phase == Phase::Running))
    }

    fn live_notif_subs(&self) -> impl Iterator<Item = &WidgetId> {
        self.notif_subs
            .iter()
            .filter(|id| self.instances.get(id).is_some_and(|i| i.visible && i.phase == Phase::Running))
    }

    fn live_usage_subs(&self) -> impl Iterator<Item = &WidgetId> {
        self.usage_subs
            .iter()
            .filter(|id| self.instances.get(id).is_some_and(|i| i.visible && i.phase == Phase::Running))
    }

    fn live_weather_subs(&self) -> impl Iterator<Item = (&WidgetId, &String)> {
        self.weather_subs
            .iter()
            .filter(|(id, _)| self.instances.get(id).is_some_and(|i| i.visible && i.phase == Phase::Running))
    }

    /// Cidades distintas pedidas pelos widgets Weather visiveis (ordenadas).
    pub fn weather_cities(&self) -> Vec<String> {
        let mut v: Vec<String> = self.live_weather_subs().map(|(_, c)| c.clone()).collect();
        v.sort();
        v.dedup();
        v
    }

    /// Entrega a cada assinante visivel o estado da sua cidade (`state_of`); o widget ignora o que nao mudou.
    pub fn tick_weather(&mut self, state_of: &dyn Fn(&str) -> pod_model::net::WeatherState) -> Vec<Command> {
        let subs: Vec<(WidgetId, String)> = self.live_weather_subs().map(|(i, c)| (*i, c.clone())).collect();
        let mut commands = Vec::new();
        for (id, city) in subs {
            if let Ok(mut c) = self.dispatch(id, &Event::Weather(state_of(&city))) {
                commands.append(&mut c);
            }
        }
        commands
    }

    fn live_news_subs(&self) -> impl Iterator<Item = (&WidgetId, &String)> {
        self.news_subs
            .iter()
            .filter(|(id, _)| self.instances.get(id).is_some_and(|i| i.visible && i.phase == Phase::Running))
    }

    /// Algum widget GitHub visivel em execucao quer o resumo?
    pub fn wants_github(&self) -> bool {
        self.github_subs.iter().any(|id| self.instances.get(id).is_some_and(|i| i.visible && i.phase == Phase::Running))
    }

    /// Entrega o estado do GitHub a cada assinante visivel.
    pub fn tick_github(&mut self, state: &pod_model::github::GithubState) -> Vec<Command> {
        let subs: Vec<WidgetId> = self
            .github_subs
            .iter()
            .filter(|id| self.instances.get(id).is_some_and(|i| i.visible && i.phase == Phase::Running))
            .copied()
            .collect();
        let mut commands = Vec::new();
        for id in subs {
            if let Ok(mut c) = self.dispatch(id, &Event::Github(state.clone())) {
                commands.append(&mut c);
            }
        }
        commands
    }

    /// Feeds distintos pedidos pelos widgets News visiveis (ordenados).
    pub fn news_feeds(&self) -> Vec<String> {
        let mut v: Vec<String> = self.live_news_subs().map(|(_, f)| f.clone()).collect();
        v.sort();
        v.dedup();
        v
    }

    /// Feed assinado por uma instancia (para o host achar o endereco de uma manchete).
    pub fn news_feed_of(&self, id: WidgetId) -> Option<String> {
        self.news_subs.get(&id).cloned()
    }

    /// Entrega a cada assinante visivel o estado do seu feed.
    pub fn tick_news(&mut self, state_of: &dyn Fn(&str) -> pod_model::news::NewsState) -> Vec<Command> {
        let subs: Vec<(WidgetId, String)> = self.live_news_subs().map(|(i, f)| (*i, f.clone())).collect();
        let mut commands = Vec::new();
        for (id, feed) in subs {
            if let Ok(mut c) = self.dispatch(id, &Event::News(state_of(&feed))) {
                commands.append(&mut c);
            }
        }
        commands
    }

    /// Algum widget visivel em execucao quer o uso do Claude Code?
    pub fn usage_wanted(&self) -> bool {
        self.live_usage_subs().next().is_some()
    }

    /// Entrega o uso do Claude Code aos assinantes visiveis.
    pub fn tick_usage(&mut self, snap: pod_model::usage::UsageSnapshot) -> Vec<Command> {
        let ids: Vec<WidgetId> = self.live_usage_subs().copied().collect();
        let mut commands = Vec::new();
        for id in ids {
            if let Ok(mut c) = self.dispatch(id, &Event::ClaudeUsage(snap.clone())) {
                commands.append(&mut c);
            }
        }
        commands
    }

    /// Algum widget visivel em execucao quer as notificacoes?
    pub fn notifications_wanted(&self) -> bool {
        self.live_notif_subs().next().is_some()
    }

    /// Entrega as notificacoes aos assinantes visiveis.
    pub fn tick_notifications(&mut self, snap: pod_model::notifications::NotifSnapshot) -> Vec<Command> {
        let ids: Vec<WidgetId> = self.live_notif_subs().copied().collect();
        let mut commands = Vec::new();
        for id in ids {
            if let Ok(mut c) = self.dispatch(id, &Event::Notifications(snap.clone())) {
                commands.append(&mut c);
            }
        }
        commands
    }

    /// Algum widget visivel em execucao quer a sessao de midia?
    pub fn media_wanted(&self) -> bool {
        self.live_media_subs().next().is_some()
    }

    /// Entrega a sessao de midia atual aos assinantes visiveis.
    pub fn tick_media(&mut self, snap: Option<pod_model::media::MediaSnapshot>) -> Vec<Command> {
        let ids: Vec<WidgetId> = self.live_media_subs().copied().collect();
        let mut commands = Vec::new();
        for id in ids {
            if let Ok(mut c) = self.dispatch(id, &Event::Media(snap.clone())) {
                commands.append(&mut c);
            }
        }
        commands
    }

    /// Entrega a hora a uma instância assinante (por exemplo, ao ficar visível).
    pub fn deliver_time(&mut self, id: WidgetId, time: TimeInfo) -> Vec<Command> {
        if !self.time_subs.contains_key(&id) {
            return Vec::new();
        }
        self.dispatch(id, &Event::Time(time)).unwrap_or_default()
    }

    /// Milissegundos até a próxima fronteira que importa a alguma instância visível assinante.
    pub fn next_time_wait_ms(&self, now: &TimeInfo) -> Option<u32> {
        self.time_subs
            .iter()
            .filter(|(id, _)| self.instances.get(id).is_some_and(|i| i.visible && i.phase == Phase::Running))
            .map(|(_, g)| now.ms_until_next(*g))
            .min()
    }

    pub fn menu_items(&self, id: WidgetId) -> Vec<pod_model::widget::MenuItem> {
        self.instances.get(&id).map(|i| i.widget.menu_items()).unwrap_or_default()
    }

    pub fn shared(&self) -> &SharedData {
        &self.shared
    }

    /// Troca os dados compartilhados (carga inicial ou edição do painel) e avisa as instâncias.
    pub fn set_shared(&mut self, data: SharedData) {
        self.shared = data;
        self.notify_shared();
    }

    /// Edita os dados compartilhados e avisa as instâncias.
    pub fn mutate_shared<R>(&mut self, f: impl FnOnce(&mut SharedData) -> R) -> R {
        let r = f(&mut self.shared);
        self.notify_shared();
        r
    }

    fn notify_shared(&mut self) {
        let ids: Vec<WidgetId> = self.instances.keys().copied().collect();
        for id in ids {
            if let Some(inst) = self.instances.get_mut(&id) {
                inst.widget.set_shared(&self.shared);
                let entry = self.invalidated.entry(id).or_default();
                *entry = entry.merge(Invalidation::CONTENT);
            }
        }
    }

    pub fn has_content(&self, id: WidgetId) -> bool {
        self.instances.get(&id).is_some_and(|i| i.widget.has_content())
    }

    pub fn snapshot(&self, id: WidgetId) -> Result<(WidgetTypeId, Snapshot), RuntimeError> {
        let inst = self.instances.get(&id).ok_or(RuntimeError::UnknownInstance(id))?;
        Ok((inst.type_id.clone(), inst.widget.snapshot()))
    }

    /// Para o widget, cancela seus prazos e libera a instância. Idempotente: remover um ID
    /// já removido não é erro. O shell só deve chamar depois do ack de gravação (M3).
    pub fn remove(&mut self, id: WidgetId, reason: StopReason) -> Vec<Command> {
        let Some(mut inst) = self.instances.remove(&id) else {
            return Vec::new();
        };
        inst.phase = Phase::Stopping;
        let effects = inst.widget.stop(reason);
        let commands = self.apply(id, inst.generation, Invalidation::NONE, effects);
        self.scheduler.cancel_all(id);
        self.invalidated.remove(&id);
        self.time_subs.remove(&id);
        self.metrics_subs.remove(&id);
        self.local_subs.remove(&id);
        self.media_subs.remove(&id);
        self.notif_subs.remove(&id);
        self.usage_subs.remove(&id);
        self.weather_subs.remove(&id);
        self.news_subs.remove(&id);
        self.github_subs.remove(&id);
        self.zone_subs.remove(&id);
        self.quote_subs.remove(&id);
        inst.phase = Phase::Disposed;
        commands
    }

    /// Encerramento: para todas as instâncias.
    pub fn shutdown(&mut self) {
        let ids: Vec<_> = self.instances.keys().copied().collect();
        for id in ids {
            self.remove(id, StopReason::Shutdown);
        }
    }

    pub fn pending_timers(&self) -> usize {
        self.scheduler.len()
    }
}

#[cfg(test)]
mod tests;
