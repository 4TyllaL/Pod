//! Ícones vetoriais próprios (estilo SF Symbols): traço arredondado, formas simples, sem dependência de
//! fonte de emoji. Um widget pede um ícone usando um caractere conhecido (☀, ☁, ▶…) num texto ou rótulo
//! de botão; o renderer troca o caractere por este desenho. Caracteres desconhecidos seguem como texto.

use windows::core::{Interface, Result};
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows_numerics::Vector2;

use crate::gfx::Gfx;
use crate::render::col;
use crate::ui::theme::Rgb;

/// O caractere tem desenho vetorial?
pub fn has(glyph: char) -> bool {
    matches!(
        glyph,
        '\u{2600}' | '\u{263e}' | '\u{26c5}' | '\u{2601}' | '\u{2602}' | '\u{2744}' | '\u{26a1}' | '\u{266a}'
            | '\u{25b2}' | '\u{25bc}' | '\u{23ee}' | '\u{23ed}' | '\u{25b6}' | '\u{25c0}' | '\u{23f8}' | '\u{2728}' | '\u{1f4f0}' | '\u{2197}'
            | '\u{2713}' | '\u{1f4a7}' | '\u{21c5}' | '\u{25a3}' | '\u{23f1}' | '\u{1f50b}'
    )
}

/// Se `text` é exatamente um ícone conhecido, devolve o caractere.
pub fn single(text: &str) -> Option<char> {
    let mut it = text.chars();
    let c = it.next()?;
    (it.next().is_none() && has(c)).then_some(c)
}

pub struct Palette {
    /// Cor do papel pedido pelo widget (traços e formas principais).
    pub ink: Rgb,
    /// Neutro para nuvens.
    pub neutral: Rgb,
    /// Tom quente de sol e raio.
    pub warm: Rgb,
}

type Pt = (f32, f32);

struct Pen<'a> {
    ctx: &'a ID2D1DeviceContext,
    gfx: &'a Gfx,
    cap: ID2D1StrokeStyle,
    cx: f32,
    cy: f32,
    s: f32,
}

impl Pen<'_> {
    fn p(&self, x: f32, y: f32) -> Vector2 {
        Vector2 { X: self.cx + x * self.s, Y: self.cy + y * self.s }
    }
    fn brush(&self, c: Rgb) -> Result<ID2D1SolidColorBrush> {
        unsafe { self.ctx.CreateSolidColorBrush(&col(c, 1.0), None) }
    }
    fn line(&self, a: Pt, b: Pt, w: f32, c: Rgb) -> Result<()> {
        unsafe {
            self.ctx.DrawLine(self.p(a.0, a.1), self.p(b.0, b.1), &self.brush(c)?, w * self.s, &self.cap);
        }
        Ok(())
    }
    fn disc(&self, x: f32, y: f32, r: f32, c: Rgb) -> Result<()> {
        let e = D2D1_ELLIPSE { point: self.p(x, y), radiusX: r * self.s, radiusY: r * self.s };
        unsafe { self.ctx.FillEllipse(&e, &self.brush(c)?) };
        Ok(())
    }
    /// Polígono preenchido com cantos arredondados (preenche e contorna com junção redonda).
    fn poly(&self, pts: &[Pt], round: f32, c: Rgb) -> Result<()> {
        unsafe {
            let g = self.gfx.d2d_factory.CreatePathGeometry()?;
            let sink = g.Open()?;
            sink.BeginFigure(self.p(pts[0].0, pts[0].1), D2D1_FIGURE_BEGIN_FILLED);
            let rest: Vec<Vector2> = pts[1..].iter().map(|q| self.p(q.0, q.1)).collect();
            sink.AddLines(&rest);
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            sink.Close()?;
            let b = self.brush(c)?;
            self.ctx.FillGeometry(&g, &b, None);
            if round > 0.0 {
                self.ctx.DrawGeometry(&g, &b, round * self.s, &self.cap);
            }
        }
        Ok(())
    }
    /// Nuvem: união de círculos e uma base arredondada (todas as formas no mesmo sentido).
    fn cloud(&self, dx: f32, dy: f32, k: f32, c: Rgb) -> Result<()> {
        unsafe {
            let f = &self.gfx.d2d_factory;
            let ell = |x: f32, y: f32, r: f32| -> Result<ID2D1Geometry> {
                let e = D2D1_ELLIPSE { point: self.p(dx + x * k, dy + y * k), radiusX: r * k * self.s, radiusY: r * k * self.s };
                Ok(f.CreateEllipseGeometry(&e)?.cast()?)
            };
            let base = {
                let (l, t, r, b) = (dx - 0.42 * k, dy + 0.02 * k, dx + 0.42 * k, dy + 0.30 * k);
                let (a, bb) = (self.p(l, t), self.p(r, b));
                let rr = D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F { left: a.X, top: a.Y, right: bb.X, bottom: bb.Y },
                    radiusX: 0.14 * k * self.s,
                    radiusY: 0.14 * k * self.s,
                };
                let g: ID2D1Geometry = f.CreateRoundedRectangleGeometry(&rr)?.cast()?;
                g
            };
            let parts = [Some(ell(-0.16, 0.06, 0.2)?), Some(ell(0.1, -0.1, 0.27)?), Some(ell(0.3, 0.1, 0.17)?), Some(base)];
            let group = f.CreateGeometryGroup(D2D1_FILL_MODE_WINDING, &parts)?;
            self.ctx.FillGeometry(&group, &self.brush(c)?, None);
        }
        Ok(())
    }
    fn sun(&self, x: f32, y: f32, r: f32, rays: bool, c: Rgb) -> Result<()> {
        self.disc(x, y, r, c)?;
        if rays {
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let (ca, sa) = (a.cos(), a.sin());
                self.line((x + ca * (r + 0.1), y + sa * (r + 0.1)), (x + ca * (r + 0.24), y + sa * (r + 0.24)), 0.07, c)?;
            }
        }
        Ok(())
    }
}

/// Desenha o ícone de `glyph` centrado em (`cx`, `cy`) dentro de um quadrado de lado `side`.
pub fn paint(ctx: &ID2D1DeviceContext, gfx: &Gfx, glyph: char, cx: f32, cy: f32, side: f32, pal: &Palette) -> Result<()> {
    let cap = round_stroke(gfx)?;
    let pen = Pen { ctx, gfx, cap, cx, cy, s: side };
    let (ink, neutral, warm) = (pal.ink, pal.neutral, pal.warm);
    match glyph {
        '\u{2600}' => pen.sun(0.0, 0.0, 0.2, true, warm)?,
        '\u{263e}' => {
            // Lua: disco menos outro deslocado.
            unsafe {
                let f = &gfx.d2d_factory;
                let big = f.CreateEllipseGeometry(&D2D1_ELLIPSE { point: pen.p(-0.02, 0.0), radiusX: 0.3 * side, radiusY: 0.3 * side })?;
                let cut = f.CreateEllipseGeometry(&D2D1_ELLIPSE { point: pen.p(0.14, -0.1), radiusX: 0.26 * side, radiusY: 0.26 * side })?;
                let path = f.CreatePathGeometry()?;
                let sink = path.Open()?;
                big.CombineWithGeometry(&cut, D2D1_COMBINE_MODE_EXCLUDE, None, 0.1, &sink)?;
                sink.Close()?;
                ctx.FillGeometry(&path, &pen.brush(warm)?, None);
            }
        }
        '\u{26c5}' => {
            pen.sun(-0.12, -0.14, 0.12, true, warm)?;
            // Recorte do sol atrás da nuvem: contorno na cor da superfície não existe aqui; a nuvem cobre.
            pen.cloud(0.06, 0.1, 0.9, neutral)?;
        }
        '\u{2601}' => pen.cloud(0.0, 0.0, 1.05, neutral)?,
        '\u{2602}' => {
            pen.cloud(0.0, -0.12, 0.95, neutral)?;
            for x in [-0.22, 0.0, 0.22] {
                pen.line((x + 0.05, 0.2), (x - 0.03, 0.36), 0.07, ink)?;
            }
        }
        '\u{2744}' => {
            for i in 0..3 {
                let a = i as f32 * std::f32::consts::PI / 3.0 + std::f32::consts::FRAC_PI_2;
                let (ca, sa) = (a.cos() * 0.34, a.sin() * 0.34);
                pen.line((-ca, -sa), (ca, sa), 0.07, ink)?;
            }
        }
        '\u{26a1}' => pen.poly(&[(0.08, -0.4), (-0.22, 0.06), (-0.02, 0.06), (-0.1, 0.4), (0.22, -0.08), (0.02, -0.08)], 0.03, warm)?,
        '\u{266a}' => {
            pen.disc(-0.12, 0.26, 0.15, ink)?;
            pen.line((0.01, 0.24), (0.01, -0.36), 0.08, ink)?;
            pen.poly(&[(0.01, -0.36), (0.3, -0.2), (0.3, -0.04), (0.01, -0.2)], 0.03, ink)?;
        }
        '\u{25b2}' => pen.poly(&[(0.0, -0.3), (0.34, 0.26), (-0.34, 0.26)], 0.05, ink)?,
        '\u{25bc}' => pen.poly(&[(0.0, 0.3), (0.34, -0.26), (-0.34, -0.26)], 0.05, ink)?,
        '\u{25b6}' => pen.poly(&[(-0.2, -0.3), (0.32, 0.0), (-0.2, 0.3)], 0.08, ink)?,
        '\u{25c0}' => pen.poly(&[(0.2, -0.3), (-0.32, 0.0), (0.2, 0.3)], 0.08, ink)?,
        '\u{23f8}' => {
            pen.line((-0.14, -0.28), (-0.14, 0.28), 0.17, ink)?;
            pen.line((0.14, -0.28), (0.14, 0.28), 0.17, ink)?;
        }
        '\u{23ed}' => {
            pen.poly(&[(-0.32, -0.26), (0.12, 0.0), (-0.32, 0.26)], 0.07, ink)?;
            pen.line((0.26, -0.26), (0.26, 0.26), 0.1, ink)?;
        }
        '\u{23ee}' => {
            pen.poly(&[(0.32, -0.26), (-0.12, 0.0), (0.32, 0.26)], 0.07, ink)?;
            pen.line((-0.26, -0.26), (-0.26, 0.26), 0.1, ink)?;
        }
        '\u{2728}' => {
            // Brilho de quatro pontas e um menor.
            let star = |x: f32, y: f32, r: f32| -> Result<()> {
                let k = 0.22;
                pen.poly(
                    &[(x, y - r), (x + r * k, y - r * k), (x + r, y), (x + r * k, y + r * k), (x, y + r), (x - r * k, y + r * k), (x - r, y), (x - r * k, y - r * k)],
                    0.03,
                    ink,
                )
            };
            star(-0.06, 0.04, 0.34)?;
            star(0.27, -0.27, 0.14)?;
        }
        '\u{1f4f0}' => {
            unsafe {
                let (a, b) = (pen.p(-0.34, -0.3), pen.p(0.34, 0.3));
                let rr = D2D1_ROUNDED_RECT { rect: D2D_RECT_F { left: a.X, top: a.Y, right: b.X, bottom: b.Y }, radiusX: 0.07 * side, radiusY: 0.07 * side };
                ctx.DrawRoundedRectangle(&rr, &pen.brush(ink)?, 0.07 * side, None);
            }
            pen.line((-0.2, -0.14), (0.2, -0.14), 0.07, ink)?;
            pen.line((-0.2, 0.02), (0.2, 0.02), 0.07, ink)?;
            pen.line((-0.2, 0.18), (0.06, 0.18), 0.07, ink)?;
        }
        '\u{2197}' => {
            pen.line((-0.24, 0.24), (0.24, -0.24), 0.09, ink)?;
            pen.line((-0.02, -0.26), (0.26, -0.26), 0.09, ink)?;
            pen.line((0.26, -0.26), (0.26, 0.02), 0.09, ink)?;
        }
        '\u{2713}' => {
            pen.line((-0.26, 0.02), (-0.08, 0.2), 0.12, ink)?;
            pen.line((-0.08, 0.2), (0.28, -0.22), 0.12, ink)?;
        }
        '\u{1f4a7}' => pen.poly(&[(0.0, -0.38), (0.2, -0.02), (0.24, 0.14), (0.12, 0.3), (-0.12, 0.3), (-0.24, 0.14), (-0.2, -0.02)], 0.06, ink)?,
        '\u{21c5}' => {
            pen.line((-0.14, 0.28), (-0.14, -0.28), 0.09, ink)?;
            pen.line((-0.14, -0.28), (-0.28, -0.12), 0.09, ink)?;
            pen.line((-0.14, -0.28), (0.0, -0.12), 0.09, ink)?;
            pen.line((0.14, -0.28), (0.14, 0.28), 0.09, ink)?;
            pen.line((0.14, 0.28), (0.0, 0.12), 0.09, ink)?;
            pen.line((0.14, 0.28), (0.28, 0.12), 0.09, ink)?;
        }
        '\u{25a3}' => {
            unsafe {
                let (a, b) = (pen.p(-0.26, -0.26), pen.p(0.26, 0.26));
                let rr = D2D1_ROUNDED_RECT { rect: D2D_RECT_F { left: a.X, top: a.Y, right: b.X, bottom: b.Y }, radiusX: 0.1 * side, radiusY: 0.1 * side };
                ctx.DrawRoundedRectangle(&rr, &pen.brush(ink)?, 0.09 * side, None);
            }
            pen.poly(&[(-0.09, -0.09), (0.09, -0.09), (0.09, 0.09), (-0.09, 0.09)], 0.02, ink)?;
        }
        '\u{23f1}' => {
            unsafe {
                let e = D2D1_ELLIPSE { point: pen.p(0.0, 0.0), radiusX: 0.3 * side, radiusY: 0.3 * side };
                ctx.DrawEllipse(&e, &pen.brush(ink)?, 0.09 * side, None);
            }
            pen.line((0.0, 0.0), (0.0, -0.17), 0.09, ink)?;
            pen.line((0.0, 0.0), (0.12, 0.07), 0.09, ink)?;
        }
        '\u{1f50b}' => {
            unsafe {
                let (a, b) = (pen.p(-0.34, -0.18), pen.p(0.26, 0.18));
                let rr = D2D1_ROUNDED_RECT { rect: D2D_RECT_F { left: a.X, top: a.Y, right: b.X, bottom: b.Y }, radiusX: 0.08 * side, radiusY: 0.08 * side };
                ctx.DrawRoundedRectangle(&rr, &pen.brush(ink)?, 0.08 * side, None);
            }
            pen.line((0.35, -0.06), (0.35, 0.06), 0.09, ink)?;
            pen.poly(&[(-0.24, -0.08), (0.04, -0.08), (0.04, 0.08), (-0.24, 0.08)], 0.02, ink)?;
        }
        _ => {}
    }
    Ok(())
}

fn round_stroke(gfx: &Gfx) -> Result<ID2D1StrokeStyle> {
    let props = D2D1_STROKE_STYLE_PROPERTIES1 {
        startCap: D2D1_CAP_STYLE_ROUND,
        endCap: D2D1_CAP_STYLE_ROUND,
        dashCap: D2D1_CAP_STYLE_ROUND,
        lineJoin: D2D1_LINE_JOIN_ROUND,
        miterLimit: 10.0,
        dashStyle: D2D1_DASH_STYLE_SOLID,
        dashOffset: 0.0,
        transformType: D2D1_STROKE_TRANSFORM_TYPE_NORMAL,
    };
    unsafe { gfx.d2d_factory.CreateStrokeStyle(&props, None)?.cast() }
}
