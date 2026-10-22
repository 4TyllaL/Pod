# Portão de 40 janelas da sombra ampla (ADR-0005, etapa 3)

Data: 21 de outubro de 2026. Windows 11 build 26200, um monitor, escala 100%, **build de debug**, com o resto da sua
área de trabalho em uso (Chrome etc.). Código: `crates/pod-app/src/shadow_proof.rs` (`pod-app --shadow-gate <pasta>`),
`tools/gate-sample.ps1` (amostragem de GPU por processo e CPU/memória do `dwm.exe`). Relatórios brutos das execuções
3 e 4: [run3](shadow-gate-report-run3.md) e [run4](shadow-gate-report-run4.md) (as execuções 1 e 2 foram feitas antes
do teste de clique em DirectComposition e só têm a tabela de custo; entram na mediana abaixo).

## Como foi medido

- **Mesmo caminho dos cartões:** as janelas usam `gfx::Gfx` e `gfx::Surface` (swap chain de composição com alfa
  pré-multiplicado, D2D, `Present(1)`), `WS_EX_NOREDIRECTIONBITMAP`, `WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`.
- **40 cartões de 88 DIP** em grade 10×4, todos visíveis (`TOPMOST`), cinco fases: sem janelas; 40 cartões (hoje);
  **A** 40 janelas únicas com margem; **B** 40 cartões + 40 janelas de sombra (margem de 24 DIP), ambas em
  DirectComposition; **B2** 40 cartões em DirectComposition + 40 sombras em camadas (`UpdateLayeredWindow`).
- Cada fase: 2 s de acomodação e 6 s de medida **ociosa** (zero quadros) e 6 s de medida **a 1 Hz** (cada cartão e
  cada janela "viva" redesenha uma vez por segundo, como um relógio; as sombras nunca redesenham).
- CPU do Pod (`GetProcessTimes`), memória do Pod (working set e privada), GPU do Pod e do `dwm` ("GPU Engine"),
  memória de GPU dedicada do Pod e do `dwm` ("GPU Process Memory"), CPU do `dwm` (WMI).
- **4 execuções**; abaixo mediana (mínimo–máximo).

## Resultado 1: clique atravessando a sombra em DirectComposition (S1)

| Janela de sombra em DirectComposition | Cliques na margem chegam à janela de baixo (outro processo)? |
| --- | --- |
| só `WS_EX_TRANSPARENT` | **NÃO** em nenhum dos 3 pontos, **nem na margem de alfa 0**: sem `WS_EX_LAYERED` a janela inteira recebe cliques |
| `WS_EX_LAYERED` + `WS_EX_TRANSPARENT` (com `SetLayeredWindowAttributes(255)`) | **sim**, nos 3 pontos; o centro do cartão continua recebendo |

A janela de sombra em DirectComposition **só passa a ser transparente a cliques com `WS_EX_LAYERED`**. Foi um
achado que o ADR não previa: copiar o estilo "só `WS_EX_TRANSPARENT`" do desenho da etapa 2 reprovaria.

## Resultado 2: custo com 40 janelas (mediana, mín–máx de 4 execuções)

| Fase | Janelas | Carga | CPU Pod % | GPU Pod % | WS Pod MiB | Privada Pod MiB | Mem. GPU Pod MiB | CPU dwm % | GPU dwm % | Mem. GPU dwm MiB |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Sem janelas (linha de base) | 0 | ocioso | 0.06 (0.00-0.22) | 0.00 (0.00-0.00) | 37 (35-39) | 26 (25-28) | 9 (8-10) | 19.0 (5.0-28.0) | 2.46 (0.36-5.97) | 611 (589-615) |
| Sem janelas (linha de base) | 0 | 1 Hz | 0.12 (0.00-0.25) | 0.00 (0.00-0.00) | 37 (35-39) | 26 (25-29) | 9 (8-10) | 15.1 (1.7-32.5) | 1.38 (0.39-5.22) | 612 (589-615) |
| 40 cartoes, sem sombra (hoje) | 40 | ocioso | 0.12 (0.00-0.22) | 0.00 (0.00-0.00) | 41 (41-41) | 30 (30-30) | 11 (11-11) | 13.1 (6.7-26.7) | 1.34 (0.36-6.37) | 604 (590-648) |
| 40 cartoes, sem sombra (hoje) | 40 | 1 Hz | 2.95 (2.26-3.58) | 0.82 (0.75-0.90) | 41 (41-41) | 31 (31-31) | 12 (12-12) | 12.7 (8.5-23.0) | 2.66 (0.47-5.45) | 614 (592-646) |
| A: 40 janelas unicas com margem (DirectComposition; reprovada em clique) | 40 | ocioso | 0.11 (0.00-0.12) | 0.00 (0.00-0.00) | 41 (41-41) | 31 (31-31) | 20 (20-20) | 17.3 (7.5-38.0) | 3.42 (0.39-6.78) | 629 (599-656) |
| A: 40 janelas unicas com margem (DirectComposition; reprovada em clique) | 40 | 1 Hz | 3.56 (1.95-4.91) | 0.95 (0.89-1.08) | 41 (41-41) | 31 (31-31) | 30 (30-30) | 18.0 (1.7-31.7) | 3.27 (0.55-6.98) | 632 (609-663) |
| B: 40 cartoes + 40 janelas de sombra, ambas em DirectComposition | 80 | ocioso | 0.29 (0.20-0.37) | 0.00 (0.00-0.00) | 42 (42-43) | 32 (32-33) | 21 (21-21) | 14.6 (3.3-26.2) | 2.87 (0.38-5.52) | 622 (600-628) |
| B: 40 cartoes + 40 janelas de sombra, ambas em DirectComposition | 80 | 1 Hz | 2.41 (2.18-2.67) | 0.82 (0.75-0.88) | 43 (42-43) | 32 (32-33) | 22 (22-22) | 16.9 (5.8-25.8) | 2.92 (0.48-6.31) | 624 (602-629) |
| B2: 40 cartoes em DirectComposition + 40 sombras em camadas (UpdateLayeredWindow) | 80 | ocioso | 0.12 (0.00-0.25) | 0.00 (0.00-0.00) | 45 (44-45) | 32 (31-32) | 11 (11-11) | 12.4 (3.5-24.0) | 1.12 (0.53-5.50) | 612 (590-618) |
| B2: 40 cartoes em DirectComposition + 40 sombras em camadas (UpdateLayeredWindow) | 80 | 1 Hz | 2.62 (2.24-2.95) | 0.80 (0.75-0.86) | 45 (44-45) | 31 (31-32) | 12 (12-12) | 14.8 (4.3-20.3) | 2.73 (0.70-4.73) | 617 (610-619) |

### Leitura

**Lado do Pod (faixas estreitas, resultado confiável nesta máquina):**
- **Sem custo de CPU ou GPU a mais:** 40 cartões a 1 Hz custam ≈ 2,9% de CPU (debug) e 0,82% de GPU; com as sombras
  (B, B2) fica em 2,4–2,6% e 0,80–0,82% (as diferenças são ruído: as sombras não redesenham). Ocioso: ≈ 0.
- **Memória do processo:** working set 37 → 41 MiB (40 cartões) → 42–43 MiB (B) → 45 MiB (B2); privada 26 → 30 → 32.
  A sombra acrescenta ≈ **1 a 4 MiB** de RAM para 40 cartões.
- **Memória de GPU do Pod:** 9 → 11–12 MiB (40 cartões) → **21–22 MiB com sombras em DirectComposition (B)** →
  **11–12 MiB com sombras em camadas (B2)**. A janela única com margem (A) fica em 20–30 MiB. Ou seja, **B2 não
  gasta memória de GPU do Pod com as sombras**; B gasta ≈ +10 MiB (dois buffers de 136×136 por janela).

**Lado do DWM (compositor): NÃO resolvido.**
- A linha de base do próprio `dwm.exe` varia de **5% a 28% de CPU**, de **0,36% a 6% de GPU** e de **589 a 615 MiB** de
  memória de GPU entre amostras, por causa do que estava na tela. Essa variação é **maior que qualquer diferença
  entre fases**. As medianas sugerem +10 a +20 MiB de memória de GPU do DWM em A e B (622–632 contra 611) e ≈ 0 em B2
  (612–617), mas as faixas se sobrepõem, então **não é prova**. Para CPU e GPU do DWM não há efeito mensurável.

## Limites

- **Debug, não release:** a CPU absoluta do Pod está inflada; a comparação entre fases vale.
- **100% de escala, um monitor.** Em 150% e 200% os buffers das sombras crescem 2,25× e 4× (estimativa por cálculo
  de pixels, **não medida**).
- **Máquina com uso normal**: ruído do DWM descrito acima. Uma máquina quieta (ou mais repetições) é necessária
  para medir o custo no compositor.
- **Janelas `TOPMOST`**, não `HWND_BOTTOM` com o hook de `WM_WINDOWPOSCHANGING` e `MA_NOACTIVATE` dos cartões:
  **S2 completo (camada do desktop, arrastar, redimensionar, modo de edição) não foi testado.** Janelas de sombra
  em `HWND_BOTTOM` também podem ser cobertas por outras janelas e o DWM pode pular o que está oculto, mudando o custo.
- **S5 (geometria cartão ↔ janela), S6 (acessibilidade, alto contraste, DPI misto) e o desenho real dos widgets
  (texto, painéis) não foram testados**; os cartões da prova só mostram um bitmap.
- Só os caminhos DirectComposition (como hoje) e `UpdateLayeredWindow` foram comparados; não foi medido o custo de
  manter a janela de sombra imediatamente atrás do cartão quando o usuário arrasta ou o Windows reordena.

## Conclusão para o ADR-0005

1. **S1b (cartão + janela de sombra) é viável em clique e tem custo de Pod desprezível em CPU/GPU.** Requisito
   novo: a janela de sombra precisa de `WS_EX_LAYERED` + `WS_EX_TRANSPARENT`.
2. **Preferir sombra em camadas (B2)** ao DirectComposition para a janela de sombra: mesma passagem de clique, sem
   o +10 MiB de memória de GPU do Pod (e provavelmente sem custo extra de swap chain no DWM, a confirmar).
3. **Dobrar o número de janelas (80 para 40 cartões) não estourou nada medido do lado do Pod**; o lado do DWM segue
   sem medição conclusiva.
4. **Ainda faltam para fechar S:** S2 completo com `HWND_BOTTOM`/hook, S5, S6 e a medição do DWM em máquina quieta
   (ou com mais repetições, em release e em 150%/200%). Sem isso a sombra ampla **não entra**; `CS_DROPSHADOW` continua
   válido.
