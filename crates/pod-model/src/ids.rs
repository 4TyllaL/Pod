//! Identidades estáveis. Nunca derivadas de HWND ou de índice de lista.

/// Identifica uma implementação de widget, por exemplo `pod.quick-notes`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WidgetTypeId(pub &'static str);

/// Identifica uma instância de widget (128 bits aleatórios).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WidgetId(pub u128);

impl WidgetId {
    pub fn to_hex(self) -> String {
        format!("{:032x}", self.0)
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        if s.len() != 32 {
            return None;
        }
        u128::from_str_radix(s, 16).ok().map(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let id = WidgetId(0x1234_5678_9abc_def0_0fed_cba9_8765_4321);
        assert_eq!(WidgetId::from_hex(&id.to_hex()), Some(id));
    }

    #[test]
    fn hex_rejects_bad_input() {
        assert_eq!(WidgetId::from_hex("zz"), None);
        assert_eq!(WidgetId::from_hex(&"g".repeat(32)), None);
        assert_eq!(WidgetId::from_hex("+".repeat(1).as_str()), None);
    }
}
