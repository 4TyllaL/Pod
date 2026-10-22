# Fechamento de pendências antes do M7

Ambiente: Windows 11 build 26200, build de debug, 3 monitores a 100% DPI. Dados de teste em
`POD_DATA_DIR`. Atualiza as pendências de `m4-quick-notes.md` e `m6-visual-a11y-perf.md`.

## 1. Mover e redimensionar pelo teclado — fechado

Alt+F7 (mover) e Alt+F8 (redimensionar), ou "Mover"/"Redimensionar" no menu da nota (que também abre por
Shift+F10, tecla de menu ou Enter no botão "⋯"). Setas em passos de 8 DIPs (32 com Shift); Enter confirma
e grava a preferência; Esc cancela e restaura; perder o foco cancela. O rodapé mostra as teclas e o contorno
vai para o acento. A nota não sai da tela: precisa manter 64×32 px sobre algum monitor.
Teclas e caracteres são consumidos durante o modo (nada vai ao editor).

Testes automatizados (7): passo escalado por DPI e Shift, tamanho preservado ao mover, redimensionar mexe só
no canto inferior direito, tamanho mínimo, alcançabilidade com origem negativa e vários monitores.

Ao vivo: 5×→ + Shift+↓ moveu de (150,150) para (190,182) e gravou `offset=(190,182)`; Alt+F8 deu 360×224 e
gravou o tamanho; Esc restaurou a posição exata; "zzz" digitado no modo não entrou no texto; pelo menu
(Shift+F10 → Mover) o modo começou; 400 passos Shift+→ pararam em x=3774 (limite 3776), a nota continuou
sobre o monitor da direita.

Limite: sem teste a 150/200% DPI nem entre monitores de escalas diferentes (só testes de regra).
Anotação: um primeiro teste falhou por um erro do meu script (setas sem a flag de tecla estendida viram
teclado numérico; `R` é alias de `Invoke-History`), não do app.

## 2. Tema real — verificado em parte; a troca real fica com você

Eu não alterei o tema do Windows: é uma configuração do sistema e ela precisa ser trocada por você.

Verificado sem alterar nada:
- O app lê `AppsUseLightTheme` e concorda com uma leitura independente do registro (ambos: escuro;
  `em uso: Dark`).
- Uma `WM_SETTINGCHANGE` real com a string "ImmersiveColorSet" (enviada com `SendMessageTimeout`, que
  empacota a string entre processos) chega à janela de controle e reavalia o tema; "Environment" é ignorada;
  o código SPI de alto contraste (0x43) também reavalia.
- Troca de tema, cores e contraste: já cobertos pelos temas forçados em debug (M6) e pelos testes WCAG.

**Pendente (para você rodar):** `tools/verify-theme.ps1` inverte `AppsUseLightTheme`, difunde o
"ImmersiveColorSet" como o Windows faz, mostra as linhas `tema` do log do Pod e grava
`tema_antes.png`/`tema_depois.png`; restaura o valor original no final. Alto contraste real (Alt+Shift+PrtSc)
também não foi testado: só o tema forçado em debug, com cores do esquema normal.

## 3. Bandeja — menu e comandos verificados; ícone visível não confirmado

Verificado (o callback real do ícone, `WM_TRAY` + `WM_RBUTTONUP`, abre o menu nativo; itens lidos pelo `HMENU`):
- Itens: `Nova nota | Primeira linha da nota | Escondida && guardada (oculta) | Ocultar todas | Sair`.
- "Nova nota": janelas 2→3 e 3 instâncias no arquivo.
- Item "(oculta)": a nota volta a ficar visível e `visible=true` é gravado (1 oculta → 0).
- "Ocultar todas": todas somem, o rótulo passa a "Mostrar todas" e a preferência gravada não muda;
  "Mostrar todas" devolve.
- `TaskbarCreated` (mensagem registrada, enviada ao Pod): o handler recria o ícone (`NIM_ADD` 1→2).
- "Sair": o processo encerra com código 0 e o arquivo fica válido com as 3 instâncias.
- `Shell_NotifyIconW(NIM_MODIFY)` devolve verdadeiro: o shell conhece o ícone.

**Não confirmado:** ver o ícone na barra. `Shell_NotifyIconGetRect` falha (`E_FAIL`) e a captura da área de
notificação mostra só o chevron de ícones ocultos, então o ícone deve estar no overflow (comportamento
normal de ícones novos no Windows 11), mas não o vi. O ícone é o genérico `IDI_APPLICATION`. Também
não reiniciei o Explorer de verdade (derrubaria a sua área de trabalho): só a mensagem `TaskbarCreated`.
O menu é construído com os nomes em memória; um `&` no texto da nota é escapado (`&&`).

## Defeitos desta rodada

Nenhum defeito do app novo; uma varredura ampla de UI Automation de um script antigo (`m4b.ps1`) foi
removida (ela alcançava elementos de outros aplicativos).
