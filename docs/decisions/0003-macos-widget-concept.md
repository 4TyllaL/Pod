# ADR-0003 — Conceito: widgets estilo macOS no desktop Windows

**Status:** **proposta** (nada implementado). **Data:** 18 de outubro de 2026.
**Origem:** o dono esclareceu que a ideia do Pod é ser **os widgets do macOS no Windows**, e que o Dockn't é
referência de conceito, não de design. Pesquisa: Rainmeter, Zebar, Seelen UI, Widgets do Windows 11, KDE
Plasma e WidgetKit (fontes em `docs/validation/` quando forem citadas).

## 1. O conceito

Widgets **glanceable**: pequenos cartões no desktop que mostram a informação certa num relance, sem rolagem,
sem virar mini-apps. Famílias de tamanho fixas, cor de identidade por widget, translucidez discreta, edição
simples (adicionar, remover, arrastar) e custo quase zero em repouso. Identidade visual **própria** do Pod,
sem copiar a aparência do macOS nem a do Dockn't.

## 2. O que o Pod já cumpre

Famílias `1×1`, `2×1`, `2×2`; view declarativa com tema e acessibilidade gerados; serviços de dados separados
da view (`Metrics`, `Local`); ajustes por menu; cor de identidade por tipo; vidro simulado opt-in; 0 quadros
em repouso e portão de 40 janelas medido.

## 3. Propostas (cada uma vira decisão própria antes de código)

| # | Proposta | Inspiração | Efeito no contrato |
| --- | --- | --- | --- |
| P1 | **Política de atualização declarada** no `Descriptor`: `Never`, `Minute`, `Second`, `Timeline` (o widget publica entradas futuras e o host só acorda nelas). Substitui efeitos ad hoc de tempo. | WidgetKit (timeline + reload policy) | `Descriptor.refresh`; `Effect::Timeline(Vec<Entry>)` |
| P2 (parcial: `Desktop` feito, ver `validation/desktop-layer.md`) | **Tipo de camada** por widget: `Desktop` (atrás das janelas, padrão), `Overlay` (sempre no topo), `Popup` (some ao perder foco). | Seelen UI (window presets) | `Descriptor.layer`; host aplica z-order |
| P3 | **Instância por monitor** e ajustes com **dependências** e valores por monitor. | Seelen UI (settings) | `SettingSpec` com `depends_on`, `per_monitor` |
| P4 | **Contêiner de grade** + modo de edição "+/−" e galeria "Adicionar widget". | KDE Plasma (containments), macOS | W6; sem mudança de contrato |
| P5 | **Esquema de view serializável** (JSON versionado), base de um futuro SDK sem webview. | Adaptive Cards (Widgets do Windows) | `View` ↔ JSON; testes de ida e volta |

## 4. Regras que continuam valendo

Sem webview (ADR-0001, §10). Sem estado global. Cada widget é um módulo isolado. Rede só por opt-in.
Nada de copiar arte, nomes ou assets da Apple ou do Dockn't.

## 5. Questões abertas (o dono decide)

1. Ordem de P1 a P5? Recomendado: P4 (o que o usuário vê) → P1 → P2 → P3 → P5.
2. O padrão de camada é `Desktop` (atrás das janelas) como no macOS, ou o atual "discreto"?
3. Aceita uma sombra larga (exige janelas com margem, ver `glass-simulated.md`) ou fica a sombra curta?
