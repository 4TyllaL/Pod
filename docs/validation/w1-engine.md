# W1 — motor de widgets: cartões de grade, Clock e Time progress

Ambiente: Windows 11 Pro 26200, 12 núcleos lógicos, 3 monitores a 100% de DPI, GPU real (sem WARP na
medição de janelas). Build release com asserts de debug, dados em `--data-dir`.

## Portões do ADR-0001

| Portão | Resultado |
|---|---|
| Layout testado sem Win32 em todos os formatos | Atendido: `children_stay_inside_parents_in_every_shape` (7 formatos), mais testes por widget (Clock e Time progress em todos os formatos aceitos) |
| Clock em 3 formatos com UIA legível | Atendido. UIA lido nos cartões abertos: `Relógio analógico, 15:51`, `domingo, 18 de outubro`, `15:51:30`, `79%` (progresso), `Semana`, `dia 1 de 7`; a raiz se chama "Relógio" / "Progresso do tempo" |
| 0 quadros entre mudanças | Atendido: 0 quadros em 45 s entre viradas de minuto, com 40 cartões |
| 40 janelas cumprem os orçamentos do M6 | Atendido (abaixo) |
| Regressão visual estável nos 3 temas | Atendido nesta máquina: 54 imagens (6 variações × 3 formatos × 3 temas), renderização determinística (duas execuções, hashes idênticos), teste `visual_regression_matches_the_golden_hashes` |
| Quick Notes sem regressão | Parcial: os 100 testes do `pod-app` (inclui runtime, storage e Quick Notes) passam e Quick Notes não foi tocado. O roteiro de janela real do M4 **não foi reexecutado** |

## Medições (40 cartões: relógios e progressos, 1×1/2×1/2×2)

| Métrica | Meta | Medido |
|---|---|---|
| Quadros na virada do minuto | ≈ 40 (um por cartão) | 40 |
| Quadros em 45 s sem virada | 0 | 0 |
| CPU em repouso | < 0,1% | 0,000% (0 s de CPU em 45 s) |
| p95 do trabalho de UI por quadro | < 8 ms | 0,65 ms (p50 0,24; máx 1,76) |
| Memória privada / handles após 3 min | sem crescimento | 38,5 → 38,8 MB; handles 1285 → 1281 |
| GPU local | informativo | 27,7 MiB |

O timer só existe se algum cartão visível assina a hora; em repouso o processo não acorda entre as viradas.

## Verificado em janela real

- 5 cartões restaurados do schema 3 abrem nos tamanhos exatos do formato (88×88, 188×88, 188×188 DIP a 100%).
- Menu de contexto do cartão: ajustes do widget (período; 24 h / segundos), "Tamanho", mover, ocultar, excluir.
- Trocar para 2×2 pelo submenu redimensiona a janela e grava `shape` no schema 3.
- Um arquivo schema 2 é lido e regravado como 3 (teste), preservando o original como nas migrações anteriores.

## Limites conhecidos (não tratados no W1)

- Quick Notes e `test_card` continuam no `Element` antigo (`tile_view` devolve `None`); a unificação fica para depois.
- Cartões não têm botão de menu visível: o menu abre por clique direito. Acesso apenas por teclado não foi testado.
- Cartões escondidos pelo modo "app em tela cheia" continuam recebendo a hora; só os ocultos pelo usuário são pulados.
- Ícones usam Segoe MDL2 Assets (nenhum widget atual os usa); o desenho do ícone não foi verificado.
- Os hashes dourados valem para a fonte (Segoe UI Variable Text) e o WARP desta máquina; regravar com `POD_UPDATE_GOLDEN=1 cargo test -p pod-app visual`.
- Narrator/leitor de tela e DPI misto real continuam não testados (pendências herdadas).
