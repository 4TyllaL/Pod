//! Dispositivos gráficos compartilhados e superfície de composição por janela.
//! Tudo aqui pertence à thread de UI (STA).

use std::cell::Cell;

use std::sync::OnceLock;

use windows::core::{w, Interface, Result, HRESULT, HSTRING};
use windows_numerics::Vector2;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::DirectComposition::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;

use crate::text::TextEditor;
use crate::ui::theme::{self, Rgb, Theme};
use crate::ui::{Placed, PlacedKind};
use crate::ui::note::NoteLayout;
use pod_model::widget::LocalId;

thread_local! {
    /// Só para teste: enquanto > 0, todo desenho falha como se o dispositivo tivesse sido removido.
    static FORCE_LOSS_ROUNDS: Cell<u32> = const { Cell::new(0) };
}

/// Simula `n` rodadas de perda de dispositivo (cada reconstrução consome uma).
pub fn force_loss_rounds(n: u32) {
    FORCE_LOSS_ROUNDS.with(|c| c.set(n));
}

pub fn consume_loss_round() {
    FORCE_LOSS_ROUNDS.with(|c| c.set(c.get().saturating_sub(1)));
}

static UI_FONT: OnceLock<String> = OnceLock::new();
static DISPLAY_FONT: OnceLock<String> = OnceLock::new();

/// Família para títulos e numerais grandes (tamanho óptico "Display", como o SF Display da Apple):
/// Segoe UI Variable Display se instalada, senão a família de texto.
pub fn display_font_family() -> &'static str {
    DISPLAY_FONT.get().map(String::as_str).unwrap_or_else(ui_font_family)
}

/// Família da interface: Segoe UI Variable Text (Windows 11) se instalada, senão Segoe UI.
pub fn ui_font_family() -> &'static str {
    UI_FONT.get().map(String::as_str).unwrap_or("Segoe UI")
}

fn pick_display(dwrite: &IDWriteFactory, fallback: &str) -> String {
    unsafe {
        let mut coll: Option<IDWriteFontCollection> = None;
        if dwrite.GetSystemFontCollection(&mut coll, false).is_ok() {
            if let Some(coll) = coll {
                let (mut index, mut exists) = (0u32, windows::core::BOOL(0));
                if coll.FindFamilyName(w!("Segoe UI Variable Display"), &mut index, &mut exists).is_ok() && exists.as_bool() {
                    return "Segoe UI Variable Display".to_string();
                }
            }
        }
    }
    fallback.to_string()
}

fn pick_family(dwrite: &IDWriteFactory) -> String {
    unsafe {
        let mut coll: Option<IDWriteFontCollection> = None;
        if dwrite.GetSystemFontCollection(&mut coll, false).is_ok() {
            if let Some(coll) = coll {
                let (mut index, mut exists) = (0u32, windows::core::BOOL(0));
                if coll.FindFamilyName(w!("Segoe UI Variable Text"), &mut index, &mut exists).is_ok() && exists.as_bool() {
                    return "Segoe UI Variable Text".to_string();
                }
            }
        }
    }
    "Segoe UI".to_string()
}

const DXGI_ERROR_DEVICE_HUNG: i32 = 0x887A0006_u32 as i32;
const DXGI_ERROR_DEVICE_REMOVED: i32 = 0x887A0005_u32 as i32;
const DXGI_ERROR_DEVICE_RESET: i32 = 0x887A0007_u32 as i32;
const D2DERR_RECREATE_TARGET: i32 = 0x8899000C_u32 as i32;

/// O erro indica que o dispositivo gráfico precisa ser recriado (os dados de UI continuam válidos).
pub fn is_device_lost(e: &windows::core::Error) -> bool {
    matches!(
        e.code().0,
        DXGI_ERROR_DEVICE_REMOVED | DXGI_ERROR_DEVICE_RESET | DXGI_ERROR_DEVICE_HUNG | D2DERR_RECREATE_TARGET
    )
}

/// Dispositivos únicos por processo.
pub struct Gfx {
    /// Renderizando em software (WARP) depois de falhas repetidas do adaptador.
    pub warp: bool,
    pub dxgi_factory: IDXGIFactory2,
    pub d2d_device: ID2D1Device,
    pub d3d_device: ID3D11Device,
    pub dcomp: IDCompositionDevice,
    pub dwrite: IDWriteFactory,
    pub d2d_factory: ID2D1Factory1,
    pub label_format: IDWriteTextFormat,
    pub button_format: IDWriteTextFormat,
    pub status_format: IDWriteTextFormat,
    pub note_label_format: IDWriteTextFormat,
    pub note_body_formats: [IDWriteTextFormat; 3],
}

impl Gfx {
    pub fn new(warp: bool) -> Result<Self> {
        unsafe {
            let mut d3d: Option<ID3D11Device> = None;
            D3D11CreateDevice(
                None,
                if warp { D3D_DRIVER_TYPE_WARP } else { D3D_DRIVER_TYPE_HARDWARE },
                Default::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut d3d),
                None,
                None,
            )?;
            let d3d_device = d3d.expect("D3D11CreateDevice sem erro mas sem device");
            let dxgi_device: IDXGIDevice = d3d_device.cast()?;
            let dxgi_factory: IDXGIFactory2 = CreateDXGIFactory1()?;

            let d2d_factory: ID2D1Factory1 =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let d2d_device = d2d_factory.CreateDevice(&dxgi_device)?;
            let dcomp: IDCompositionDevice = DCompositionCreateDevice(&dxgi_device)?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let family = UI_FONT.get_or_init(|| pick_family(&dwrite)).clone();
            DISPLAY_FONT.get_or_init(|| pick_display(&dwrite, &family));
            let family = HSTRING::from(family);
            let make = |size: f32, weight: DWRITE_FONT_WEIGHT| -> Result<IDWriteTextFormat> {
                dwrite.CreateTextFormat(
                    &family,
                    None,
                    weight,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    w!("pt-BR"),
                )
            };
            let label_format = make(13.0, DWRITE_FONT_WEIGHT_NORMAL)?;
            let button_format = make(13.0, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
            button_format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            button_format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            let status_format = make(12.0, DWRITE_FONT_WEIGHT_NORMAL)?;
            status_format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_TRAILING)?;
            status_format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            let note_label_format = make(12.0, DWRITE_FONT_WEIGHT_NORMAL)?;
            let note_body_formats = [make(16.0, DWRITE_FONT_WEIGHT_NORMAL)?, make(22.0, DWRITE_FONT_WEIGHT_NORMAL)?, make(28.0, DWRITE_FONT_WEIGHT_NORMAL)?];
            Ok(Self { warp, dxgi_factory, d2d_device, d3d_device, dcomp, dwrite, d2d_factory, label_format, button_format, status_format, note_label_format, note_body_formats })
        }
    }
}

/// Superfície de uma janela: swap chain de composição ligada a um visual.
pub struct Surface {
    swap: IDXGISwapChain1,
    ctx: ID2D1DeviceContext,
    target: Option<ID2D1Bitmap1>,
    _dcomp_target: IDCompositionTarget,
    _visual: IDCompositionVisual,
    pub width: u32,
    pub height: u32,
}

impl Surface {
    pub fn new(gfx: &Gfx, hwnd: HWND, width: u32, height: u32) -> Result<Self> {
        let (width, height) = (width.max(1), height.max(1));
        unsafe {
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: width,
                Height: height,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
                ..Default::default()
            };
            let swap = gfx.dxgi_factory.CreateSwapChainForComposition(&gfx.d3d_device, &desc, None)?;
            let dcomp_target = gfx.dcomp.CreateTargetForHwnd(hwnd, true)?;
            let visual = gfx.dcomp.CreateVisual()?;
            visual.SetContent(&swap)?;
            dcomp_target.SetRoot(&visual)?;
            gfx.dcomp.Commit()?;
            let ctx = gfx.d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            let mut s = Self {
                swap,
                ctx,
                target: None,
                _dcomp_target: dcomp_target,
                _visual: visual,
                width,
                height,
            };
            s.bind_target()?;
            Ok(s)
        }
    }

    fn bind_target(&mut self) -> Result<()> {
        unsafe {
            let surface: IDXGISurface = self.swap.GetBuffer(0)?;
            let props = D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                ..Default::default()
            };
            let bmp = self.ctx.CreateBitmapFromDxgiSurface(&surface, Some(&props))?;
            self.ctx.SetTarget(&bmp);
            self.target = Some(bmp);
        }
        Ok(())
    }

    /// Libera referências aos buffers antes de `ResizeBuffers`; ignora dimensão zero.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        if width == 0 || height == 0 || (width == self.width && height == self.height) {
            return Ok(());
        }
        unsafe {
            self.ctx.SetTarget(None);
            self.target = None;
            self.swap.ResizeBuffers(0, width, height, DXGI_FORMAT_UNKNOWN, DXGI_SWAP_CHAIN_FLAG(0))?;
        }
        self.width = width;
        self.height = height;
        self.bind_target()
    }

    /// Desenha o cartão inteiro em DIPs (o contexto D2D usa o DPI da janela).
    pub fn draw(&self, gfx: &Gfx, p: &DrawParams) -> Result<()> {
        if FORCE_LOSS_ROUNDS.with(|c| c.get()) > 0 {
            return Err(windows::core::Error::from(HRESULT(DXGI_ERROR_DEVICE_REMOVED)));
        }
        let started = std::time::Instant::now();
        let s = p.dpi as f32 / 96.0;
        let (w, h) = (self.width as f32 / s, self.height as f32 / s);
        let th = p.theme;
        let note = NoteLayout::new(w, h);
        let r = note.radius;
        unsafe {
            self.ctx.SetDpi(p.dpi as f32, p.dpi as f32);
            self.ctx.BeginDraw();
            self.ctx.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
            let brush = |c: Rgb, a: f32| self.ctx.CreateSolidColorBrush(&col(c, a), None);
            let rrect = |l: f32, t: f32, rr: f32, b: f32, rad: f32| D2D1_ROUNDED_RECT {
                rect: D2D_RECT_F { left: l, top: t, right: rr, bottom: b },
                radiusX: rad,
                radiusY: rad,
            };
            let ink = brush(th.text, 1.0)?;

            // Superfície: gradiente vertical sutil (opaca, legível).
            let stops = [
                D2D1_GRADIENT_STOP { position: 0.0, color: col(th.surface_top, th.surface_alpha) },
                D2D1_GRADIENT_STOP { position: 1.0, color: col(th.surface_bottom, th.surface_alpha) },
            ];
            let rt: ID2D1RenderTarget = self.ctx.cast()?;
            let coll = rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)?;
            let grad = rt.CreateLinearGradientBrush(
                &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                    startPoint: Vector2 { X: 0.0, Y: 0.0 },
                    endPoint: Vector2 { X: 0.0, Y: h },
                },
                None,
                &coll,
            )?;
            self.ctx.FillRoundedRectangle(&rrect(0.0, 0.0, w, h, r), &grad);

            // Editorial: cabeçalho integrado ao papel, sem faixa tingida ou separador.
            let label = note.label;
            let label_rect = D2D_RECT_F { left: label.x, top: label.y, right: label.x + label.w, bottom: label.y + label.h };
            self.ctx.DrawText(
                &"NOTA".encode_utf16().collect::<Vec<_>>(), &gfx.note_label_format, &label_rect,
                &brush(th.text_secondary, 1.0)?, D2D1_DRAW_TEXT_OPTIONS_CLIP, DWRITE_MEASURING_MODE_NATURAL,
            );

            // Botão de menu "...": fundo no hover, anel de foco por teclado, três pontos.
            let mb = note.menu;
            let mrect = rrect(mb.x, mb.y, mb.x + mb.w, mb.y + mb.h, 8.0);
            if p.menu_hover > 0.001 {
                self.ctx.FillRoundedRectangle(&mrect, &brush(th.text, 0.14 * p.menu_hover)?);
            }
            let dots = brush(th.text, 0.85)?;
            for i in 0..3 {
                self.ctx.FillEllipse(
                    &D2D1_ELLIPSE {
                        point: Vector2 { X: mb.x + mb.w / 2.0 + (i as f32 - 1.0) * 4.0, Y: mb.y + mb.h / 2.0 },
                        radiusX: 1.2,
                        radiusY: 1.2,
                    },
                    &dots,
                );
            }
            if p.menu_focus {
                let ring = rrect(mb.x - 2.0, mb.y - 2.0, mb.x + mb.w + 2.0, mb.y + mb.h + 2.0, 10.0);
                self.ctx.DrawRoundedRectangle(&ring, &brush(p.accent, 1.0)?, 2.0, None);
            }

            // Elementos da view.
            let on_accent = brush(th.on_accent, 1.0)?;
            for pl in p.placed {
                let rc = D2D_RECT_F {
                    left: pl.rect.x,
                    top: pl.rect.y,
                    right: pl.rect.x + pl.rect.w,
                    bottom: pl.rect.y + pl.rect.h,
                };
                match &pl.kind {
                    PlacedKind::Label(text) => {
                        let wide: Vec<u16> = text.encode_utf16().collect();
                        self.ctx.DrawText(&wide, &gfx.label_format, &rc, &ink, D2D1_DRAW_TEXT_OPTIONS_CLIP, DWRITE_MEASURING_MODE_NATURAL);
                    }
                    PlacedKind::Button { id, label } => {
                        let hover = p.button_hover.filter(|(b, _)| b == id).map_or(0.0, |(_, t)| t);
                        let fill = theme::blend(th.on_accent, p.accent, 0.14 * hover);
                        self.ctx.FillRoundedRectangle(&rrect(rc.left, rc.top, rc.right, rc.bottom, 8.0), &brush(fill, 1.0)?);
                        let wide: Vec<u16> = label.encode_utf16().collect();
                        self.ctx.DrawText(&wide, &gfx.button_format, &rc, &on_accent, D2D1_DRAW_TEXT_OPTIONS_CLIP, DWRITE_MEASURING_MODE_NATURAL);
                    }
                    PlacedKind::TextField { .. } => {}
                }
            }
            // Placeholder desenhado, nunca inserido no buffer persistente da nota.
            // Durante IME o modelo ainda pode estar vazio, enquanto o buffer nativo já contém
            // a composição transitória. Não sobrepor o placeholder a esse texto.
            if p.empty && p.editor.is_none_or(|ed| ed.text().is_empty()) {
                if let Some(field) = p.placed.iter().find(|pl| matches!(pl.kind, PlacedKind::TextField { .. })) {
                    let rc = field.rect;
                    let index = if note.font_dip >= 28.0 { 2 } else if note.font_dip >= 22.0 { 1 } else { 0 };
                    self.ctx.DrawText(
                        &"Anote alguma coisa.".encode_utf16().collect::<Vec<_>>(), &gfx.note_body_formats[index],
                        &D2D_RECT_F { left: rc.x, top: rc.y, right: rc.x + rc.w, bottom: rc.y + rc.h },
                        &brush(th.text_secondary, 1.0)?, D2D1_DRAW_TEXT_OPTIONS_CLIP, DWRITE_MEASURING_MODE_NATURAL,
                    );
                }
            }
            if !p.status.is_empty() {
                let wide: Vec<u16> = p.status.encode_utf16().collect();
                let st = note.status;
                let rc = D2D_RECT_F { left: st.x, top: st.y, right: st.x + st.w, bottom: st.y + st.h };
                self.ctx.DrawText(&wide, &gfx.status_format, &rc, &brush(th.text_secondary, 1.0)?, D2D1_DRAW_TEXT_OPTIONS_CLIP, DWRITE_MEASURING_MODE_NATURAL);
            }

            // Contorno fino; quando a nota está em edição, migra para o acento (transição curta).
            let bw = th.border_width_dip.max(1.0);
            let border = theme::blend(p.accent, theme::blend(th.border, th.surface_top, th.border_alpha), p.focus_ring);
            let inset = bw / 2.0;
            let stroke_w = bw + 0.5 * p.focus_ring;
            self.ctx.DrawRoundedRectangle(&rrect(inset, inset, w - inset, h - inset, r - inset), &brush(border, 1.0)?, stroke_w, None);
            // Realce interno do topo (a "luz" do vidro): uma linha clara logo abaixo do contorno.
            if th.highlight_alpha > 0.0 {
                let hl = brush([255, 255, 255], th.highlight_alpha * 0.55)?;
                self.ctx.FillRectangle(&D2D_RECT_F { left: r, top: bw, right: w - r, bottom: bw + 1.0 }, &hl);
            }

            if let Some(ed) = p.editor {
                // O serviço de texto trabalha em pixels físicos: contexto a 96 DPI aqui.
                self.ctx.SetDpi(96.0, 96.0);
                self.ctx.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
                let c = ed.client();
                let to_f = |r: windows::Win32::Foundation::RECT| D2D_RECT_F {
                    left: r.left as f32,
                    top: r.top as f32,
                    right: r.right as f32,
                    bottom: r.bottom as f32,
                };
                self.ctx.PushAxisAlignedClip(&to_f(c), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                let _ = ed.draw(&self.ctx);
                if let Some(cr) = ed.caret_rect() {
                    self.ctx.FillRectangle(&to_f(cr), &ink);
                }
                self.ctx.PopAxisAlignedClip();
            }
            let work = started.elapsed();
            self.ctx.EndDraw(None, None)?;
            self.swap.Present(1, DXGI_PRESENT(0)).ok()?;
            crate::metrics::record_frame(work, started.elapsed());
        }
        Ok(())
    }
}

/// Desenho de um cartão de grade.
pub struct TileDraw<'a> {
    pub dpi: u32,
    pub theme: &'a Theme,
    pub accent: Rgb,
    pub view: &'a pod_model::view::View,
    pub laid: &'a [pod_model::layout::Laid],
    /// Modo de edição: desenha o selo "−" de remover.
    pub edit: bool,
    /// O cartão tem mais de um formato: mostra a alça de redimensionar no modo de edição.
    pub resizable: bool,
}

impl Surface {
    /// Desenho livre em DIPs para janelas que não são cartões (a galeria "Adicionar widget").
    pub fn draw_custom(&self, dpi: u32, f: impl FnOnce(&ID2D1DeviceContext) -> Result<()>) -> Result<()> {
        if FORCE_LOSS_ROUNDS.with(|c| c.get()) > 0 {
            return Err(windows::core::Error::from(HRESULT(DXGI_ERROR_DEVICE_REMOVED)));
        }
        unsafe {
            self.ctx.SetDpi(dpi as f32, dpi as f32);
            self.ctx.BeginDraw();
            self.ctx.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
            let drawn = f(&self.ctx);
            self.ctx.EndDraw(None, None)?;
            drawn?;
            self.swap.Present(1, DXGI_PRESENT(0)).ok()?;
        }
        Ok(())
    }

    pub fn draw_tile(&self, gfx: &Gfx, p: &TileDraw) -> Result<()> {
        if FORCE_LOSS_ROUNDS.with(|c| c.get()) > 0 {
            return Err(windows::core::Error::from(HRESULT(DXGI_ERROR_DEVICE_REMOVED)));
        }
        let started = std::time::Instant::now();
        let s = p.dpi as f32 / 96.0;
        let size = (self.width as f32 / s, self.height as f32 / s);
        let st = crate::render::PaintStyle { theme: p.theme, accent: p.accent };
        unsafe {
            self.ctx.SetDpi(p.dpi as f32, p.dpi as f32);
            self.ctx.BeginDraw();
            self.ctx.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
            let drawn = crate::render::paint_tile_background(&self.ctx, size, &st)
                .and_then(|_| crate::render::paint_view(&self.ctx, gfx, p.view, p.laid, &st))
                .and_then(|_| {
                    // Sem selo de excluir nem alça: excluir fica no menu do botão direito e o tamanho
                    // muda puxando as bordas do cartão.
                    let _ = (p.edit, p.resizable);
                    Ok(())
                });
            let work = started.elapsed();
            self.ctx.EndDraw(None, None)?;
            drawn?;
            self.swap.Present(1, DXGI_PRESENT(0)).ok()?;
            crate::metrics::record_frame(work, started.elapsed());
        }
        Ok(())
    }
}

/// Tudo o que um desenho precisa saber além da superfície. Valores de animação já avaliados.
pub struct DrawParams<'a> {
    pub dpi: u32,
    pub theme: &'a Theme,
    pub accent: Rgb,
    pub placed: &'a [Placed],
    pub editor: Option<&'a TextEditor>,
    pub status: &'a str,
    pub empty: bool,
    /// 0..1: realce do botão de menu sob o mouse.
    pub menu_hover: f32,
    /// 0..1: o editor tem o foco (contorno migra para o acento).
    pub focus_ring: f32,
    /// Anel de foco do teclado no botão de menu.
    pub menu_focus: bool,
    pub button_hover: Option<(LocalId, f32)>,
}

fn col(c: Rgb, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: c[0] as f32 / 255.0, g: c[1] as f32 / 255.0, b: c[2] as f32 / 255.0, a }
}
