//! Formato durável do workspace (JSON versionado) e sua validação. Sem filesystem.
//!
//! Campos desconhecidos são preservados (`extra`), de modo que um tipo de widget que esta
//! versão não conhece volta intacto ao disco quando as outras instâncias são salvas.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ids::WidgetId;
use crate::placement::PreferredPlacement;

pub const SCHEMA_VERSION: u32 = 3;
/// Teto do arquivo de snapshot (ARCHITECTURE §8).
pub const MAX_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_INSTANCES: usize = 4096;
/// Teto do estado opaco de uma instância (o limite de texto de cada tipo é menor).
pub const MAX_STATE_BYTES: usize = 1024 * 1024;

type Extra = BTreeMap<String, Value>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub schema_version: u32,
    /// Monotônica: cada snapshot gravado tem geração maior que o anterior.
    pub generation: u64,
    #[serde(default)]
    pub preferences: Preferences,
    #[serde(default)]
    pub instances: Vec<InstanceRecord>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceRecord {
    /// `WidgetId` em hexadecimal (32 dígitos).
    pub id: String,
    pub type_id: String,
    pub state_version: u32,
    /// Estado opaco do tipo (o formato pertence ao widget).
    pub state: String,
    pub visible: bool,
    /// Posição preferida (durável), relativa à área útil de um monitor. O efetivo é derivado.
    pub placement: PreferredPlacement,
    /// Formato em grade ("2x1"); ausente em widgets de tamanho livre (notas) e em schemas < 3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
    /// Ajustes por instância (formato do tipo); ausente quando o padrão basta.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<Value>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SchemaError {
    TooLarge(usize),
    /// JSON inválido ou campos com tipo errado.
    Parse(String),
    /// Gravado por uma versão mais nova: nunca reescrever nem fazer downgrade.
    NewerVersion { found: u32, supported: u32 },
    /// Versão anterior sem migração conhecida.
    UnsupportedOld(u32),
    Invalid(String),
}

impl std::fmt::Display for SchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge(n) => write!(f, "snapshot grande demais ({n} bytes)"),
            Self::Parse(e) => write!(f, "JSON inválido: {e}"),
            Self::NewerVersion { found, supported } => {
                write!(f, "workspace da versão {found}, este aplicativo suporta até a {supported}")
            }
            Self::UnsupportedOld(v) => write!(f, "versão {v} sem migração conhecida"),
            Self::Invalid(e) => write!(f, "workspace inválido: {e}"),
        }
    }
}

impl Workspace {
    pub fn empty() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            generation: 0,
            preferences: Preferences::default(),
            instances: Vec::new(),
            extra: Extra::new(),
        }
    }

    /// Lê, migra em memória (cópia) e valida.
    pub fn from_json(bytes: &[u8]) -> Result<Self, SchemaError> {
        Self::from_json_detailed(bytes).map(|(ws, _)| ws)
    }

    /// Como `from_json`, devolvendo também a versão que estava no arquivo. Se for menor que
    /// `SCHEMA_VERSION`, o chamador deve preservar o arquivo original antes de gravar o migrado.
    pub fn from_json_detailed(bytes: &[u8]) -> Result<(Self, u32), SchemaError> {
        if bytes.len() > MAX_SNAPSHOT_BYTES {
            return Err(SchemaError::TooLarge(bytes.len()));
        }
        // Lê só a versão primeiro: uma versão futura pode ter um formato que não conseguimos
        // interpretar, e isso não deve parecer corrupção.
        let raw: Value = serde_json::from_slice(bytes).map_err(|e| SchemaError::Parse(e.to_string()))?;
        let version = raw
            .get("schema_version")
            .and_then(Value::as_u64)
            .ok_or_else(|| SchemaError::Invalid("falta schema_version".into()))? as u32;
        if version > SCHEMA_VERSION {
            return Err(SchemaError::NewerVersion { found: version, supported: SCHEMA_VERSION });
        }
        let ws = match version {
            // O schema 3 só acrescenta campos opcionais: um arquivo 2 lê igual e é regravado como 3.
            2 | 3 => {
                let mut ws = serde_json::from_value::<Workspace>(raw).map_err(|e| SchemaError::Parse(e.to_string()))?;
                ws.schema_version = SCHEMA_VERSION;
                ws
            }
            1 => v1::migrate(raw)?,
            other => return Err(SchemaError::UnsupportedOld(other)),
        };
        ws.validate()?;
        Ok((ws, version))
    }

    pub fn to_json(&self) -> Result<Vec<u8>, SchemaError> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| SchemaError::Parse(e.to_string()))?;
        if bytes.len() > MAX_SNAPSHOT_BYTES {
            return Err(SchemaError::TooLarge(bytes.len()));
        }
        Ok(bytes)
    }

    pub fn validate(&self) -> Result<(), SchemaError> {
        let bad = |m: String| Err(SchemaError::Invalid(m));
        if self.instances.len() > MAX_INSTANCES {
            return bad(format!("instâncias demais ({})", self.instances.len()));
        }
        let mut seen = BTreeSet::new();
        for (i, r) in self.instances.iter().enumerate() {
            let Some(id) = WidgetId::from_hex(&r.id) else {
                return bad(format!("instância {i}: id inválido"));
            };
            if !seen.insert(id) {
                return bad(format!("instância {i}: id duplicado"));
            }
            if r.type_id.is_empty() || r.type_id.len() > 128 {
                return bad(format!("instância {i}: type_id inválido"));
            }
            if r.state.len() > MAX_STATE_BYTES {
                return bad(format!("instância {i}: estado grande demais"));
            }
            if !r.placement.is_valid() {
                return bad(format!("instância {i}: geometria inválida"));
            }
            if r.shape.as_deref().is_some_and(|k| crate::shape::Shape::parse(k).is_none()) {
                return bad(format!("instância {i}: formato inválido"));
            }
            if r.settings.as_ref().is_some_and(|v| v.to_string().len() > MAX_STATE_BYTES) {
                return bad(format!("instância {i}: ajustes grandes demais"));
            }
        }
        Ok(())
    }
}

/// Formato do schema 1: posição absoluta no desktop virtual. Só existe para migrar.
mod v1 {
    use super::*;

    #[derive(Deserialize)]
    struct Placement {
        x: i32,
        y: i32,
        width_dip: f32,
        height_dip: f32,
    }

    #[derive(Deserialize)]
    struct Record {
        id: String,
        type_id: String,
        state_version: u32,
        state: String,
        visible: bool,
        placement: Placement,
        #[serde(flatten)]
        extra: Extra,
    }

    #[derive(Deserialize)]
    struct Old {
        generation: u64,
        #[serde(default)]
        preferences: Preferences,
        #[serde(default)]
        instances: Vec<Record>,
        #[serde(flatten)]
        extra: Extra,
    }

    pub fn migrate(raw: Value) -> Result<Workspace, SchemaError> {
        let mut old: Old = serde_json::from_value(raw).map_err(|e| SchemaError::Parse(e.to_string()))?;
        old.extra.remove("schema_version");
        Ok(Workspace {
            schema_version: SCHEMA_VERSION,
            generation: old.generation,
            preferences: old.preferences,
            instances: old
                .instances
                .into_iter()
                .map(|r| InstanceRecord {
                    id: r.id,
                    type_id: r.type_id,
                    state_version: r.state_version,
                    state: r.state,
                    visible: r.visible,
                    placement: PreferredPlacement::from_legacy(
                        r.placement.x,
                        r.placement.y,
                        r.placement.width_dip,
                        r.placement.height_dip,
                    ),
                    shape: None,
                    settings: None,
                    extra: r.extra,
                })
                .collect(),
            extra: old.extra,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::placement::MonitorHint;

    fn pref() -> PreferredPlacement {
        PreferredPlacement {
            monitor: Some(MonitorHint {
                device_path: r"\?DISPLAY#TST0001#1".into(),
                edid_id: Some("TST:0001".into()),
                name: "Teste".into(),
                connection: None,
                known_left: -1920,
                known_top: 0,
                known_width: 1920,
                known_height: 1080,
            }),
            offset_x_dip: 40.0,
            offset_y_dip: 40.0,
            width_dip: 320.0,
            height_dip: 240.0,
            ratio_x: 0.1,
            ratio_y: 0.1,
            ref_work_w_dip: 1920.0,
            ref_work_h_dip: 1040.0,
            legacy_abs: None,
        }
    }

    fn record(id: u128) -> InstanceRecord {
        InstanceRecord {
            id: WidgetId(id).to_hex(),
            type_id: "pod.test-card".into(),
            state_version: 1,
            state: "3".into(),
            visible: true,
            placement: pref(),
            shape: None,
            settings: None,
            extra: Extra::new(),
        }
    }

    fn sample() -> Workspace {
        let mut ws = Workspace::empty();
        ws.generation = 7;
        ws.instances = vec![record(1), record(2)];
        ws
    }

    #[test]
    fn roundtrips_including_negative_origins() {
        let ws = sample();
        let back = Workspace::from_json(&ws.to_json().unwrap()).unwrap();
        assert_eq!(back, ws);
        assert_eq!(back.instances[0].placement.monitor.as_ref().unwrap().known_left, -1920);
    }

    #[test]
    fn unknown_fields_and_types_survive_a_roundtrip() {
        let json = r#"{"schema_version":1,"generation":2,"future_flag":true,"instances":[
            {"id":"00000000000000000000000000000009","type_id":"pod.from-the-future","state_version":5,
             "state":"{\"a\":1}","visible":false,"mystery":[1,2,3],
             "placement":{"x":1,"y":2,"width_dip":100.0,"height_dip":80.0}}]}"#;
        let ws = Workspace::from_json(json.as_bytes()).unwrap();
        assert_eq!(ws.instances[0].type_id, "pod.from-the-future");
        let again = Workspace::from_json(&ws.to_json().unwrap()).unwrap();
        assert_eq!(again.extra["future_flag"], Value::Bool(true));
        assert_eq!(again.instances[0].extra["mystery"], serde_json::json!([1, 2, 3]));
        assert_eq!(again.instances[0].state, "{\"a\":1}");
    }

    #[test]
    fn newer_version_is_not_corruption() {
        let json = br#"{"schema_version":99,"layout":"formato desconhecido"}"#;
        assert_eq!(
            Workspace::from_json(json),
            Err(SchemaError::NewerVersion { found: 99, supported: SCHEMA_VERSION })
        );
    }

    #[test]
    fn rejects_duplicate_ids_bad_hex_and_bad_geometry() {
        let mut ws = sample();
        ws.instances[1].id = ws.instances[0].id.clone();
        assert!(matches!(ws.validate(), Err(SchemaError::Invalid(m)) if m.contains("duplicado")));

        let mut ws = sample();
        ws.instances[0].id = "xyz".into();
        assert!(ws.validate().is_err());

        for bad in [f32::NAN, f32::INFINITY, 0.0, -5.0] {
            let mut ws = sample();
            ws.instances[0].placement.width_dip = bad;
            assert!(ws.validate().is_err(), "largura {bad} deveria ser recusada");
        }
        let mut ws = sample();
        ws.instances[0].placement.offset_x_dip = 1e12;
        assert!(ws.validate().is_err());
    }

    #[test]
    fn rejects_truncated_garbage_and_oversize_input() {
        let json = sample().to_json().unwrap();
        assert!(matches!(Workspace::from_json(&json[..json.len() / 2]), Err(SchemaError::Parse(_))));
        assert!(Workspace::from_json(b"").is_err());
        assert!(Workspace::from_json(b"[1,2,3]").is_err());
        assert!(Workspace::from_json(br#"{"generation":1}"#).is_err());
        let huge = vec![b' '; MAX_SNAPSHOT_BYTES + 1];
        assert!(matches!(Workspace::from_json(&huge), Err(SchemaError::TooLarge(_))));
    }

    #[test]
    fn old_versions_without_migration_are_reported_not_guessed() {
        let json = br#"{"schema_version":0,"generation":1}"#;
        assert_eq!(Workspace::from_json(json), Err(SchemaError::UnsupportedOld(0)));
    }

    #[test]
    fn schema1_is_migrated_in_a_copy_and_reports_the_original_version() {
        let v1 = r#"{"schema_version":1,"generation":5,"instances":[
            {"id":"0000000000000000000000000000000a","type_id":"pod.quick-notes","state_version":1,
             "state":"texto","visible":true,"placement":{"x":-1500,"y":120,"width_dip":320.0,"height_dip":240.0}}]}"#;
        let (ws, original) = Workspace::from_json_detailed(v1.as_bytes()).unwrap();
        assert_eq!(original, 1, "o chamador usa isso para preservar o arquivo antigo");
        assert_eq!(ws.schema_version, SCHEMA_VERSION);
        assert_eq!(ws.generation, 5);
        let p = &ws.instances[0].placement;
        assert!(p.monitor.is_none());
        assert_eq!(p.legacy_abs.map(|a| (a.x, a.y)), Some((-1500, 120)));
        assert_eq!(ws.instances[0].state, "texto", "conteúdo intocado");
        // Gravar produz schema 2, que relido já não é migração.
        let (_, again) = Workspace::from_json_detailed(&ws.to_json().unwrap()).unwrap();
        assert_eq!(again, SCHEMA_VERSION);
    }

    #[test]
    fn schema1_with_invalid_geometry_is_still_rejected_after_migration() {
        let v1 = r#"{"schema_version":1,"generation":1,"instances":[
            {"id":"0000000000000000000000000000000a","type_id":"x","state_version":1,"state":"",
             "visible":true,"placement":{"x":0,"y":0,"width_dip":-5.0,"height_dip":240.0}}]}"#;
        assert!(Workspace::from_json(v1.as_bytes()).is_err());
    }
}

#[cfg(test)]
mod schema3_tests {
    use super::*;

    const V2: &str = r#"{"schema_version":2,"generation":4,"instances":[
        {"id":"0000000000000000000000000000000a","type_id":"pod.quick-notes","state_version":1,"state":"x",
         "visible":true,"placement":{"monitor":null,"offset_x_dip":10.0,"offset_y_dip":10.0,"width_dip":320.0,
         "height_dip":240.0,"ratio_x":0.1,"ratio_y":0.1,"ref_work_w_dip":1920.0,"ref_work_h_dip":1080.0}}]}"#;

    #[test]
    fn schema2_reads_as_3_and_reports_the_original() {
        let (ws, original) = Workspace::from_json_detailed(V2.as_bytes()).unwrap();
        assert_eq!(original, 2, "o chamador preserva o arquivo 2");
        assert_eq!(ws.schema_version, 3);
        assert_eq!(ws.instances[0].shape, None);
        let json = String::from_utf8(ws.to_json().unwrap()).unwrap();
        assert!(!json.contains("\"shape\"") && !json.contains("\"settings\""), "opcionais ausentes não são gravados");
    }

    #[test]
    fn shape_and_settings_roundtrip_and_bad_shapes_are_rejected() {
        let (mut ws, _) = Workspace::from_json_detailed(V2.as_bytes()).unwrap();
        ws.instances[0].shape = Some("2x1".into());
        ws.instances[0].settings = Some(serde_json::json!({"format": "24h"}));
        let again = Workspace::from_json(&ws.to_json().unwrap()).unwrap();
        assert_eq!(again.instances[0].shape.as_deref(), Some("2x1"));
        assert_eq!(again.instances[0].settings, ws.instances[0].settings);

        ws.instances[0].shape = Some("7x7".into());
        assert!(ws.validate().is_err());
    }
}
