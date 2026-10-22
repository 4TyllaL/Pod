# P4 (ADR-0003): galeria "Adicionar widget", modo de edição "+/−" e grade

Ambiente: Windows 11 build 26200, um monitor 2000×1200 a 144 DPI (150%), release com asserts de debug.

## O que existe

- **Menu da bandeja → "Editar widgets…"** liga o modo de edição (marcado enquanto ligado).
- **Galeria "Adicionar widget"** (`edit.rs`): janela de 628×580 DIP, cantos arredondados, arrastável pelo
  cabeçalho, com a prévia de cada widget do catálogo (formato padrão, dados de demonstração, cor de
  identidade, selo verde "+"). Rolagem pela roda do mouse. Clicar numa prévia **adiciona** o widget e o rótulo
  passa a "adicionado ×N". **Concluído** ou **Esc** fecha a galeria e sai da edição.
- **Selo vermelho "−"** no canto superior esquerdo de cada cartão enquanto edita: clicar remove (pelo mesmo
  caminho de exclusão e gravação de antes). Em edição o cartão inteiro arrasta e os botões do widget não
  disparam.
- **Grade** (`pod-model::grid`): passo de 100 DIP (célula 88 + espaço 12) com margem de 12 DIP a partir do
  canto da área útil do monitor. Cartões **encaixam durante o arraste** (`WM_MOVING`) e **novos cartões
  nascem na primeira célula livre** (linha a linha); sem lugar, voltam à cascata de antes. Notas se movem livres.

## Testes

- Regras puras (113 no `pod-model`): passo e margem por DPI, ponto de grade mais próximo, monitor com origem
  negativa, idempotência, primeira célula livre (pula ocupadas, quebra de linha, sem lugar, alinhada).
- `pod-app` (198): plano da galeria (quebra de linha, sem sobreposição, item largo demais, vazio) e cabeçalho
  com botão "Concluído" dentro dos limites.
- **Ao vivo** (scripts fora do repositório): menu → galeria abriu (942×870 px = 628×580 DIP a 150%); clique na
  primeira prévia aumentou os cartões de 1 para 2; clique no selo "−" do cartão (0,0) removeu (2 → 1);
  **WM_MOVING real** enviado a um cartão com retângulos fora da grade devolveu posições alinhadas
  (100,100→168,168; 217,333→168,318; 5,400→18,468; 1000,700→1068,768) com o tamanho preservado;
  o novo cartão nasceu em (318,18), na grade; "Concluído" fechou a galeria e saiu da edição.

## Não verificado / limites

- **Arrastar de verdade com o mouse**: cliques e `WM_MOVING` foram verificados, mas o mouse sintético
  (`mouse_event`) não conseguiu mover janelas, então o encaixe foi provado pela mensagem real, não por arraste.
- A galeria **não tem UI Automation nem navegação por teclado** além de Esc (Tab/Enter não existem). É uma
  lacuna de acessibilidade em relação ao resto do Pod.
- Selo "−" e galeria foram vistos em captura (cartão de CPU e galeria), só em tema escuro e 150% de DPI.
- Cartões antigos fora da grade só encaixam quando movidos; não há "arrumar tudo".
- Sem animação de "tremer" do iOS nem arrastar da galeria para o desktop (clique adiciona).
- Em vários monitores, a galeria abre sempre no principal.
