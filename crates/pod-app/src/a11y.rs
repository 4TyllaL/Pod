//! UI Automation: expõe o editor windowless como um elemento `Edit` com `ValuePattern`.
//!
//! O Rich Edit windowless não tem HWND filho e portanto nenhuma acessibilidade automática
//! (ARCHITECTURE §7). Este provedor é o primeiro passo: nome, foco, retângulo e valor.
//! Seleção/caret (`TextPattern`) ainda não existem.
//!
//! Threading: `ProviderOptions_UseComThreading` faz o UIA chamar os provedores no apartamento
//! (STA) da thread de UI, onde vive o editor. Nenhum estado é tocado de outra thread.

use std::ffi::c_void;

use windows::core::{implement, ComObject, Error, IUnknown, IUnknownImpl, Interface, Result, BOOL, BSTR, HRESULT, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::Com::SAFEARRAY;
use windows::Win32::System::Ole::{SafeArrayCreateVector, SafeArrayPutElement};
use windows::Win32::System::Variant::{VARIANT, VT_I4};
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetFocus, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::host::{editor_of, is_tile, menu_button_focused, menu_button_screen_rect, tile_a11y_items, WM_OPEN_NOTE_MENU, WM_TILE_INVOKE};
use pod_model::a11y::A11yRole;

const PATTERN_INVOKE: i32 = 10000;
const PATTERN_VALUE: i32 = 10002;
const PROP_PROCESS_ID: i32 = 30002;
const PROP_CONTROL_TYPE: i32 = 30003;
const PROP_NAME: i32 = 30005;
const PROP_HAS_FOCUS: i32 = 30008;
const PROP_FOCUSABLE: i32 = 30009;
const PROP_ENABLED: i32 = 30010;
const PROP_AUTOMATION_ID: i32 = 30011;
const PROP_IS_CONTROL: i32 = 30016;
const PROP_IS_CONTENT: i32 = 30017;
const CONTROL_TYPE_BUTTON: i32 = 50000;
const CONTROL_TYPE_EDIT: i32 = 50004;
const CONTROL_TYPE_IMAGE: i32 = 50006;
const CONTROL_TYPE_CHECKBOX: i32 = 50002;
const CONTROL_TYPE_PROGRESS: i32 = 50012;
const CONTROL_TYPE_TEXT: i32 = 50020;
/// Ids de filhos de cartão começam longe dos da nota (1 = editor, 2 = menu).
const TILE_CHILD_BASE: i32 = 100;
const OPTIONS: ProviderOptions = ProviderOptions(
    ProviderOptions_ServerSideProvider.0 | ProviderOptions_UseComThreading.0,
);

pub fn root_provider(hwnd: HWND) -> IRawElementProviderSimple {
    ComObject::new(RootProvider { hwnd }).to_interface()
}

fn edit_fragment(hwnd: HWND) -> IRawElementProviderFragment {
    ComObject::new(EditProvider { hwnd }).to_interface()
}

fn tile_fragment(hwnd: HWND, index: usize) -> IRawElementProviderFragment {
    ComObject::new(TileItemProvider { hwnd, index }).to_interface()
}

fn menu_fragment(hwnd: HWND) -> IRawElementProviderFragment {
    ComObject::new(MenuButtonProvider { hwnd }).to_interface()
}

/// Avisar o foco do filho windowless, pois o evento do HWND identifica apenas a raiz.
/// Chamado na thread de UI e sem empréstimos do estado atravessando os callbacks do UIA.
pub fn notify_note_focus(hwnd: HWND) {
    unsafe {
        if GetFocus() != hwnd || is_tile(hwnd) || !UiaClientsAreListening().as_bool() {
            return;
        }
        let fragment = if menu_button_focused(hwnd) || editor_of(hwnd).is_none() {
            menu_fragment(hwnd)
        } else {
            edit_fragment(hwnd)
        };
        if let Ok(provider) = fragment.cast::<IRawElementProviderSimple>() {
            let _ = UiaRaiseAutomationEvent(&provider, UIA_AutomationFocusChangedEventId);
        }
    }
}

/// Raiz hospedada em HWND (`None`): o UIA completa o ID. Os filhos usam [AppendRuntimeId, n].
fn runtime_id(child: Option<i32>) -> Result<*mut SAFEARRAY> {
    let Some(child) = child else {
        return Ok(std::ptr::null_mut());
    };
    unsafe {
        let sa = SafeArrayCreateVector(VT_I4, 0, 2);
        if sa.is_null() {
            return Err(Error::from(E_OUTOFMEMORY));
        }
        for (i, v) in [UiaAppendRuntimeId as i32, child].iter().enumerate() {
            SafeArrayPutElement(sa, &(i as i32), v as *const i32 as *const c_void)?;
        }
        Ok(sa)
    }
}

/// Raiz do fragmento: a própria janela (propriedades vêm do provedor padrão do HWND).
#[implement(IRawElementProviderSimple, IRawElementProviderFragment, IRawElementProviderFragmentRoot)]
struct RootProvider {
    hwnd: HWND,
}

impl IRawElementProviderSimple_Impl for RootProvider_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(OPTIONS)
    }
    fn GetPatternProvider(&self, _: UIA_PATTERN_ID) -> Result<IUnknown> {
        Err(Error::empty())
    }
    fn GetPropertyValue(&self, _: UIA_PROPERTY_ID) -> Result<VARIANT> {
        Ok(VARIANT::default())
    }
    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        unsafe { UiaHostProviderFromHwnd(self.hwnd) }
    }
}

impl IRawElementProviderFragment_Impl for RootProvider_Impl {
    fn Navigate(&self, direction: NavigateDirection) -> Result<IRawElementProviderFragment> {
        if is_tile(self.hwnd) {
            let n = tile_a11y_items(self.hwnd).map_or(0, |v| v.len());
            return match direction {
                NavigateDirection_FirstChild if n > 0 => Ok(tile_fragment(self.hwnd, 0)),
                NavigateDirection_LastChild if n > 0 => Ok(tile_fragment(self.hwnd, n - 1)),
                _ => Err(Error::empty()),
            };
        }
        // Ordem dos filhos: o editor (quando existe) e depois o botão de menu.
        match direction {
            NavigateDirection_FirstChild if editor_of(self.hwnd).is_some() => Ok(edit_fragment(self.hwnd)),
            NavigateDirection_FirstChild | NavigateDirection_LastChild => Ok(menu_fragment(self.hwnd)),
            _ => Err(Error::empty()),
        }
    }
    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        runtime_id(None)
    }
    fn BoundingRectangle(&self) -> Result<UiaRect> {
        Ok(UiaRect::default())
    }
    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }
    fn SetFocus(&self) -> Result<()> {
        unsafe { SetFocus(Some(self.hwnd)) }.map(|_| ())
    }
    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        Ok(ComObject::new(RootProvider { hwnd: self.hwnd }).to_interface())
    }
}

impl IRawElementProviderFragmentRoot_Impl for RootProvider_Impl {
    fn ElementProviderFromPoint(&self, x: f64, y: f64) -> Result<IRawElementProviderFragment> {
        if is_tile(self.hwnd) {
            let items = tile_a11y_items(self.hwnd).unwrap_or_default();
            // O último item que contém o ponto é o mais interno.
            return match items.iter().rposition(|(_, r)| {
                x >= r.left as f64 && x < r.right as f64 && y >= r.top as f64 && y < r.bottom as f64
            }) {
                Some(i) => Ok(tile_fragment(self.hwnd, i)),
                None => Err(Error::empty()),
            };
        }
        let m = menu_button_screen_rect(self.hwnd);
        let on_menu = x >= m.left as f64 && x < m.right as f64 && y >= m.top as f64 && y < m.bottom as f64;
        if on_menu || editor_of(self.hwnd).is_none() {
            return Ok(menu_fragment(self.hwnd));
        }
        Ok(edit_fragment(self.hwnd))
    }
    fn GetFocus(&self) -> Result<IRawElementProviderFragment> {
        if unsafe { GetFocus() } != self.hwnd || is_tile(self.hwnd) {
            return Err(Error::empty()); // nada focável dentro do cartão
        }
        if editor_of(self.hwnd).is_none() || menu_button_focused(self.hwnd) {
            return Ok(menu_fragment(self.hwnd));
        }
        Ok(edit_fragment(self.hwnd))
    }
}

/// O campo de texto da nota.
#[implement(IRawElementProviderSimple, IRawElementProviderFragment, IValueProvider)]
struct EditProvider {
    hwnd: HWND,
}

impl IRawElementProviderSimple_Impl for EditProvider_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(OPTIONS)
    }
    fn GetPatternProvider(&self, id: UIA_PATTERN_ID) -> Result<IUnknown> {
        if id.0 == PATTERN_VALUE {
            let me: ComObject<EditProvider> = self.to_object();
            me.to_interface::<IValueProvider>().cast()
        } else {
            Err(Error::empty())
        }
    }
    fn GetPropertyValue(&self, id: UIA_PROPERTY_ID) -> Result<VARIANT> {
        Ok(match id.0 {
            PROP_CONTROL_TYPE => VARIANT::from(CONTROL_TYPE_EDIT),
            PROP_NAME => VARIANT::from(BSTR::from("Nota")),
            PROP_AUTOMATION_ID => VARIANT::from(BSTR::from("pod.quick-notes.editor")),
            PROP_PROCESS_ID => VARIANT::from(std::process::id() as i32),
            PROP_HAS_FOCUS => VARIANT::from(unsafe { GetFocus() } == self.hwnd && !menu_button_focused(self.hwnd)),
            PROP_FOCUSABLE | PROP_ENABLED | PROP_IS_CONTROL | PROP_IS_CONTENT => VARIANT::from(true),
            _ => VARIANT::default(),
        })
    }
    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        Err(Error::empty())
    }
}

impl IRawElementProviderFragment_Impl for EditProvider_Impl {
    fn Navigate(&self, direction: NavigateDirection) -> Result<IRawElementProviderFragment> {
        match direction {
            NavigateDirection_Parent => Ok(ComObject::new(RootProvider { hwnd: self.hwnd }).to_interface()),
            NavigateDirection_NextSibling => Ok(menu_fragment(self.hwnd)),
            _ => Err(Error::empty()),
        }
    }
    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        runtime_id(Some(1))
    }
    fn BoundingRectangle(&self) -> Result<UiaRect> {
        let ed = editor_of(self.hwnd).ok_or_else(|| Error::from(HRESULT(UIA_E_ELEMENTNOTAVAILABLE as i32)))?;
        let c = ed.client();
        let mut origin = POINT { x: c.left, y: c.top };
        unsafe {
            let _ = ClientToScreen(self.hwnd, &mut origin);
        }
        Ok(UiaRect {
            left: origin.x as f64,
            top: origin.y as f64,
            width: (c.right - c.left) as f64,
            height: (c.bottom - c.top) as f64,
        })
    }
    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }
    fn SetFocus(&self) -> Result<()> {
        crate::host::focus_note_control(self.hwnd, false)
    }
    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        Ok(ComObject::new(RootProvider { hwnd: self.hwnd }).to_interface())
    }
}

impl IValueProvider_Impl for EditProvider_Impl {
    fn SetValue(&self, val: &PCWSTR) -> Result<()> {
        let ed = editor_of(self.hwnd).ok_or_else(|| Error::from(HRESULT(UIA_E_ELEMENTNOTAVAILABLE as i32)))?;
        ed.replace_text(&unsafe { val.to_string() }?)?;
        // Vem de fora do editor: o modelo precisa saber (e persistir) como numa digitação.
        crate::host::deliver_text(self.hwnd);
        Ok(())
    }
    fn Value(&self) -> Result<BSTR> {
        let ed = editor_of(self.hwnd).ok_or_else(|| Error::from(HRESULT(UIA_E_ELEMENTNOTAVAILABLE as i32)))?;
        Ok(BSTR::from(ed.text_lf().replace('\n', "\r\n")))
    }
    fn IsReadOnly(&self) -> Result<BOOL> {
        Ok(BOOL(0))
    }
}

/// Botão "Menu da nota" na faixa de movimentação: acessível por UI Automation e por teclado.
#[implement(IRawElementProviderSimple, IRawElementProviderFragment, IInvokeProvider)]
struct MenuButtonProvider {
    hwnd: HWND,
}

impl IRawElementProviderSimple_Impl for MenuButtonProvider_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(OPTIONS)
    }
    fn GetPatternProvider(&self, id: UIA_PATTERN_ID) -> Result<IUnknown> {
        if id.0 == PATTERN_INVOKE {
            let me: ComObject<MenuButtonProvider> = self.to_object();
            me.to_interface::<IInvokeProvider>().cast()
        } else {
            Err(Error::empty())
        }
    }
    fn GetPropertyValue(&self, id: UIA_PROPERTY_ID) -> Result<VARIANT> {
        Ok(match id.0 {
            PROP_CONTROL_TYPE => VARIANT::from(CONTROL_TYPE_BUTTON),
            PROP_NAME => VARIANT::from(BSTR::from("Menu da nota")),
            PROP_AUTOMATION_ID => VARIANT::from(BSTR::from("pod.note.menu")),
            PROP_PROCESS_ID => VARIANT::from(std::process::id() as i32),
            PROP_HAS_FOCUS => VARIANT::from(menu_button_focused(self.hwnd)),
            PROP_FOCUSABLE | PROP_ENABLED | PROP_IS_CONTROL | PROP_IS_CONTENT => VARIANT::from(true),
            _ => VARIANT::default(),
        })
    }
    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        Err(Error::empty())
    }
}

impl IRawElementProviderFragment_Impl for MenuButtonProvider_Impl {
    fn Navigate(&self, direction: NavigateDirection) -> Result<IRawElementProviderFragment> {
        match direction {
            NavigateDirection_Parent => Ok(ComObject::new(RootProvider { hwnd: self.hwnd }).to_interface()),
            NavigateDirection_PreviousSibling if editor_of(self.hwnd).is_some() => Ok(edit_fragment(self.hwnd)),
            _ => Err(Error::empty()),
        }
    }
    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        runtime_id(Some(2))
    }
    fn BoundingRectangle(&self) -> Result<UiaRect> {
        let r = menu_button_screen_rect(self.hwnd);
        Ok(UiaRect {
            left: r.left as f64,
            top: r.top as f64,
            width: (r.right - r.left) as f64,
            height: (r.bottom - r.top) as f64,
        })
    }
    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }
    fn SetFocus(&self) -> Result<()> {
        crate::host::focus_note_control(self.hwnd, true)
    }
    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        Ok(ComObject::new(RootProvider { hwnd: self.hwnd }).to_interface())
    }
}

impl IInvokeProvider_Impl for MenuButtonProvider_Impl {
    fn Invoke(&self) -> Result<()> {
        // O menu é modal: abre depois de a chamada do provedor retornar.
        unsafe { PostMessageW(Some(self.hwnd), WM_OPEN_NOTE_MENU, WPARAM(0), LPARAM(0)) }
    }
}

/// Um item da view de um cartão (texto, progresso, imagem, botão), gerado em `pod_model::a11y`.
#[implement(IRawElementProviderSimple, IRawElementProviderFragment, IInvokeProvider)]
struct TileItemProvider {
    hwnd: HWND,
    index: usize,
}

impl TileItemProvider_Impl {
    fn item(&self) -> Result<(pod_model::a11y::A11yItem, RECT)> {
        tile_a11y_items(self.hwnd)
            .and_then(|mut v| (self.index < v.len()).then(|| v.swap_remove(self.index)))
            .ok_or_else(|| Error::from(HRESULT(UIA_E_ELEMENTNOTAVAILABLE as i32)))
    }
}

impl IRawElementProviderSimple_Impl for TileItemProvider_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(OPTIONS)
    }
    fn GetPatternProvider(&self, id: UIA_PATTERN_ID) -> Result<IUnknown> {
        if id.0 == PATTERN_INVOKE && self.item().is_ok_and(|(i, _)| i.button.is_some()) {
            let me: ComObject<TileItemProvider> = self.to_object();
            me.to_interface::<IInvokeProvider>().cast()
        } else {
            Err(Error::empty())
        }
    }
    fn GetPropertyValue(&self, id: UIA_PROPERTY_ID) -> Result<VARIANT> {
        let (item, _) = self.item()?;
        Ok(match id.0 {
            PROP_CONTROL_TYPE => VARIANT::from(match item.role {
                A11yRole::Text => CONTROL_TYPE_TEXT,
                A11yRole::Progress => CONTROL_TYPE_PROGRESS,
                A11yRole::Image => CONTROL_TYPE_IMAGE,
                A11yRole::Button => CONTROL_TYPE_BUTTON,
                A11yRole::CheckBox => CONTROL_TYPE_CHECKBOX,
            }),
            PROP_NAME => VARIANT::from(BSTR::from(item.name.as_str())),
            PROP_AUTOMATION_ID => VARIANT::from(BSTR::from(format!("pod.tile.{}", self.index).as_str())),
            PROP_PROCESS_ID => VARIANT::from(std::process::id() as i32),
            PROP_HAS_FOCUS => VARIANT::from(false),
            PROP_FOCUSABLE => VARIANT::from(item.button.is_some()),
            PROP_ENABLED | PROP_IS_CONTROL | PROP_IS_CONTENT => VARIANT::from(true),
            _ => VARIANT::default(),
        })
    }
    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        Err(Error::empty())
    }
}

impl IRawElementProviderFragment_Impl for TileItemProvider_Impl {
    fn Navigate(&self, direction: NavigateDirection) -> Result<IRawElementProviderFragment> {
        let n = tile_a11y_items(self.hwnd).map_or(0, |v| v.len());
        match direction {
            NavigateDirection_Parent => Ok(ComObject::new(RootProvider { hwnd: self.hwnd }).to_interface()),
            NavigateDirection_NextSibling if self.index + 1 < n => Ok(tile_fragment(self.hwnd, self.index + 1)),
            NavigateDirection_PreviousSibling if self.index > 0 && self.index <= n => {
                Ok(tile_fragment(self.hwnd, self.index - 1))
            }
            _ => Err(Error::empty()),
        }
    }
    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        runtime_id(Some(TILE_CHILD_BASE + self.index as i32))
    }
    fn BoundingRectangle(&self) -> Result<UiaRect> {
        let (_, r) = self.item()?;
        Ok(UiaRect {
            left: r.left as f64,
            top: r.top as f64,
            width: (r.right - r.left) as f64,
            height: (r.bottom - r.top) as f64,
        })
    }
    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }
    fn SetFocus(&self) -> Result<()> {
        Err(Error::empty())
    }
    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        Ok(ComObject::new(RootProvider { hwnd: self.hwnd }).to_interface())
    }
}

impl IInvokeProvider_Impl for TileItemProvider_Impl {
    fn Invoke(&self) -> Result<()> {
        let (item, _) = self.item()?;
        let button = item.button.ok_or_else(Error::empty)?;
        // Postado: o widget reage depois de a chamada do provedor retornar.
        unsafe { PostMessageW(Some(self.hwnd), WM_TILE_INVOKE, WPARAM(button.0 as usize), LPARAM(0)) }
    }
}
