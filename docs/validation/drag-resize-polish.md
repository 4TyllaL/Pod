# Arrastar, redimensionar e acabamento (correção após uso real)

## Problema relatado
"Não dá para arrastar nem redimensionar" e aparência pouco refinada.

## Causa e correção (verificadas ao vivo com mouse sintético)
- O arrasto dependia do loop modal do sistema (`SC_MOVE`). Em teste, o loop recebia só o primeiro
  movimento e o cartão não seguia o cursor. Agora o cartão trata o arrasto: `WM_LBUTTONDOWN` + captura do
  mouse + `WM_MOUSEMOVE`; o cartão acompanha livremente e **encaixa na grade ao soltar** (e fica dentro da
  área útil). Fora do modo de edição há limite de 4 DIP para não confundir clique com arrasto; botões do
  cartão continuam clicáveis. Medido: cartão 2×1 de (150,510) para (318,618) após arrastar 240×160 px.
- Redimensionar: no modo de edição, cartões com mais de um formato mostram uma alça (canto inferior
  direito). Arrastá-la escolhe o formato mais próximo entre os que o tipo declara. Medido: 2×2 → 1×1.
- A galeria "Adicionar widget" ficava no centro e cobria os cartões; agora fica encostada à direita.
- Gancho só de depuração `POD_START_EDIT=1` abre direto no modo de edição (testes ao vivo).

## Acabamento
Superfícies neutras (branco/cinza frio e grafite), borda mais fina, menos tinta de cor no topo, raio 22,
botões "tonais" (pílula com a cor em 18%), título do calendário na cor de destaque. Vidro escuro subiu de
0,90 para 0,93 para manter o contraste.

## Não resolvido / a confirmar
- Fora do modo de edição os cartões ficam atrás dos aplicativos (comportamento de desktop): para mexer
  neles com janelas maximizadas é preciso entrar em "Editar widgets…" (bandeja ou botão direito).
- Teste ao vivo feito com mouse sintético; falta confirmar com o mouse real do usuário.
- Acabamento é uma primeira passada (cores, botões, calendário); ícones e tipografia ainda não são SF.

## Ícones e tipografia (segunda passada)
- `icons.rs`: ícones vetoriais próprios (traço arredondado, estilo SF Symbols) para sol, lua, nuvem,
  sol com nuvem, chuva, neve, raio, nota musical, anterior/próxima/play/pausa, brilho (Claude), jornal e
  seta de abrir. O renderer troca o caractere por este desenho em textos e rótulos de botão; qualquer
  outro caractere segue como texto. Sol e raio usam tom quente fixo; nuvem é neutra.
- Tipografia: títulos e numerais pedem "Segoe UI Variable Display" quando instalada (aqui só há a Text),
  legendas em peso médio. Não é SF Pro: nenhuma fonte da Apple é usada.
- Setas ↑ ↓ do widget Rede e o ícone de clima por código (chuva, neve etc.) só foram vistos pela
  galeria com o clima padrão; os demais códigos têm teste de descrição, não de desenho.
