//! View declarativa de um widget (ADR-0001, D3): primitivas fechadas, sem CSS nem DOM.
//!
//! Um widget devolve uma árvore de `Node`; o host valida (`View::validate`), calcula o layout
//! (`layout.rs`) e desenha. Cores são papéis (`Role`), nunca valores: o tema decide.

use std::sync::Arc;

use crate::image::{ImageData, MAX_IMAGES};
use crate::widget::LocalId;

pub const MAX_NODES: usize = 256;
pub const MAX_TEXT_BYTES: usize = 4096;
pub const MAX_DEPTH: usize = 8;
/// Pontos de um gráfico (a soma de todos os gráficos conta para o teto de texto/nós via `MAX_CHART_POINTS`).
pub const MAX_CHART_POINTS: usize = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Primary,
    Secondary,
    Tertiary,
    Accent,
    Positive,
    Warning,
    Danger,
    /// Cores de símbolo dos cabeçalhos (paleta de sistema; viram a cor do texto no alto contraste).
    Blue,
    Indigo,
    Orange,
    Pink,
    Red,
    Clay,
    Cyan,
    /// Texto sobre preenchimento colorido (`Fill::Gradient`/`Identity`): branco.
    OnFill,
    /// Texto secundário sobre preenchimento colorido escuro (branco mais suave).
    OnFillDim,
    /// Alta (verde de painel de bolsa, igual em qualquer tema; usar sobre fundo escuro).
    Up,
    /// Baixa (vermelho de painel de bolsa).
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextStyle {
    Caption,
    /// Rótulo de cabeçalho (13 DIP, semibold): título pequeno ao lado do símbolo.
    Label,
    Body,
    Title,
    /// Numeral médio (26 DIP): 1×1 com hora ou percentual.
    Headline,
    Display,
}

impl TextStyle {
    pub fn size_dip(self) -> f32 {
        match self {
            Self::Caption => 11.0,
            Self::Label => 13.0,
            Self::Body => 13.0,
            Self::Title => 17.0,
            Self::Headline => 26.0,
            Self::Display => 34.0,
        }
    }
    pub fn line_dip(self) -> f32 {
        match self {
            Self::Caption => 14.0,
            Self::Label => 16.0,
            Self::Body => 17.0,
            Self::Title => 22.0,
            Self::Headline => 30.0,
            Self::Display => 40.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Row,
    Column,
}

/// Comprimento de um nó em um eixo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Len {
    Fixed(f32),
    /// Tamanho natural do conteúdo.
    Auto,
    /// Divide o espaço restante do eixo principal na proporção do peso (cheia, no eixo cruzado).
    Fill(u8),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fill {
    Plain,
    Accent,
    /// Cor da identidade do widget (vem do tema/instância).
    Identity,
    /// Degradê fixo de cima para baixo (cartões com cor de marca, como o News do Dockn't).
    Gradient { top: [u8; 3], bottom: [u8; 3] },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VisualKind {
    /// Relógio analógico; `secs` = segundos desde 00:00.
    AnalogClock { secs: f32, seconds_hand: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Stack { axis: Axis, gap: f32, pad: f32, main: Align, cross: Align, children: Vec<Node> },
    Spacer,
    /// Filhos sobrepostos, todos centralizados no mesmo retângulo (anel com texto no centro).
    Layers { children: Vec<Node> },
    Plate { fill: Fill, radius: f32, pad: f32, child: Box<Node> },
    Text { text: String, style: TextStyle, role: Role, align: Align, max_lines: u8 },
    Icon { glyph: char, size: f32, role: Role },
    Ring { value: f32, size: f32, thickness: f32, role: Role },
    Bar { value: f32, thickness: f32, role: Role },
    /// Barra em segmentos iguais (meses do ano): `value` (0..1) enche da esquerda, o último parcialmente.
    Segments { value: f32, count: u8, thickness: f32, role: Role },
    /// Anéis concêntricos, do externo ao interno: (valor 0..1, papel). `label` é o que o leitor de tela anuncia.
    Rings { rings: Vec<(f32, Role)>, size: f32, thickness: f32, label: String },
    Divider,
    /// Imagem entregue por um servico (capa de album): `radius` arredonda os cantos; `label` e o
    /// texto alternativo lido por leitores de tela (vazio = decorativa, nao e anunciada).
    Image { data: Arc<ImageData>, radius: f32, label: String },
    Button { id: LocalId, label: String },
    /// Gráfico de linha com área: `values` (>= 0) na escala de 0 a `max` (0 = pelo maior valor);
    /// `label` é o que o leitor de tela anuncia.
    Chart { values: Vec<f32>, max: f32, role: Role, label: String },
    /// Linha de lista clicável: rótulo à esquerda e detalhe (data, tamanho) à direita.
    Item { id: LocalId, label: String, detail: String },
    /// Caixa de seleção com rótulo (clicável, como um botão).
    Check { id: LocalId, label: String, checked: bool },
    TextField { id: LocalId, text: String },
    /// Uma linha que corre da direita para a esquerda (letreiro de manchetes). Para no "reduzir movimento".
    Marquee { text: String, style: TextStyle, role: Role },
    Visual { kind: VisualKind, size: f32 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub w: Len,
    pub h: Len,
    pub kind: Kind,
}

impl Node {
    pub fn new(kind: Kind) -> Self {
        Self { w: Len::Auto, h: Len::Auto, kind }
    }
    pub fn size(mut self, w: Len, h: Len) -> Self {
        self.w = w;
        self.h = h;
        self
    }
    pub fn fill(self) -> Self {
        self.size(Len::Fill(1), Len::Fill(1))
    }

    pub fn column(gap: f32, pad: f32, children: Vec<Node>) -> Self {
        Self::new(Kind::Stack { axis: Axis::Column, gap, pad, main: Align::Start, cross: Align::Start, children })
    }
    pub fn row(gap: f32, pad: f32, children: Vec<Node>) -> Self {
        Self::new(Kind::Stack { axis: Axis::Row, gap, pad, main: Align::Start, cross: Align::Center, children })
    }
    pub fn centered(mut self) -> Self {
        if let Kind::Stack { main, cross, .. } = &mut self.kind {
            *main = Align::Center;
            *cross = Align::Center;
        }
        self
    }
    pub fn layers(children: Vec<Node>) -> Self {
        Self::new(Kind::Layers { children })
    }
    pub fn spacer() -> Self {
        Self::new(Kind::Spacer).size(Len::Fill(1), Len::Fill(1))
    }
    pub fn plate(fill: Fill, radius: f32, pad: f32, child: Node) -> Self {
        Self::new(Kind::Plate { fill, radius, pad, child: Box::new(child) })
    }
    pub fn text(text: impl Into<String>, style: TextStyle, role: Role) -> Self {
        Self::new(Kind::Text { text: text.into(), style, role, align: Align::Start, max_lines: 1 })
    }
    pub fn icon(glyph: char, size: f32, role: Role) -> Self {
        Self::new(Kind::Icon { glyph, size, role })
    }
    pub fn ring(value: f32, size: f32, thickness: f32, role: Role) -> Self {
        Self::new(Kind::Ring { value, size, thickness, role })
    }
    pub fn bar(value: f32, thickness: f32, role: Role) -> Self {
        Self::new(Kind::Bar { value, thickness, role }).size(Len::Fill(1), Len::Auto)
    }
    pub fn segments(value: f32, count: u8, thickness: f32, role: Role) -> Self {
        Self::new(Kind::Segments { value, count, thickness, role }).size(Len::Fill(1), Len::Auto)
    }
    pub fn rings(rings: Vec<(f32, Role)>, size: f32, thickness: f32, label: impl Into<String>) -> Self {
        Self::new(Kind::Rings { rings, size, thickness, label: label.into() })
    }
    pub fn chart(values: Vec<f32>, max: f32, role: Role, label: impl Into<String>) -> Self {
        Self::new(Kind::Chart { values, max, role, label: label.into() }).size(Len::Fill(1), Len::Fill(1))
    }
    pub fn list_item(id: LocalId, label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(Kind::Item { id, label: label.into(), detail: detail.into() }).size(Len::Fill(1), Len::Auto)
    }
    pub fn check(id: LocalId, label: impl Into<String>, checked: bool) -> Self {
        Self::new(Kind::Check { id, label: label.into(), checked }).size(Len::Fill(1), Len::Auto)
    }
    pub fn image(data: Arc<ImageData>, size: f32, radius: f32, label: impl Into<String>) -> Self {
        Self::new(Kind::Image { data, radius, label: label.into() }).size(Len::Fixed(size), Len::Fixed(size))
    }
    pub fn divider() -> Self {
        Self::new(Kind::Divider).size(Len::Fill(1), Len::Auto)
    }
    pub fn visual(kind: VisualKind, size: f32) -> Self {
        Self::new(Kind::Visual { kind, size })
    }

    /// A árvore tem um letreiro (pede quadros contínuos ao host)?
    pub fn has_marquee(&self) -> bool {
        matches!(self.kind, Kind::Marquee { .. }) || self.children().iter().any(Node::has_marquee)
    }

    pub fn children(&self) -> &[Node] {
        match &self.kind {
            Kind::Stack { children, .. } | Kind::Layers { children } => children,
            Kind::Plate { child, .. } => std::slice::from_ref(child.as_ref()),
            _ => &[],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub root: Node,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewError {
    TooManyNodes,
    TooMuchText,
    TooDeep,
    BadNumber(&'static str),
    BadImage,
    TooManyImages,
    DuplicateId(u32),
}

impl View {
    pub fn new(root: Node) -> Self {
        Self { root }
    }

    pub fn node_count(&self) -> usize {
        fn count(n: &Node) -> usize {
            1 + n.children().iter().map(count).sum::<usize>()
        }
        count(&self.root)
    }

    /// Limites de tamanho/profundidade, números finitos e ids de controle únicos.
    pub fn validate(&self) -> Result<(), ViewError> {
        struct Acc {
            nodes: usize,
            text: usize,
            ids: Vec<u32>,
            images: usize,
        }
        fn finite(v: f32, what: &'static str) -> Result<(), ViewError> {
            if v.is_finite() && v >= 0.0 {
                Ok(())
            } else {
                Err(ViewError::BadNumber(what))
            }
        }
        fn len(l: Len) -> Result<(), ViewError> {
            match l {
                Len::Fixed(v) => finite(v, "fixed"),
                Len::Fill(0) => Err(ViewError::BadNumber("fill weight")),
                _ => Ok(()),
            }
        }
        fn walk(n: &Node, depth: usize, acc: &mut Acc) -> Result<(), ViewError> {
            if depth > MAX_DEPTH {
                return Err(ViewError::TooDeep);
            }
            acc.nodes += 1;
            if acc.nodes > MAX_NODES {
                return Err(ViewError::TooManyNodes);
            }
            len(n.w)?;
            len(n.h)?;
            match &n.kind {
                Kind::Stack { gap, pad, .. } => {
                    finite(*gap, "gap")?;
                    finite(*pad, "pad")?;
                }
                Kind::Plate { radius, pad, .. } => {
                    finite(*radius, "radius")?;
                    finite(*pad, "pad")?;
                }
                Kind::Text { text, .. } | Kind::Marquee { text, .. } => acc.text += text.len(),
                Kind::Icon { size, .. } | Kind::Visual { size, .. } => finite(*size, "size")?,
                Kind::Ring { value, size, thickness, .. } => {
                    finite(*value, "value")?;
                    finite(*size, "size")?;
                    finite(*thickness, "thickness")?;
                }
                Kind::Bar { value, thickness, .. } | Kind::Segments { value, thickness, .. } => {
                    finite(*value, "value")?;
                    finite(*thickness, "thickness")?;
                }
                Kind::Rings { rings, size, thickness, label } => {
                    if rings.len() > 6 {
                        return Err(ViewError::TooManyNodes);
                    }
                    for (v, _) in rings {
                        finite(*v, "value")?;
                    }
                    finite(*size, "size")?;
                    finite(*thickness, "thickness")?;
                    acc.text += label.len();
                }
                Kind::Button { id, label } | Kind::Check { id, label, .. } => {
                    acc.text += label.len();
                    acc.ids.push(id.0);
                }
                Kind::Item { id, label, detail } => {
                    acc.text += label.len() + detail.len();
                    acc.ids.push(id.0);
                }
                Kind::Chart { values, max, label, .. } => {
                    if values.len() > MAX_CHART_POINTS {
                        return Err(ViewError::TooManyNodes);
                    }
                    if values.iter().any(|v| !v.is_finite() || *v < 0.0) {
                        return Err(ViewError::BadNumber("chart value"));
                    }
                    finite(*max, "chart max")?;
                    acc.text += label.len();
                }
                Kind::TextField { id, text } => {
                    acc.text += text.len();
                    acc.ids.push(id.0);
                }
                Kind::Image { data, radius, label } => {
                    acc.images += 1;
                    if acc.images > MAX_IMAGES {
                        return Err(ViewError::TooManyImages);
                    }
                    if !data.is_valid() {
                        return Err(ViewError::BadImage);
                    }
                    finite(*radius, "radius")?;
                    acc.text += label.len();
                }
                Kind::Spacer | Kind::Divider | Kind::Layers { .. } => {}
            }
            if acc.text > MAX_TEXT_BYTES {
                return Err(ViewError::TooMuchText);
            }
            for c in n.children() {
                walk(c, depth + 1, acc)?;
            }
            Ok(())
        }
        let mut acc = Acc { nodes: 0, text: 0, ids: Vec::new(), images: 0 };
        walk(&self.root, 1, &mut acc)?;
        acc.ids.sort_unstable();
        if let Some(w) = acc.ids.windows(2).find(|w| w[0] == w[1]) {
            return Err(ViewError::DuplicateId(w[0]));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Node {
        Node::text(s, TextStyle::Body, Role::Primary)
    }

    #[test]
    fn a_small_view_is_valid_and_counted() {
        let v = View::new(Node::column(4.0, 8.0, vec![t("a"), Node::row(2.0, 0.0, vec![t("b"), Node::spacer()])]));
        assert_eq!(v.node_count(), 5);
        assert_eq!(v.validate(), Ok(()));
    }

    #[test]
    fn limits_are_enforced() {
        let wide = View::new(Node::column(0.0, 0.0, (0..MAX_NODES).map(|_| Node::spacer()).collect()));
        assert_eq!(wide.validate(), Err(ViewError::TooManyNodes), "raiz + 256 filhos passa de 256");
        let ok = View::new(Node::column(0.0, 0.0, (0..MAX_NODES - 1).map(|_| Node::spacer()).collect()));
        assert_eq!(ok.validate(), Ok(()));

        let mut deep = t("x");
        for _ in 0..MAX_DEPTH {
            deep = Node::column(0.0, 0.0, vec![deep]);
        }
        assert_eq!(View::new(deep).validate(), Err(ViewError::TooDeep));

        let big = View::new(t(&"a".repeat(MAX_TEXT_BYTES + 1)));
        assert_eq!(big.validate(), Err(ViewError::TooMuchText));
        assert_eq!(View::new(t(&"a".repeat(MAX_TEXT_BYTES))).validate(), Ok(()));
    }

    #[test]
    fn images_are_limited_and_must_be_valid() {
        use crate::image::{ImageData, MAX_IMAGES};
        let img = ImageData::gradient(4, 4, [0, 0, 0], [255, 255, 255]);
        let one = View::new(Node::image(img.clone(), 40.0, 8.0, "Capa"));
        assert_eq!(one.validate(), Ok(()));
        let many = View::new(Node::row(0.0, 0.0, (0..=MAX_IMAGES).map(|_| Node::image(img.clone(), 10.0, 0.0, "")).collect()));
        assert_eq!(many.validate(), Err(ViewError::TooManyImages));
        let broken = std::sync::Arc::new(ImageData { id: 9, width: 4, height: 4, bgra: vec![0; 3] });
        assert_eq!(View::new(Node::image(broken, 10.0, 0.0, "")).validate(), Err(ViewError::BadImage));
        let bad_radius = View::new(Node::image(img, 10.0, f32::NAN, ""));
        assert!(matches!(bad_radius.validate(), Err(ViewError::BadNumber(_))));
    }

    #[test]
    fn bad_numbers_are_rejected() {
        for bad in [f32::NAN, f32::INFINITY, -1.0] {
            let v = View::new(Node::column(bad, 0.0, vec![]));
            assert!(matches!(v.validate(), Err(ViewError::BadNumber(_))), "{bad}");
        }
        let v = View::new(t("x").size(Len::Fill(0), Len::Auto));
        assert!(matches!(v.validate(), Err(ViewError::BadNumber(_))));
        let v = View::new(Node::ring(f32::NAN, 40.0, 4.0, Role::Accent));
        assert!(matches!(v.validate(), Err(ViewError::BadNumber(_))));
    }

    #[test]
    fn control_ids_must_be_unique() {
        let b = |id| Node::new(Kind::Button { id: LocalId(id), label: "ok".into() });
        let v = View::new(Node::row(0.0, 0.0, vec![b(1), b(2), b(1)]));
        assert_eq!(v.validate(), Err(ViewError::DuplicateId(1)));
        assert_eq!(View::new(Node::row(0.0, 0.0, vec![b(1), b(2)])).validate(), Ok(()));
    }
}
