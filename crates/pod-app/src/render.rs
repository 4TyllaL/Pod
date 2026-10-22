//! Renderer das primitivas da view (ADR-0001, D3): desenha uma `View` já posicionada em qualquer
//! contexto D2D (janela real ou bitmap offscreen da galeria) e mede texto com DirectWrite.

use windows::core::{w, Interface, Result, HSTRING};
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows_numerics::Vector2;

use crate::gfx::{display_font_family, ui_font_family, Gfx};
use crate::ui::theme::{self, Rgb, Theme};
use pod_model::layout::{Laid, Rect, TextMeasure};
use pod_model::view::{Align, Fill, Kind, Node, Role, TextStyle, View, VisualKind};

pub const TILE_RADIUS_DIP: f32 = 22.0;
const ICON_FONT: windows::core::PCWSTR = w!("Segoe MDL2 Assets");

/// Selo do modo de edição no canto superior esquerdo do cartão: centro e raio em DIP.
pub const EDIT_BADGE_CENTER: f32 = 16.0;
pub const EDIT_BADGE_RADIUS: f32 = 11.0;

/// Desenha um selo redondo com "−" (remover) ou "+" (adicionar) centrado em (`cx`, `cy`).
pub fn paint_badge(ctx: &ID2D1DeviceContext, cx: f32, cy: f32, plus: bool, st: &PaintStyle) -> Result<()> {
    let fill = if plus { st.theme.role_rgb(Role::Positive, st.accent) } else { st.theme.role_rgb(Role::Danger, st.accent) };
    let on = if st.theme.mode == theme::Mode::Dark { [0x12, 0x14, 0x19] } else { [0xFF, 0xFF, 0xFF] };
    unsafe {
        let r = EDIT_BADGE_RADIUS;
        let ring = ctx.CreateSolidColorBrush(&col(st.theme.surface_top, 0.9), None)?;
        let ellipse = |rad: f32| D2D1_ELLIPSE { point: Vector2 { X: cx, Y: cy }, radiusX: rad, radiusY: rad };
        ctx.FillEllipse(&ellipse(r + 1.5), &ring);
        let body = ctx.CreateSolidColorBrush(&col(fill, 1.0), None)?;
        ctx.FillEllipse(&ellipse(r), &body);
        let glyph = ctx.CreateSolidColorBrush(&col(on, 1.0), None)?;
        let arm = r * 0.5;
        ctx.DrawLine(Vector2 { X: cx - arm, Y: cy }, Vector2 { X: cx + arm, Y: cy }, &glyph, 2.0, None);
        if plus {
            ctx.DrawLine(Vector2 { X: cx, Y: cy - arm }, Vector2 { X: cx, Y: cy + arm }, &glyph, 2.0, None);
        }
    }
    Ok(())
}

pub fn col(c: Rgb, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: c[0] as f32 / 255.0, g: c[1] as f32 / 255.0, b: c[2] as f32 / 255.0, a }
}

fn weight(style: TextStyle) -> DWRITE_FONT_WEIGHT {
    match style {
        TextStyle::Body => DWRITE_FONT_WEIGHT_NORMAL,
        TextStyle::Caption => DWRITE_FONT_WEIGHT_MEDIUM,
        TextStyle::Label => DWRITE_FONT_WEIGHT_SEMI_BOLD,
        TextStyle::Title => DWRITE_FONT_WEIGHT_SEMI_BOLD,
        TextStyle::Headline => DWRITE_FONT_WEIGHT_NORMAL,
        TextStyle::Display => DWRITE_FONT_WEIGHT_LIGHT,
    }
}

/// Layout DirectWrite de um texto: altura de linha uniforme (a do estilo), reticências quando
/// excede a caixa e algarismos tabulares (a hora não "dança" ao mudar).
pub fn text_layout(
    gfx: &Gfx,
    text: &str,
    style: TextStyle,
    max_w: f32,
    max_h: f32,
    wrap: bool,
    align: Align,
) -> Result<IDWriteTextLayout> {
    unsafe {
        let family = HSTRING::from(if matches!(style, TextStyle::Title | TextStyle::Headline | TextStyle::Display) { display_font_family() } else { ui_font_family() });
        let format = gfx.dwrite.CreateTextFormat(
            &family,
            None,
            weight(style),
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            style.size_dip(),
            w!("pt-BR"),
        )?;
        format.SetWordWrapping(if wrap { DWRITE_WORD_WRAPPING_WRAP } else { DWRITE_WORD_WRAPPING_NO_WRAP })?;
        format.SetTextAlignment(match align {
            Align::Start => DWRITE_TEXT_ALIGNMENT_LEADING,
            Align::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
            Align::End => DWRITE_TEXT_ALIGNMENT_TRAILING,
        })?;
        let line = style.line_dip();
        format.SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM, line, line * 0.8)?;
        let wide: Vec<u16> = text.encode_utf16().collect();
        let layout = gfx.dwrite.CreateTextLayout(&wide, &format, max_w.max(1.0), max_h.max(1.0))?;
        let sign = gfx.dwrite.CreateEllipsisTrimmingSign(&format)?;
        layout.SetTrimming(
            &DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 },
            &sign,
        )?;
        if !wide.is_empty() {
            let typo = gfx.dwrite.CreateTypography()?;
            typo.AddFontFeature(DWRITE_FONT_FEATURE { nameTag: DWRITE_FONT_FEATURE_TAG_TABULAR_FIGURES, parameter: 1 })?;
            layout.SetTypography(&typo, DWRITE_TEXT_RANGE { startPosition: 0, length: wide.len() as u32 })?;
        }
        Ok(layout)
    }
}

/// Medidor de texto para o motor de layout: usa as mesmas fontes que o desenho.
pub struct DwMeasure<'a>(pub &'a Gfx);

impl TextMeasure for DwMeasure<'_> {
    fn measure(&self, text: &str, style: TextStyle, max_w: Option<f32>, max_lines: u8) -> (f32, f32) {
        let lines = max_lines.max(1) as f32;
        let box_w = max_w.unwrap_or(4096.0);
        let max_h = lines * style.line_dip();
        let Ok(layout) = text_layout(self.0, text, style, box_w, max_h, max_lines > 1, Align::Start) else {
            return (0.0, style.line_dip());
        };
        let mut m = DWRITE_TEXT_METRICS::default();
        if unsafe { layout.GetMetrics(&mut m) }.is_err() {
            return (0.0, style.line_dip());
        }
        (m.width.min(box_w).ceil(), m.height.min(max_h).max(style.line_dip()))
    }
}

pub struct PaintStyle<'a> {
    pub theme: &'a Theme,
    /// Identidade da instância.
    pub accent: Rgb,
}

/// Fundo do cartão: superfície arredondada (gradiente sutil) e contorno fino.
pub fn paint_tile_background(ctx: &ID2D1DeviceContext, size: (f32, f32), st: &PaintStyle) -> Result<()> {
    let (w, h) = size;
    let th = st.theme;
    let r = TILE_RADIUS_DIP;
    unsafe {
        // Superfície limpa (estilo macOS): só o gradiente sutil do tema, sem lavagem do acento.
        let (top, bottom) = (th.surface_top, th.surface_bottom);
        let stops = [
            D2D1_GRADIENT_STOP { position: 0.0, color: col(top, th.surface_alpha) },
            D2D1_GRADIENT_STOP { position: 1.0, color: col(bottom, th.surface_alpha) },
        ];
        let rt: ID2D1RenderTarget = ctx.cast()?;
        let coll = rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)?;
        let grad = rt.CreateLinearGradientBrush(
            &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                startPoint: Vector2 { X: 0.0, Y: 0.0 },
                endPoint: Vector2 { X: 0.0, Y: h },
            },
            None,
            &coll,
        )?;
        let rr = |inset: f32| D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F { left: inset, top: inset, right: w - inset, bottom: h - inset },
            radiusX: r - inset,
            radiusY: r - inset,
        };
        ctx.FillRoundedRectangle(&rr(0.0), &grad);
        let bw = th.border_width_dip.max(1.0);
        let border = theme::blend(th.border, th.surface_top, th.border_alpha);
        let brush = ctx.CreateSolidColorBrush(&col(border, 1.0), None)?;
        ctx.DrawRoundedRectangle(&rr(bw / 2.0), &brush, bw, None);
    }
    Ok(())
}

fn round_cap(gfx: &Gfx) -> Result<ID2D1StrokeStyle> {
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

fn rect_f(r: &Rect) -> D2D_RECT_F {
    D2D_RECT_F { left: r.x, top: r.y, right: r.right(), bottom: r.bottom() }
}

fn polar(cx: f32, cy: f32, radius: f32, turns: f32) -> Vector2 {
    let a = turns * std::f32::consts::TAU;
    Vector2 { X: cx + radius * a.sin(), Y: cy - radius * a.cos() }
}

/// Desenha a view já posicionada (`laid` em pré-ordem, igual à travessia da árvore).
pub fn paint_view(ctx: &ID2D1DeviceContext, gfx: &Gfx, view: &View, laid: &[Laid], st: &PaintStyle) -> Result<()> {
    let mut idx = 0usize;
    paint_node(ctx, gfx, &view.root, laid, &mut idx, st)
}

fn paint_node(
    ctx: &ID2D1DeviceContext,
    gfx: &Gfx,
    n: &Node,
    laid: &[Laid],
    idx: &mut usize,
    st: &PaintStyle,
) -> Result<()> {
    let Some(l) = laid.get(*idx) else { return Ok(()) };
    let rc = l.rect;
    *idx += 1;
    let th = st.theme;
    let brush = |c: Rgb, a: f32| unsafe { ctx.CreateSolidColorBrush(&col(c, a), None) };
    unsafe {
        match &n.kind {
            Kind::Plate { fill, radius, .. } => {
                if let Fill::Gradient { top, bottom } = fill {
                    let hc = th.mode == theme::Mode::HighContrast;
                    let (a, b) = if hc { (th.surface_top, th.surface_top) } else { (*top, *bottom) };
                    let stops = [
                        D2D1_GRADIENT_STOP { position: 0.0, color: col(a, 1.0) },
                        D2D1_GRADIENT_STOP { position: 1.0, color: col(b, 1.0) },
                    ];
                    let rt: ID2D1RenderTarget = ctx.cast()?;
                    let coll = rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)?;
                    let grad = rt.CreateLinearGradientBrush(
                        &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                            startPoint: Vector2 { X: 0.0, Y: rc.y },
                            endPoint: Vector2 { X: 0.0, Y: rc.bottom() },
                        },
                        None,
                        &coll,
                    )?;
                    ctx.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: rect_f(&rc), radiusX: *radius, radiusY: *radius }, &grad);
                    // Sobre um preenchimento colorido o "acento" vira branco: botões e ícones ficam legíveis.
                    let on_fill = PaintStyle { theme: st.theme, accent: if hc { st.accent } else { [255, 255, 255] } };
                    for c in n.children() {
                        paint_node(ctx, gfx, c, laid, idx, &on_fill)?;
                    }
                    return Ok(());
                }
                let (c, a) = match fill {
                    Fill::Plain => (th.text, if th.mode == theme::Mode::Dark { 0.07 } else { 0.05 }),
                    Fill::Accent => (st.accent, 0.18),
                    Fill::Identity | Fill::Gradient { .. } => (st.accent, 1.0),
                };
                ctx.FillRoundedRectangle(
                    &D2D1_ROUNDED_RECT { rect: rect_f(&rc), radiusX: *radius, radiusY: *radius },
                    &brush(c, a)?,
                );
            }
            Kind::Text { text, style, role, align, max_lines } if crate::icons::single(text).is_some() => {
                // Ícone vetorial no lugar do caractere (estilo SF Symbols).
                let g = crate::icons::single(text).unwrap_or(' ');
                let side = (style.size_dip() * 1.3).min(rc.w).max(8.0);
                let cx = match align {
                    Align::Start => rc.x + side / 2.0,
                    Align::Center => rc.x + rc.w / 2.0,
                    Align::End => rc.right() - side / 2.0,
                };
                crate::icons::paint(ctx, gfx, g, cx, rc.y + rc.h / 2.0, side, &icon_palette(th, th.role_rgb(*role, st.accent)))?;
            }
            Kind::Text { text, style, role, align, max_lines } => {
                let layout = text_layout(gfx, text, *style, rc.w, rc.h, *max_lines > 1, *align)?;
                let ink = brush(th.role_rgb(*role, st.accent), 1.0)?;
                ctx.DrawTextLayout(Vector2 { X: rc.x, Y: rc.y }, &layout, &ink, D2D1_DRAW_TEXT_OPTIONS_CLIP);
            }
            Kind::Icon { glyph, size, role } => {
                let family = ICON_FONT;
                let format = gfx.dwrite.CreateTextFormat(
                    family,
                    None,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    *size,
                    w!("en-US"),
                )?;
                format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
                format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
                let mut buf = [0u16; 2];
                let wide = glyph.encode_utf16(&mut buf);
                ctx.DrawText(
                    wide,
                    &format,
                    &rect_f(&rc),
                    &brush(th.role_rgb(*role, st.accent), 1.0)?,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
            Kind::Bar { value, thickness, role } => {
                let t = thickness.min(rc.h).max(1.0);
                let y = rc.y + (rc.h - t) / 2.0;
                let track = D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F { left: rc.x, top: y, right: rc.right(), bottom: y + t },
                    radiusX: t / 2.0,
                    radiusY: t / 2.0,
                };
                let on_fill = matches!(role, Role::OnFill | Role::OnFillDim) && th.mode != theme::Mode::HighContrast;
                ctx.FillRoundedRectangle(&track, &if on_fill { brush([255, 255, 255], 0.28)? } else { brush(th.text, 0.12)? });
                let v = value.clamp(0.0, 1.0);
                if v > 0.0 {
                    let fillw = (rc.w * v).max(t);
                    let fill = D2D1_ROUNDED_RECT {
                        rect: D2D_RECT_F { left: rc.x, top: y, right: rc.x + fillw.min(rc.w), bottom: y + t },
                        radiusX: t / 2.0,
                        radiusY: t / 2.0,
                    };
                    ctx.FillRoundedRectangle(&fill, &brush(th.role_rgb(*role, st.accent), 1.0)?);
                }
            }
            Kind::Segments { value, count, thickness, role } => {
                let n = (*count).max(1) as usize;
                let t = thickness.min(rc.h).max(1.0);
                let y = rc.y + (rc.h - t) / 2.0;
                let gap = 3.0f32;
                let seg_w = ((rc.w - gap * (n as f32 - 1.0)) / n as f32).max(1.0);
                let filled = value.clamp(0.0, 1.0) * n as f32;
                let ink = brush(th.role_rgb(*role, st.accent), 1.0)?;
                let track = brush(th.text, 0.12)?;
                for i in 0..n {
                    let x = rc.x + i as f32 * (seg_w + gap);
                    let bar = |w: f32| D2D1_ROUNDED_RECT {
                        rect: D2D_RECT_F { left: x, top: y, right: x + w, bottom: y + t },
                        radiusX: t / 2.0,
                        radiusY: t / 2.0,
                    };
                    ctx.FillRoundedRectangle(&bar(seg_w), &track);
                    let f = (filled - i as f32).clamp(0.0, 1.0);
                    if f > 0.0 {
                        ctx.FillRoundedRectangle(&bar((seg_w * f).max(t.min(seg_w))), &ink);
                    }
                }
            }
            Kind::Rings { rings, thickness, .. } => {
                let size = rc.w.min(rc.h);
                let (cx, cy) = (rc.x + rc.w / 2.0, rc.y + rc.h / 2.0);
                let cap = round_cap(gfx)?;
                for (i, (value, role)) in rings.iter().enumerate() {
                    let radius = (size - thickness) / 2.0 - i as f32 * (thickness + 3.0);
                    if radius <= 0.0 {
                        break;
                    }
                    let c = th.role_rgb(*role, st.accent);
                    let ellipse = D2D1_ELLIPSE { point: Vector2 { X: cx, Y: cy }, radiusX: radius, radiusY: radius };
                    ctx.DrawEllipse(&ellipse, &brush(c, 0.2)?, *thickness, None);
                    let v = value.clamp(0.0, 1.0);
                    let ink = brush(c, 1.0)?;
                    if v >= 0.999 {
                        ctx.DrawEllipse(&ellipse, &ink, *thickness, None);
                    } else if v > 0.0 {
                        let geo = gfx.d2d_factory.CreatePathGeometry()?;
                        let sink = geo.Open()?;
                        sink.BeginFigure(polar(cx, cy, radius, 0.0), D2D1_FIGURE_BEGIN_HOLLOW);
                        sink.AddArc(&D2D1_ARC_SEGMENT {
                            point: polar(cx, cy, radius, v),
                            size: D2D_SIZE_F { width: radius, height: radius },
                            rotationAngle: 0.0,
                            sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                            arcSize: if v > 0.5 { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL },
                        });
                        sink.EndFigure(D2D1_FIGURE_END_OPEN);
                        sink.Close()?;
                        ctx.DrawGeometry(&geo, &ink, *thickness, &cap);
                    }
                }
            }
            Kind::Image { data, radius, .. } => {
                // Cria o bitmap a cada quadro: imagens são pequenas (<= 512 KiB) e o cartão só é
                // redesenhado quando a view muda.
                let props = D2D1_BITMAP_PROPERTIES1 {
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                    },
                    dpiX: 96.0,
                    dpiY: 96.0,
                    ..Default::default()
                };
                let bmp = ctx.CreateBitmap(
                    D2D_SIZE_U { width: data.width, height: data.height },
                    Some(data.bgra.as_ptr() as *const _),
                    data.width * 4,
                    &props,
                )?;
                let bb = ctx.CreateBitmapBrush(&bmp, None, None)?;
                bb.SetInterpolationMode(D2D1_BITMAP_INTERPOLATION_MODE_LINEAR);
                bb.SetTransform(&windows_numerics::Matrix3x2 {
                    M11: rc.w / data.width as f32,
                    M12: 0.0,
                    M21: 0.0,
                    M22: rc.h / data.height as f32,
                    M31: rc.x,
                    M32: rc.y,
                });
                ctx.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: rect_f(&rc), radiusX: *radius, radiusY: *radius }, &bb);
            }
            Kind::Divider => {
                let y = rc.y + rc.h / 2.0;
                ctx.FillRectangle(
                    &D2D_RECT_F { left: rc.x, top: y - 0.5, right: rc.right(), bottom: y + 0.5 },
                    &brush(th.border, th.border_alpha * 0.7)?,
                );
            }
            Kind::Ring { value, thickness, role, .. } => {
                let size = rc.w.min(rc.h);
                let (cx, cy) = (rc.x + rc.w / 2.0, rc.y + rc.h / 2.0);
                let radius = (size - thickness) / 2.0;
                let ellipse = D2D1_ELLIPSE { point: Vector2 { X: cx, Y: cy }, radiusX: radius, radiusY: radius };
                ctx.DrawEllipse(&ellipse, &brush(th.text, 0.12)?, *thickness, None);
                let v = value.clamp(0.0, 1.0);
                let ink = brush(th.role_rgb(*role, st.accent), 1.0)?;
                if v >= 0.999 {
                    ctx.DrawEllipse(&ellipse, &ink, *thickness, None);
                } else if v > 0.0 {
                    let geo = gfx.d2d_factory.CreatePathGeometry()?;
                    let sink = geo.Open()?;
                    sink.BeginFigure(polar(cx, cy, radius, 0.0), D2D1_FIGURE_BEGIN_HOLLOW);
                    sink.AddArc(&D2D1_ARC_SEGMENT {
                        point: polar(cx, cy, radius, v),
                        size: D2D_SIZE_F { width: radius, height: radius },
                        rotationAngle: 0.0,
                        sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                        arcSize: if v > 0.5 { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL },
                    });
                    sink.EndFigure(D2D1_FIGURE_END_OPEN);
                    sink.Close()?;
                    let cap = round_cap(gfx)?;
                    ctx.DrawGeometry(&geo, &ink, *thickness, &cap);
                }
            }
            Kind::Chart { values, max, role, .. } => {
                let col_c = th.role_rgb(*role, st.accent);
                let top = values.iter().cloned().fold(0.0f32, f32::max);
                let scale = if *max > 0.0 { *max } else { top.max(f32::EPSILON) };
                // Linha de base discreta.
                ctx.FillRectangle(
                    &D2D_RECT_F { left: rc.x, top: rc.bottom() - 0.5, right: rc.right(), bottom: rc.bottom() + 0.5 },
                    &brush(th.border, th.border_alpha * 0.7)?,
                );
                if values.len() >= 2 && rc.w > 2.0 && rc.h > 2.0 {
                    let n = values.len();
                    let pt = |i: usize| Vector2 {
                        X: rc.x + rc.w * i as f32 / (n - 1) as f32,
                        Y: rc.bottom() - 1.0 - (rc.h - 2.0) * (values[i] / scale).clamp(0.0, 1.0),
                    };
                    let area = gfx.d2d_factory.CreatePathGeometry()?;
                    let sink = area.Open()?;
                    sink.BeginFigure(Vector2 { X: rc.x, Y: rc.bottom() - 1.0 }, D2D1_FIGURE_BEGIN_FILLED);
                    for i in 0..n {
                        sink.AddLine(pt(i));
                    }
                    sink.AddLine(Vector2 { X: rc.right(), Y: rc.bottom() - 1.0 });
                    sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                    sink.Close()?;
                    ctx.FillGeometry(&area, &brush(col_c, 0.18)?, None::<&ID2D1Brush>);
                    let line = gfx.d2d_factory.CreatePathGeometry()?;
                    let sink = line.Open()?;
                    sink.BeginFigure(pt(0), D2D1_FIGURE_BEGIN_HOLLOW);
                    for i in 1..n {
                        sink.AddLine(pt(i));
                    }
                    sink.EndFigure(D2D1_FIGURE_END_OPEN);
                    sink.Close()?;
                    let cap = round_cap(gfx)?;
                    ctx.DrawGeometry(&line, &brush(col_c, 1.0)?, 1.6, &cap);
                }
            }
            Kind::Item { label, detail, .. } => {
                let (lw, dw) = {
                    let d = text_layout(gfx, detail, TextStyle::Caption, rc.w * 0.45, rc.h, false, Align::End)?;
                    let mut m = DWRITE_TEXT_METRICS::default();
                    d.GetMetrics(&mut m)?;
                    (rc.w - m.width.ceil() - 8.0, m.width.ceil())
                };
                let l = text_layout(gfx, label, TextStyle::Body, lw.max(1.0), rc.h, false, Align::Start)?;
                l.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
                ctx.DrawTextLayout(Vector2 { X: rc.x, Y: rc.y }, &l, &brush(th.role_rgb(Role::Primary, st.accent), 1.0)?, D2D1_DRAW_TEXT_OPTIONS_CLIP);
                if !detail.is_empty() {
                    let d = text_layout(gfx, detail, TextStyle::Caption, dw + 2.0, rc.h, false, Align::End)?;
                    d.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
                    ctx.DrawTextLayout(Vector2 { X: rc.right() - dw - 2.0, Y: rc.y }, &d, &brush(th.role_rgb(Role::Secondary, st.accent), 1.0)?, D2D1_DRAW_TEXT_OPTIONS_CLIP);
                }
            }
            Kind::Check { label, checked, .. } => {
                let bx = D2D_RECT_F { left: rc.x, top: rc.y + (rc.h - 16.0) / 2.0, right: rc.x + 16.0, bottom: rc.y + (rc.h - 16.0) / 2.0 + 16.0 };
                let rr = D2D1_ROUNDED_RECT { rect: bx, radiusX: 4.0, radiusY: 4.0 };
                if *checked {
                    ctx.FillRoundedRectangle(&rr, &brush(st.accent, 1.0)?);
                    let tick = brush(th.on_accent, 1.0)?;
                    let cap = round_cap(gfx)?;
                    let (x, y) = (bx.left, bx.top);
                    ctx.DrawLine(Vector2 { X: x + 4.0, Y: y + 8.5 }, Vector2 { X: x + 7.0, Y: y + 11.5 }, &tick, 2.0, &cap);
                    ctx.DrawLine(Vector2 { X: x + 7.0, Y: y + 11.5 }, Vector2 { X: x + 12.0, Y: y + 5.0 }, &tick, 2.0, &cap);
                } else {
                    ctx.DrawRoundedRectangle(&rr, &brush(th.text_secondary, 1.0)?, 1.5, None);
                }
                let role = if *checked { Role::Tertiary } else { Role::Primary };
                let layout = text_layout(gfx, label, TextStyle::Body, (rc.w - 24.0).max(1.0), rc.h, false, Align::Start)?;
                layout.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
                ctx.DrawTextLayout(
                    Vector2 { X: rc.x + 24.0, Y: rc.y },
                    &layout,
                    &brush(th.role_rgb(role, st.accent), 1.0)?,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                );
            }
            Kind::Button { label, .. } => {
                // Botão "tonal" (estilo Apple): pílula com a cor de identidade em ~18% e rótulo na cor
                // cheia. No alto contraste volta ao preenchimento sólido com a cor de texto do sistema.
                let hc = th.mode == theme::Mode::HighContrast;
                let pill = (rc.h / 2.0).min(16.0);
                let (fill, ink) = if hc {
                    (brush(st.accent, 1.0)?, brush(th.on_accent, 1.0)?)
                } else {
                    (brush(st.accent, 0.18)?, brush(th.role_rgb(Role::Accent, st.accent), 1.0)?)
                };
                ctx.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: rect_f(&rc), radiusX: pill, radiusY: pill }, &fill);
                if let Some(g) = crate::icons::single(label) {
                    let c = if hc { th.on_accent } else { th.role_rgb(Role::Accent, st.accent) };
                    let side = (rc.h * 0.52).clamp(10.0, 20.0);
                    crate::icons::paint(ctx, gfx, g, rc.x + rc.w / 2.0, rc.y + rc.h / 2.0, side, &icon_palette(th, c))?;
                } else {
                    let layout = text_layout(gfx, label, TextStyle::Body, rc.w, rc.h, false, Align::Center)?;
                    layout.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
                    ctx.DrawTextLayout(Vector2 { X: rc.x, Y: rc.y }, &layout, &ink, D2D1_DRAW_TEXT_OPTIONS_CLIP);
                }
            }
            Kind::Visual { kind: VisualKind::AnalogClock { secs, seconds_hand }, .. } => {
                let size = rc.w.min(rc.h);
                let (cx, cy) = (rc.x + rc.w / 2.0, rc.y + rc.h / 2.0);
                let r = size / 2.0 - 1.0;
                let face = D2D1_ELLIPSE { point: Vector2 { X: cx, Y: cy }, radiusX: r, radiusY: r };
                ctx.FillEllipse(&face, &brush(th.text, if th.mode == theme::Mode::Dark { 0.09 } else { 0.06 })?);
                if th.mode == theme::Mode::HighContrast {
                    ctx.DrawEllipse(&face, &brush(th.text, 1.0)?, 1.5, None);
                }
                let tick = brush(th.text_secondary, 0.75)?;
                for i in 0..12 {
                    let t = i as f32 / 12.0;
                    ctx.DrawLine(polar(cx, cy, r * 0.90, t), polar(cx, cy, r * 0.96, t), &tick, 1.2, None);
                }
                // Números nos quatro pontos cardeais.
                let ink = brush(th.text, 1.0)?;
                for (n, t) in [("12", 0.0f32), ("3", 0.25), ("6", 0.5), ("9", 0.75)] {
                    let p = polar(cx, cy, r * 0.70, t);
                    let layout = text_layout(gfx, n, TextStyle::Label, 40.0, 16.0, false, Align::Center)?;
                    ctx.DrawTextLayout(Vector2 { X: p.X - 20.0, Y: p.Y - 8.0 }, &layout, &ink, D2D1_DRAW_TEXT_OPTIONS_CLIP);
                }
                let cap = round_cap(gfx)?;
                let hour = (secs / 3600.0).rem_euclid(12.0) / 12.0;
                let minute = (secs / 60.0).rem_euclid(60.0) / 60.0;
                let second = secs.rem_euclid(60.0) / 60.0;
                let c = Vector2 { X: cx, Y: cy };
                let orange = th.role_rgb(Role::Orange, st.accent);
                ctx.DrawLine(c, polar(cx, cy, r * 0.46, hour), &ink, 4.2, &cap);
                ctx.DrawLine(c, polar(cx, cy, r * 0.72, minute), &ink, 3.0, &cap);
                if *seconds_hand {
                    ctx.DrawLine(polar(cx, cy, -r * 0.16, second), polar(cx, cy, r * 0.86, second), &brush(orange, 1.0)?, 1.5, &cap);
                }
                ctx.FillEllipse(&D2D1_ELLIPSE { point: c, radiusX: 4.0, radiusY: 4.0 }, &brush(orange, 1.0)?);
                ctx.FillEllipse(&D2D1_ELLIPSE { point: c, radiusX: 1.5, radiusY: 1.5 }, &brush(th.surface_top, 1.0)?);
            }
            Kind::Marquee { text, style, role } => {
                // Letreiro: a linha (e cópias com um respiro de 48 DIP) corre a 34 DIP/s depois de
                // 1,2 s parada, recortada à caixa. Parado com "reduzir movimento" e na galeria.
                let layout = text_layout(gfx, text, *style, 100_000.0, rc.h, false, Align::Start)?;
                let mut m = DWRITE_TEXT_METRICS::default();
                layout.GetMetrics(&mut m)?;
                let span = m.widthIncludingTrailingWhitespace + 48.0;
                let t = marquee_seconds();
                let off = if m.width <= rc.w { 0.0 } else { ((t - 1.2).max(0.0) * 34.0) % span };
                let ink = brush(th.role_rgb(*role, st.accent), 1.0)?;
                ctx.PushAxisAlignedClip(&rect_f(&rc), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                let mut x = rc.x - off;
                while x < rc.right() {
                    ctx.DrawTextLayout(Vector2 { X: x, Y: rc.y }, &layout, &ink, D2D1_DRAW_TEXT_OPTIONS_NONE);
                    if m.width <= rc.w {
                        break;
                    }
                    x += span;
                }
                ctx.PopAxisAlignedClip();
            }
            Kind::Stack { .. } | Kind::Layers { .. } | Kind::Spacer | Kind::TextField { .. } => {}
        }
    }
    for c in n.children() {
        paint_node(ctx, gfx, c, laid, idx, st)?;
    }
    Ok(())
}

/// Cores dos ícones vetoriais: traço do papel, nuvem neutra e tom quente (sol, raio).
fn icon_palette(th: &Theme, ink: Rgb) -> crate::icons::Palette {
    let dark = th.mode == theme::Mode::Dark;
    crate::icons::Palette {
        ink,
        neutral: if dark { [0xC9, 0xCE, 0xD8] } else { [0x8E, 0x97, 0xA8] },
        warm: if dark { [0xFF, 0xCC, 0x1A] } else { [0xFF, 0x9F, 0x0A] },
    }
}

static MARQUEE_START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
/// Galeria e testes: letreiros parados (imagens determinísticas).
pub static MARQUEE_FROZEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn marquee_seconds() -> f32 {
    if cfg!(test) || MARQUEE_FROZEN.load(std::sync::atomic::Ordering::Relaxed) || crate::sysinfo::reduce_motion() {
        return 0.0;
    }
    MARQUEE_START.get_or_init(std::time::Instant::now).elapsed().as_secs_f32()
}

/// Papel usado quando o widget não declara um (testes e fallback).
#[allow(dead_code)]
pub const DEFAULT_ROLE: Role = Role::Primary;
