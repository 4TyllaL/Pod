# Legibilidade do vidro simulado sobre papéis de parede reais (ADR-0005, etapa 1 / critério V1)

Data: 21 de outubro de 2026. Ambiente: Windows 11 build 26200, um monitor 2000×1200. Build de teste (debug).

## O que foi medido

- **Papéis de parede:** os 26 que o Windows traz em `C:\Windows\Web\Wallpaper` e `C:\Windows\Web\4K\Wallpaper`
  (duplicatas descartadas). Luminância média de 8 a 173 em 255 (do quase preto ao claro-médio).
- **Método:** `tools/dump-wallpapers.ps1` reduz cada imagem a 960×540 e grava pixels brutos. O teste ignorado
  `ui::theme::tests::measure_glass_legibility_on_real_wallpapers` compõe a superfície translúcida do tema de vidro
  (alfa 0,86 claro / 0,93 escuro, **nas duas pontas do gradiente**, usando o pior caso) sobre os pixels e mede o
  contraste WCAG de cada papel de texto. Cartões de 1×1, 2×2 e 4×4 (88, 188 e 388 DIP, em tela de 1920×1080 DIP)
  em grade de posições (≈ 12 mil a 21 mil cartões por tamanho e tema).
- **Critério por cartão:** 5º percentil do contraste (um pixel isolado não define leitura). Também registrado o pior pixel.
- **Mínimos:** texto principal 7:1; secundário, acentos, positivo/aviso/perigo e vermelho de texto 4,5:1;
  ícones dos cabeçalhos 3:1.
- Reprodução:

```powershell
powershell -File tools/dump-wallpapers.ps1 -Out <pasta>
$env:POD_WALLPAPER_DIR = "<pasta>"; $env:POD_REPORT_OUT = "<relatorio.md>"
cargo test -p pod-app measure_glass -- --ignored
```

## Achado e correção

A primeira medição **reprovou** os papéis de cor de símbolo que entraram na entrega de estilo macOS, porque
o modo vidro não os ajustava (o teste de legibilidade do vidro só cobria os papéis antigos):

| Papel (vidro) | Pior p5 antes | Mínimo | Cartões abaixo do mínimo antes |
| --- | --- | --- | --- |
| Laranja, claro | 1,87 | 3,0 | 100% |
| Ciano, claro | 2,38 | 3,0 | ≈ 85–93% |
| Argila, claro | 2,75 | 3,0 | ≈ 60–69% |
| Vermelho (texto), claro | 3,00 | 4,5 | 100% |
| Vermelho (texto), escuro | 3,12 | 4,5 | 100% |
| Azul, escuro | 2,91 | 3,0 | ≈ 2% |

Correção em `Theme::role_rgb_color`: no vidro, os papéis de símbolo escurecem no claro e clareiam no escuro.
Os sete papéis entraram em `glass_is_readable_on_any_backdrop` (preto, branco e cinza), para não regredirem.
Fora do vidro as cores não mudaram.

## Resultado depois da correção

**0 cartões abaixo do mínimo em todos os papéis, tamanhos e temas** (relatório completo em
[glass-legibility-report.md](glass-legibility-report.md)). Piores 5º percentis:

| Papel | Claro | Escuro | Mínimo |
| --- | --- | --- | --- |
| Primário | 11,96 | 9,99 | 7,0 |
| Secundário | 6,52 | 6,73 | 4,5 |
| Acento (pior dos 4) | 5,30 | **4,57** | 4,5 |
| Vermelho (texto) | 4,83 | 4,91 | 4,5 |
| Ícones (pior) | 3,75 (Ciano/Laranja) | 3,70 (Rosa) | 3,0 |

Margem fina: o acento 0 do tema escuro fica em 4,57 contra 4,5.

## Papel de parede do próprio usuário (mesma data)

Medido o `TranscodedWallpaper` atual do Windows do dono (floresta escura com um feixe de luz; luminância média 52 em
255), com `POD_WALLPAPER_ONLY=TranscodedWallpaper` e `-Extra` no `tools/dump-wallpapers.ps1`. **0 cartões abaixo do
mínimo** em todos os papéis, tamanhos e temas ([relatório](glass-legibility-user-wallpaper.md)). No claro os piores
valores são os mesmos dos extremos (texto 11,96; secundário 6,52); no escuro ficam um pouco acima dos mínimos
(texto 10,19; acento 0 em 4,66; Rosa 3,77). Não substitui um papel claro: a cena é escura.

## Limites desta medição

- **Não há papel de parede claro** entre os 26 (média máxima 173). Fundos quase brancos só estão cobertos pelo teste
  sintético (preto, branco e cinza), não por imagem real. **V1 pede papel claro, listrado e foto detalhada reais:
  falta fornecer ao menos um papel de parede claro e um de foto com muito detalhe.**
- Mede contraste **calculado**, não a tela renderizada: sem antialiasing, sem o texto real do widget, sem a sombra.
- Amostras em metade da resolução e posições em grade de 24 px; um detalhe fino pode escapar do percentil.
- Só o Windows 11 desta máquina, um monitor, sem DPI misto. Nada ao vivo (troca de "efeitos de transparência", alto contraste).
- Legibilidade não é aprovação visual: **V4 (aval do dono vendo capturas reais) continua pendente.**

## Situação do critério V1 (ADR-0005)

**Parcial, sem reprovação:** numericamente passa para os 26 papéis disponíveis depois da correção; fica
pendente o item "papel claro/foto detalhada reais". Etapa 1 do ADR concluída; as etapas 2 (janela de prova da
sombra) e 3 (backdrop do DWM) não foram iniciadas.
