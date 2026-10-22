# Paridade com os widgets de desktop do macOS

Alvo do dono: funcionar **idêntico ao macOS**. Quatro lacunas atacadas (as demais ficam listadas no fim).

## Feito

1. **Esmaecer quando um aplicativo está em foco** (como o Sonoma): `Theme::muted()` troca acentos e cores
   semânticas por cinza de mesma luminância; texto principal e secundário não mudam. Alto contraste não
   esmaece. O Shell reavalia a cada evento de primeiro plano (o hook agora entrega a janela que ganhou o foco) e
   de novo ~500 ms depois, para pegar o estado assentado. Em edição os cartões ficam coloridos. Galeria ganhou
   `muted-light` e `muted-dark` (e 198 hashes novos).
2. **Menu do widget no estilo macOS**: "Editar widgets…" (ou "Concluir edição") e **Tamanho** com
   **Pequeno / Médio / Grande** (e "Extra grande"), por ordem de área entre os formatos que o widget tem
   (`Shape::size_name`). Lido do `HMENU` real: `Pequeno (1×1)`, `Médio (2×1)`, `Grande (2×2) [x]`.
3. **Arrastar a prévia da galeria para o desktop**: depois de 6 px com o botão apertado, o cartão nasce sob o
   cursor e o movimento passa a ele (`SC_MOVE`), com encaixe na grade e gravação ao soltar. Soltar sem arrastar
   continua adicionando (clique).

## Verificado ao vivo

- Menu do cartão (leitura do `HMENU`): itens e tamanhos acima, o formato atual marcado.
- Foco: o log registrou `app em foco: true` ao abrir uma janela comum e `false` ao fechá-la.
- Arraste da galeria: de 1 para 2 cartões; o novo cartão acompanhou o cursor (18,1218 durante, 18,1156 ao
  soltar) com o X na grade.

## Não verificado / limites

- O esmaecimento só foi visto nas imagens da galeria; **não vi o cartão mudar de cor na tela** (ele fica atrás dos
  aplicativos). A troca é instantânea, sem a transição suave do macOS.
- No arraste de teste, o ponteiro (aceleração do mouse) foi parar fora da tela; o cartão foi reposicionado para
  dentro e, por isso, ao soltar o Y não ficou na grade. Arraste normal dentro da tela não foi reproduzido.
- O Pod **não** consegue o menu de botão direito **no fundo do desktop** (exigiria hook no Explorer, vetado
  no ARCHITECTURE §5): a edição se abre pelo botão direito **no widget** ou pela bandeja.

## Lacunas restantes em relação ao macOS

- Clicar no papel de parede para "revelar o desktop" (feature do Explorer/WindowServer, não reproduzível sem hook).
- Transição animada ao esmaecer, tremer ao editar, widgets de iPhone/Continuidade, empilhamento (Smart Stack).
- Galeria com busca, categorias e visualização de todos os tamanhos de cada widget.
- Cantos/sombra e vidro de verdade (DWM) continuam como nas notas anteriores.

## Segunda rodada

- **Transição suave do esmaecimento** (~220 ms, curva suave): `Theme::muted_by(0..1)` interpola cada acento e cor
  semântica até o cinza; o Shell anima o valor por um timer de ~16 ms e repinta só enquanto muda. Verificado
  no log: 8 quadros de 0,97 a 0,00 ao devolver o foco ao desktop e de 0,03 a 1,00 ao abrir um app. A
  galeria offscreen segue com os extremos (`muted-*`).
- **Galeria por teclado**: Tab/Shift+Tab e setas movem a seleção (anel de foco, a lista rola para mostrá-la),
  Enter ou Espaço adicionam, Esc fecha. Ao vivo: Tab, Direita, Baixo, Enter aumentou os cartões de 2 para 3 e
  Esc fechou a galeria. `neighbor` (setas entre linhas) tem teste de regra.
- Suítes: 201 no `pod-app`, 114 no `pod-model`.

Ainda sem: UI Automation da galeria (só o nome da janela), busca/categorias, e ver o esmaecimento na tela
(o cartão fica atrás dos aplicativos; confirmei pelo log e pela galeria).

## Terceira rodada: busca e categorias na galeria

- **Busca** (`pod-model::search`): sem diferenciar maiúsculas nem acentos, por palavras (todas precisam
  aparecer); o texto pesquisado é o nome do widget mais a categoria. Digita-se direto na galeria (Backspace
  apaga); **Esc limpa a busca** e, com a busca vazia, fecha. Sem resultado: "Nenhum widget encontrado".
- **Categorias** (`widgets::CATEGORIES`/`category_of`): Todos, Tempo, Produtividade, Sistema e Arquivos, como
  pílulas ao lado da busca; a ativa fica destacada. Um teste garante que todo widget do catálogo tem
  categoria e que nenhuma fica vazia. O filtro de categoria e busca juntos é `filter_flags` (testado).
- A lista é reorganizada a cada mudança (rolagem e seleção se ajustam); setas, Tab e Enter operam só sobre os
  itens visíveis, e Enter com um único resultado o adiciona. Espaço agora digita na busca (não adiciona).
- Ao vivo: "agu" mostrou só Água; a categoria Produtividade mostrou 5 widgets; "zzz" em Arquivos mostrou
  o estado vazio; as pílulas cabem na largura da galeria. 203 testes no `pod-app`, 116 no `pod-model`.
- Não verificado: IME e colar (Ctrl+V) na busca; clique na caixa de busca (o foco já é dela); UI Automation da
  galeria segue só com o nome da janela.
