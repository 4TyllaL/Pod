# M4 — Quick Notes utilizável

Ambiente: Windows 11, build de debug, 100% DPI, teclado PT-BR/ABNT2, um monitor principal de teste.
Os testes usaram `POD_DATA_DIR` (só em debug); o perfil real não foi tocado.

## O que existe

- `widgets/quick_notes`: texto simples UTF-8 por instância; limite de 256 KiB **em bytes**,
  recusado (o texto anterior volta ao campo e a view mostra o erro), sem truncamento silencioso;
  `has_content` para decidir se a exclusão pede confirmação.
- Contrato: `Effect::SetText` (o widget manda trocar o texto do campo) e `Widget::has_content`.
- Rich Edit windowless religado: o editor existe só para views com campo de texto, posicionado pelo
  layout, reposicionado em resize/DPI; `EN_CHANGE` → texto confirmado (LF) → `Event::TextChanged`;
  nada é entregue durante composição IME; Esc encerra a edição fora de composição.
- Feedback de salvamento no rodapé: "Salvando…" (sujo), "Salvo" (só após ack da geração) e
  "Erro ao salvar — tentando de novo".
- Exclusão: confirmação local (Sim/Não, Não como padrão) quando há conteúdo; o menu da faixa de
  movimentação (clique direito) tem Nova nota, Ocultar e Excluir.
- Bandeja (`tray.rs`): ícone, menu nativo com Nova nota, lista de notas pela primeira linha (marca
  "(oculta)"), Ocultar/Mostrar todas (transitório) e Sair; recria o ícone em `TaskbarCreated`.
- Fechar a janela (Alt+F4) oculta a nota; não exclui nem encerra o aplicativo.

## Testes automatizados (56 em `pod-app`, 12 em `pod-model`)

Novos: Quick Notes (digitação, texto idêntico, só espaços não é conteúdo, limite em bytes e
multibyte, recusa sem truncar, erro some na edição válida, restauração valida versão e tamanho,
eventos de outros campos e após `stop`), normalização de quebras de linha, rótulo da bandeja.

## Verificação em Windows real (automação de mouse/teclado)

| Cenário | Resultado |
| --- | --- |
| Digitar `Olá, nota ção`, Enter, emoji | gravado em ~1,5 s com LF (sem CR); rodapé "Salvando…" e depois "Salvo" |
| Emoji e símbolos **digitados** (😀 ❤) | renderizam (monocromáticos) |
| Ctrl+Z | desfaz, mas ver "Pendências": deixou meia metade de um emoji |
| Colar (Ctrl+V) com `✓` e japonês | entra no modelo e no disco |
| Esc, depois digitar | texto inalterado |
| Colar 200 000 × `é` (400 000 bytes) | recusado: texto anterior volta, mensagem de limite visível, rodapé "Salvo", processo vivo |
| Excluir nota com conteúdo | diálogo; "Não" mantém (janela e arquivo), "Sim" remove janela e instância |
| Dez notas, texto próprio em cada | 10 IDs distintos, cada nota só com o próprio texto |
| Matar o processo e reabrir | 10 janelas, textos idênticos, geração inalterada (nada regravado ao abrir) |
| Fechar a janela | nota oculta, processo vivo, `visible=false` gravado |
| Ctrl+Shift+A (mostrar todas) | nota volta, `visible=true` gravado |
| UI Automation `SetValue` | texto entra no editor, no modelo e no disco |
| `Shell_NotifyIconW(NIM_ADD)` | aceito pelo Windows |

## Pendências e achados (não fingir cobertura)

- **Bandeja não verificada em uso.** O Windows aceitou o ícone, mas não consegui abrir o menu por
  automação (a barra de tarefas do Windows 11 não expôs elementos ao UI Automation), então
  Nova nota, lista, Ocultar/Mostrar todas e Sair pela bandeja **não foram exercitados**.
- **Emoji e símbolos restaurados do disco aparecem como caixas** (o texto gravado está intacto).
  Testado: `EM_SETTEXTEX`, `EM_REPLACESEL` e digitar por `WM_CHAR`. Só a digitação real do teclado
  produz o glifo. Uma nota com emoji reaberta mostra caixas até o emoji ser redigitado.
  A tentativa do M1 de forçar `Segoe UI Symbol`/`Segoe UI Emoji` por trecho (`sync_fonts`) piorava a
  digitação e foi removida.
- **Desfazer pode deixar meio emoji.** Com emoji digitado como duas unidades (par substituto),
  Ctrl+Z removeu só a segunda; o texto ficou com uma metade órfã, que vira U+FFFD no modelo. Não
  verificado com teclado/painel de emoji reais, que talvez entreguem o par de forma diferente.
- **IME real e Narrator não testados** (sem IME japonês instalado; Narrator não executado). A
  retenção da entrega durante composição é lógica sem prova.
- **Japonês digitado por SendInput** aparece como caixas; só o texto inserido em bloco renderiza CJK.
- Teste de clipboard cobre texto simples; não foi testado colar conteúdo rico/ativo (o editor está
  em modo texto simples, sem OLE; falta prova com clipboard HTML/RTF).
- Ctrl+Shift+S é interceptado por outro programa desta máquina; o atalho de teste "mostrar todas"
  passou a ser Ctrl+Shift+A. Os atalhos Ctrl+Shift+N/W/H/A são auxiliares de desenvolvimento.
- A política "discreta" de camadas (reposicionar atrás das janelas ao sair da edição) **não foi
  implementada**: uma nota clicada fica acima até outra janela a cobrir, e um aplicativo maximizado
  na frente esconde notas recém-criadas (visto nos testes). Alt+Tab e Win+D não foram verificados.
- Exclusão por Delete não existe como atalho de teclado da nota além de Ctrl+Shift+W e do menu.
- DPI misto, vários monitores, device loss e reinício do Explorer seguem para o M5.
- Medições (≈1,5 s de gravação) são de uma máquina, debug, sem repetição; não são metas calibradas.
- Durante a verificação da bandeja, uma varredura de UI Automation sobre a árvore inteira do desktop
  alcançou elementos do aplicativo Claude e pode ter acionado o botão "Mostrar/ocultar árvore de
  arquivos" dele (a chamada foi interrompida; não confirmei o efeito). Foi um erro do script de
  teste, corrigido: a busca agora se limita à janela da barra de tarefas.
