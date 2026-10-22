//! Contrato de widget: estado, eventos tipados, view limitada e efeitos.
//!
//! Os nomes especificam responsabilidades (ARCHITECTURE §7), não uma ABI. Um widget nunca
//! recebe HWND, renderer, caminhos ou executor: só `Event` entra e só `Outcome` sai.

use crate::ids::WidgetTypeId;
use crate::local::{LocalNeeds, LocalSnapshot};
use crate::media::{MediaCommand, MediaSnapshot};
use crate::net::WeatherState;
use crate::news::NewsState;
use crate::notifications::NotifSnapshot;
use crate::usage::UsageSnapshot;
use crate::metrics::{MetricsNeeds, Sample};
use crate::shape::Shape;
use crate::shared::{SharedData, SharedOp};
use crate::time::{Granularity, TimeInfo};
use crate::view::View;

/// Tamanho em DIPs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizeDip {
    pub w: f32,
    pub h: f32,
}

/// Moldura da janela: nota (tamanho livre, legenda de arrastar) ou cartão de grade (sem legenda).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chrome {
    Note,
    Tile,
}

/// Metadados estáticos de um tipo de widget (operação *Describe*).
#[derive(Debug, Clone, PartialEq)]
pub struct Descriptor {
    pub type_id: WidgetTypeId,
    /// Versão do formato de estado durável que este tipo grava.
    pub state_version: u32,
    pub default_size: SizeDip,
    pub min_size: SizeDip,
    pub max_text_bytes: usize,
    pub chrome: Chrome,
    /// Formatos de grade aceitos (vazio em `Chrome::Note`).
    pub shapes: Vec<Shape>,
    pub default_shape: Option<Shape>,
}

/// Painel de edição que um widget pode pedir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    Events,
    Tasks,
    Links,
    Parcels,
}

/// Pedido de uma janelinha de entrada de texto (um campo, OK/Cancelar).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputSpec {
    pub title: String,
    pub label: String,
    pub initial: String,
    /// Segredo (token): campo mascarado, o texto vai direto ao cofre do Windows e **nunca** chega ao widget.
    pub secret: bool,
}

/// Item de menu contribuído por um widget (menu de contexto do cartão).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    pub id: u32,
    pub label: String,
    pub checked: bool,
    /// Itens com o mesmo grupo viram um submenu com esse nome (ajustes agrupados).
    pub group: Option<&'static str>,
}

/// Contexto entregue a `Widget::tile_view`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewCtx {
    pub shape: Shape,
    pub time: TimeInfo,
}

/// Identificador local e estável de um campo/botão dentro da view de um widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalId(pub u32);

/// Eventos entregues ao widget. Tempo é monotônico, em milissegundos.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A instância passou a `Running`. Permite agendar o primeiro prazo.
    Started { now_ms: u64 },
    /// Texto confirmado de um campo (nunca composição IME em andamento).
    TextChanged { field: LocalId, text: String },
    Clicked { button: LocalId },
    /// Prazo agendado pelo próprio widget venceu.
    Timer { token: u64, now_ms: u64 },
    Resized(SizeDip),
    /// Nova leitura de hora (só para quem pediu `Effect::TimeTicks`).
    Time(TimeInfo),
    ShapeChanged(Shape),
    /// O usuário escolheu um item de `Widget::menu_items`.
    MenuCommand(u32),
    /// Nova leitura de métricas (só para quem pediu `Effect::Metrics`).
    Metrics(Sample),
    /// Nova leitura de pastas/área de transferência (só para quem pediu `Effect::Local`).
    Local(LocalSnapshot),
    /// Sessao de midia atual (`None` = nada tocando). So para quem pediu `Effect::Media`.
    Media(Option<MediaSnapshot>),
    /// Notificacoes do Windows (so para quem pediu `Effect::Notifications`).
    Notifications(NotifSnapshot),
    /// Uso do Claude Code lido dos transcritos locais (so para quem pediu `Effect::ClaudeUsage`).
    ClaudeUsage(UsageSnapshot),
    /// Clima da cidade que o widget assinou (so para quem pediu `Effect::Weather`).
    Weather(WeatherState),
    /// Manchetes do feed que o widget assinou (so para quem pediu `Effect::News`).
    News(NewsState),
    /// Resumo do GitHub (so para quem pediu `Effect::Github`). Nunca traz o token.
    Github(crate::github::GithubState),
    /// O usuário escolheu um caminho no seletor pedido por `Effect::PickPath`.
    PathPicked { path: String, folder: bool },
    /// Hora em cada fuso pedido por `Effect::Zones`, na mesma ordem (`None` = fuso desconhecido).
    Zones(Vec<Option<crate::worldclock::ZoneTime>>),
    /// Cotações do pedido feito com `Effect::Quotes`.
    Quotes(crate::quotes::QuoteState),
}

/// O que precisa ser redesenhado depois de tratar um evento.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Invalidation {
    pub content: bool,
    pub layout: bool,
}

impl Invalidation {
    pub const NONE: Self = Self { content: false, layout: false };
    pub const CONTENT: Self = Self { content: true, layout: false };
    pub const LAYOUT: Self = Self { content: true, layout: true };

    pub fn merge(self, other: Self) -> Self {
        Self { content: self.content || other.content, layout: self.layout || other.layout }
    }
    pub fn is_none(self) -> bool {
        !self.content && !self.layout
    }
}

/// Pedidos que o widget faz ao runtime. O runtime controla a execução.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Entregar `Event::Timer { token }` a partir de `deadline_ms` (relógio monotônico).
    ScheduleAt { deadline_ms: u64, token: u64 },
    CancelTimer { token: u64 },
    /// O estado durável mudou: o shell deve agendar um snapshot.
    StateChanged,
    /// Substituir o texto de um campo (por exemplo, ao recusar um valor acima do limite).
    SetText { field: LocalId, text: String },
    /// Assinar (ou cancelar, com `None`) a hora na granularidade pedida. O serviço de tempo do
    /// shell entrega `Event::Time` só nas fronteiras e pula janelas invisíveis.
    TimeTicks(Option<Granularity>),
    /// Assinar (ou cancelar, com `None`) as métricas do sistema pedidas. O provedor só lê o que
    /// algum assinante visível pediu e para quando não há nenhum.
    Metrics(Option<MetricsNeeds>),
    /// Assinar (ou cancelar, com `None`) pastas conhecidas e área de transferência. Mesmas regras
    /// das métricas: só lê o que um assinante visível pediu.
    Local(Option<LocalNeeds>),
    /// Assinar (`true`) ou cancelar (`false`) a sessao de midia do Windows.
    Media(bool),
    /// Pede ao host que execute um comando na sessao de midia atual.
    MediaCommand(MediaCommand),
    /// Assinar (`true`) ou cancelar (`false`) as notificacoes do Windows.
    Notifications(bool),
    /// Assinar (`true`) ou cancelar (`false`) o uso do Claude Code (leitura local de ~/.claude/projects).
    ClaudeUsage(bool),
    /// Assinar o clima de uma cidade (`Some`) ou cancelar (`None`). Exige "Widgets de internet" ligado: sem
    /// o opt-in o host so entrega `WeatherState::Off` e nada sai do computador.
    Weather(Option<String>),
    /// Assinar um feed do catalogo por id (`Some`) ou cancelar (`None`). Exige "Widgets de internet".
    News(Option<String>),
    /// Pede ao host que abra a materia da manchete `n` do feed assinado no navegador. O host guarda os
    /// enderecos e os revalida; o widget nunca fornece URL.
    OpenNews(usize),
    /// Assinar (`true`) ou cancelar o resumo do GitHub. Exige "Widgets de internet" e um token no cofre.
    Github(bool),
    /// Apagar o token do GitHub do cofre do Windows.
    ForgetToken,
    /// Pedir ao host o seletor de arquivo (programa) ou de pasta; a resposta chega em `Event::PathPicked`.
    PickPath { folder: bool },
    /// Assinar a hora em fusos do Windows (chaves como "Tokyo Standard Time"); vazio cancela.
    Zones(Vec<String>),
    /// Assinar cotações (`Some`) ou cancelar (`None`). Exige "Widgets de internet".
    Quotes(Option<crate::quotes::QuoteRequest>),
    /// Abrir um atalho do lançador. O host valida o caminho de novo (absoluto, `.exe`/`.lnk` ou pasta).
    Launch { path: String, folder: bool },
    /// Toca o aviso sonoro do sistema (fim de timer ou de fase).
    Chime,
    /// Altera os dados compartilhados (sem abrir o painel).
    SharedOp(SharedOp),
    /// Abre o painel de edição dos dados compartilhados.
    OpenPanel(PanelKind),
    /// Abre o link salvo (por id) no navegador padrão. O host busca o endereço nos dados
    /// compartilhados e o valida de novo: o widget nunca fornece o endereço.
    OpenUri(u64),
    /// Pede uma entrada de texto ao usuário; a resposta chega como `Event::TextChanged`
    /// (campo 0) e o widget pode recusar com `InputError`.
    OpenInput(InputSpec),
    /// Recusa a entrada recebida: a janelinha continua aberta e mostra a mensagem.
    InputError(String),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outcome {
    pub invalidate: Invalidation,
    pub effects: Vec<Effect>,
}

impl Outcome {
    pub fn none() -> Self {
        Self::default()
    }
}

/// View mínima: superfície, rótulo, botão e campo multilinha. Sem CSS nem DOM.
#[derive(Debug, Clone, PartialEq)]
pub enum Element {
    Surface(Vec<Element>),
    Label(String),
    Button { id: LocalId, label: String },
    TextField { id: LocalId, text: String },
}

/// Estado serializável com versão. O formato do `data` pertence ao tipo do widget.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub version: u32,
    pub data: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Removed,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WidgetError {
    /// Estado de versão mais nova que a suportada: não migrar nem sobrescrever.
    UnsupportedVersion { found: u32, supported: u32 },
    /// Estado inválido; isolado sem derrubar o workspace.
    InvalidState(String),
}

/// Comportamento de uma instância. Sem recursos Windows.
pub trait Widget {
    fn handle(&mut self, event: &Event) -> Outcome;
    /// Relógio monotônico (ms) no momento do próximo evento. O runtime chama antes de `handle`,
    /// para que eventos sem tempo próprio (cliques, menu) possam datar a ação.
    fn set_now(&mut self, _now_ms: u64) {}
    /// Dados compartilhados atuais (eventos e tarefas). O runtime chama ao criar a instância e a
    /// cada mudança; quem não os usa ignora.
    fn set_shared(&mut self, _shared: &SharedData) {}
    fn view(&self, available: SizeDip) -> Element;
    /// View de cartão de grade (ADR-0001). `None` para widgets de nota, que usam `view`.
    fn tile_view(&self, _ctx: &ViewCtx) -> Option<View> {
        None
    }
    /// Itens de menu específicos do widget (ajustes por instância).
    fn menu_items(&self) -> Vec<MenuItem> {
        Vec::new()
    }
    /// Há conteúdo do usuário que seria perdido ao excluir? Exige confirmação local se sim.
    fn has_content(&self) -> bool {
        false
    }
    fn snapshot(&self) -> Snapshot;
    /// Idempotente. Devolve efeitos de cancelamento, se houver.
    fn stop(&mut self, reason: StopReason) -> Vec<Effect>;
}

/// Cria uma instância a partir de estado validado (ou de defaults quando `None`).
/// A migração de versões antigas acontece aqui, em cópia, devolvendo erro recuperável.
pub type Factory = fn(Option<&Snapshot>) -> Result<Box<dyn Widget>, WidgetError>;

/// Fase do ciclo de vida. Visibilidade e suspensão são flags ortogonais, não fases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    Loaded,
    Mounted,
    Running,
    Stopping,
    Disposed,
}

impl Phase {
    /// Transições permitidas: avança uma fase por vez; `Stopping` é alcançável de qualquer
    /// fase viva (remoção durante carga ou montagem).
    pub fn can_become(self, next: Phase) -> bool {
        use Phase::*;
        matches!(
            (self, next),
            (Loaded, Mounted)
                | (Mounted, Running)
                | (Loaded | Mounted | Running, Stopping)
                | (Stopping, Disposed)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_only_moves_forward() {
        use Phase::*;
        assert!(Loaded.can_become(Mounted));
        assert!(Running.can_become(Stopping));
        assert!(!Running.can_become(Loaded));
        assert!(!Disposed.can_become(Running));
        assert!(!Loaded.can_become(Running), "montar é obrigatório antes de rodar");
        assert!(!Stopping.can_become(Stopping), "stop é idempotente no runtime, não na fase");
    }

    #[test]
    fn invalidation_merges() {
        assert_eq!(Invalidation::NONE.merge(Invalidation::CONTENT), Invalidation::CONTENT);
        assert_eq!(Invalidation::CONTENT.merge(Invalidation::LAYOUT), Invalidation::LAYOUT);
        assert!(Invalidation::NONE.is_none());
    }
}
