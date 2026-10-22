# Prova S5 e S6 da sombra ampla: geometria, grade, monitores, selos, UI Automation, alto contraste e DPI (ADR-0005, etapa 5)

Data: 21 de outubro de 2026. Windows 11 build 26200, **3 monitores reais (todos a 96 DPI)**, build de debug. Código:
`crates/pod-app/src/shadow_layer.rs` (`pod-app --shadow-s56 <pasta>`; modo `hold` descrito no fim). Relatório da execução 3:
[shadow-s56-report.md](shadow-s56-report.md). **3 execuções seguidas, todas com todas as verificações passando** (58
verificações positivas, 0 falhas por execução). Isolado: não cria cartões do produto, não mexe em workspace nem na bandeja.

## O que foi usado do produto

- `pod_model::placement` (`to_preferred`, `resolve_all`, `effective_rect`) e `pod_model::grid` (`snap_axis`, `is_snapped`,
  `pitch_px`, `margin_px`), `crate::monitors::enumerate` (monitores reais), `crate::sysinfo` (alto contraste, animações,
  transparência) e o mesmo encaixe de `host::snap_tile_rect`.
- A prova passou a guardar o **DPI por cartão** (antes era global): a margem da sombra é `round(24 × DPI/96)` do monitor do
  cartão, e a imagem por nove fatias é montada para esse DPI.

## S5: geometria cartão ↔ janela, grade, monitores, selos

| Passo | Resultado |
| --- | --- |
| **S5.1 posicionamento real do produto**: layouts sintéticos (1 monitor; 2 monitores com DPI 96+144 e 96+192, coordenadas negativas; monitor secundário removido com cartões remapeados) | 6/6, 12/12, 12/12 cartões **dentro da área útil**, **posição gravada ida e volta (±1 px)**, **na grade**; 18/18 remapeados dentro da área do monitor principal |
| S5.2 bordas do monitor real: cartão parcialmente fora do desktop virtual, no canto, de volta | sombra = cartão ± margem em todos; ordem Z e geometria mantidas |
| **S5.2b cartão metade em cada monitor** (bordas reais x=1920 e x=0, 96 DPI nos dois lados) | sombra = cartão ± 24 px e inteira no desktop virtual |
| **S5.3 arrastar com encaixe na grade** (`SendInput` real, mesmo encaixe do host) | cartão ficou na grade do monitor; a sombra acompanhou cada posição (0 divergências, 0 posições fora da grade) |
| **S5.4 selos do modo de edição** (clique real no ponto do selo, 16 DIP do canto) em 2 cartões e na nota | a janela no ponto do selo é o cartão e o clique chega a (16, 16) do cliente, **inclusive na faixa do cartão coberta pela margem da sombra do vizinho** (cenário existe e o cartão vence: as sombras ficam abaixo de todos os cartões) |

**Por que a posição gravada não muda:** a sombra é uma janela separada; o retângulo do **cartão** (que o produto grava) continua
sendo o da janela do cartão. Gravar o retângulo da sombra seria o erro; a prova confirma que, usando o do cartão, o ciclo
`to_preferred` → `effective_rect` devolve o mesmo retângulo.

**Informativo (não reprova):** nos layouts com monitor vizinho, **3 sombras invadem o monitor vizinho em até 24 px** (um
cartão na primeira coluna da grade fica a 12 DIP da borda e a sombra tem 24 DIP). Vira uma franja escura no outro monitor,
possivelmente com DPI diferente. Mitigação possível, **não testada**: recortar a sombra pelo monitor do cartão.

## S6: UI Automation, alto contraste, DPI misto

### S6.1 UI Automation

Cliente UI Automation (outra thread) conta, por visão da árvore, as janelas de cartão e de sombra do processo:

| Provedor da janela de sombra | Cartões (bruta / controle) | Sombras (bruta / controle / conteúdo) |
| --- | --- | --- |
| padrão do HWND (sem mitigação) | 3 / 3 | **3 / 3 / 3** |
| provedor que não é elemento de controle nem de conteúdo | 3 / 3 | **3 / 0 / 0** |
| sem provedor (`WM_GETOBJECT` devolve 0) | 3 / 3 | 3 / 3 / 3 (**não esconde**) |

- **Sem mitigação, cada sombra aparece na árvore de controle e de conteúdo**, onde um leitor de tela as encontraria como
  janelas sem nome. **Isso é um problema real da sombra em janela separada.**
- **Mitigação que funciona:** `WM_GETOBJECT` devolve um provedor mínimo (`IRawElementProviderSimple` que responde
  `IsControlElement = false` e `IsContentElement = false` e hospeda o provedor padrão do HWND). Resultado: 0 sombras nas visões de
  controle e de conteúdo, **os cartões continuam todos na visão de controle**. A sombra continua na visão bruta (é uma janela).
- **O que não funciona:** devolver 0 em `WM_GETOBJECT` não esconde nada.
- Testado só com um cliente UI Automation (contagem por visão). **Não testado com o Narrator nem com os provedores reais dos
  cartões** (`a11y.rs`): a prova usa cartões com o provedor padrão.

### S6.2 Alto contraste e "reduzir efeitos"

Estado desta máquina: alto contraste = desligado, animações = ligadas, efeitos de transparência = ligados.

- **Política** (`wide_shadow_allowed`): `sombra ampla = !alto_contraste && !animações_desligadas` (teste unitário da tabela).
- **Simulado** (não alterei configurações do sistema, regra do projeto): forçar "desligada" esconde todas as sombras, **todos os
  cartões continuam visíveis**, mover e redimensionar o cartão continua funcionando; forçar "ligada" volta com todas as sombras
  visíveis, na ordem e na geometria certas. É a mesma função `apply_policy` que reage a `WM_SETTINGCHANGE`/`WM_THEMECHANGED`.
- **Reação ao vivo ao alto contraste real: NÃO verificada.** Modo `hold`: `pod-app --shadow-s56 <pasta> hold` abre 3 cartões com sombra por
  2 minutos e grava `live-log.md`; alterne o alto contraste (Alt esquerdo + Shift esquerdo + Print Screen) e as animações do Windows,
  cada mudança que o Windows avisa registra uma linha com o estado lido e se a sombra ficou ligada ou desligada. **Peço que você rode:**
  mudar essas configurações é o que o projeto não permite que eu faça.

### S6.3 DPI misto

- **Monitores reais:** 3, todos a 96 DPI → **não há DPI misto real para testar**. O que foi feito é **simulação**.
- Dois cartões a 96 e 192 DPI **ao mesmo tempo**: cada sombra = seu cartão ± a margem do seu DPI (24 e 48 px); ordem e geometria mantidas.
- **Troca de DPI de um cartão** (o que `WM_DPICHANGED` faria ao cruzar monitores) 96 → 120, 144, 168, 192, 240: cartão de 110, 132, 154,
  176 e 220 px, margem de 30, 36, 42, 48 e 60 px, sombra = cartão ± margem **sem fresta**, imagem refeita no DPI novo.
- Teste unitário (`policy_table_and_dpi_math`) para 96, 120, 144, 168, 192, 240 e 288 DPI: a sombra por nove fatias tem
  exatamente `cartão + 2 × margem` e fica a ≤ 2/255 de alfa da sombra completa.

## Achados paralelos (não são da sombra, mas apareceram nas provas)

1. **Clicar num cartão deixa "nenhuma janela" em primeiro plano.** Com o Chrome (ou a Área de Trabalho Remota) em foco, um clique
   real num cartão (`MA_NOACTIVATE`, como no host) fez `GetForegroundWindow()` devolver nenhuma janela. **Controle: o resultado é igual
   com a sombra escondida**, então não vem dela. O cartão da prova copia o contrato do host; **não verifiquei o aplicativo real**.
   **Investigado no app real: confirmado**, causa: falta de `WS_EX_NOACTIVATE`; **corrigido no host** (ver [focus-click.md](focus-click.md)).
2. **A nota ignorou `HWND_TOPMOST` depois de um redimensionamento real pelo laço modal do sistema** (a chamada devolve sucesso e o
   estilo não muda); só subiu depois de `HWND_NOTOPMOST` + `HWND_TOPMOST`. Sem o redimensionamento real subiu direto. Vale conferir
   `host::set_edit_layer`.
3. **Cartões cobertos por outra janela** (o navegador em tela cheia): cliques e arrastes reais só os alcançam em modo de edição
   (`TOPMOST`). Nesta máquina os passos S5.3 e S5.4 rodaram assim.

## Defeitos da própria prova encontrados e corrigidos

- A janela normal de teste era criada com `WS_VISIBLE` e **ativava, roubando o foco do usuário**; as verificações de foco comparavam com
  uma base tirada depois disso. Corrigido nos dois harnesses (S2 e S5/S6): a base agora é o primeiro plano **antes de criar qualquer
  janela**, com uma linha própria que confirma que **criar cartões, sombras e a janela normal não mudou o primeiro plano**.
- O processo caía na saída (superfícies de DirectComposition destruídas depois do `Gfx`); corrigido.

## Limites (não verificado)

- **Alto contraste real e animações reais** (ver `hold`), **DPI misto real**, Narrator, os provedores de UI Automation reais dos cartões e
  o editor Rich Edit real.
- Os "cartões" são um bitmap: sem widgets, painéis, selos desenhados de verdade (o selo é só o ponto de clique).
- Sincronia visual cartão ↔ sombra no compositor, custo no DWM e 150%/200% reais continuam como em [shadow-gate.md](shadow-gate.md) e
  [shadow-layer.md](shadow-layer.md).

## Conclusão para o ADR-0005

1. **S5 passa:** o posicionamento, a grade, os monitores e os selos do produto continuam valendo, porque a sombra é outra janela e o
   retângulo do cartão não muda. Efeito colateral a decidir: a sombra de um cartão perto da borda invade o monitor vizinho em até 24 px.
2. **S6 passa com uma exigência nova:** a janela de sombra **precisa de um provedor de UI Automation que a esconda das visões de controle e
   de conteúdo**; sem ele, cada sombra vira um elemento anunciado.
3. **Alto contraste e animações:** a política e a reação existem e funcionam simuladas; a confirmação com configurações reais depende de você
   (modo `hold`). **DPI misto:** só simulado.
