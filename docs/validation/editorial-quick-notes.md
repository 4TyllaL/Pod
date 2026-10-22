# Quick Notes — implementação Editorial

**Data:** 20/10/2026. Implementação no renderer nativo existente, sem WebView ou dependências novas.

## Mudanças

- Material Papel no claro e grafite no escuro, sem a faixa tingida, separador ou ponto colorido do cabeçalho anterior.
- Rótulo discreto `NOTA`, menu de reticências com alvo de 28×28 DIP e raio de 20 DIP em notas maiores; 14 DIP nas pequenas.
- Geometria única em `ui/note.rs`, compartilhada por pintura, editor, hit testing de menu, região de janela e bounds usados pelo UI Automation.
- Fonte por dimensão: 28 DIP em notas com pelo menos 360×320 DIP; 22 DIP a partir de 288×200; 16 DIP nas compactas. Padding lateral de 28/24/16 DIP, respectivamente. Quantidade de texto não altera a fonte.
- Entrelinha proporcional nativa de 1,2 linhas. Os valores absolutos de entrelinha do estudo são referências ópticas; não uma altura fixa imposta aos glifos.
- Placeholder desenhado `Anote alguma coisa.`, sem inserir esse texto no estado ou no editor.
- Região de status reservada e estável: o salvamento não desloca o editor. Estados e mensagens continuam ligados ao mecanismo existente de gravação/ack; não foi adicionado timer para esconder `Salvo`.
- Material de Notes separado do tema dos cartões/galeria; cores do sistema de alto contraste preservadas e vidro continua opt-in. Texto secundário reforçado no vidro claro para manter contraste.
- Conteúdo, schema, posição, dimensões iniciais e preferências persistidas não foram migrados ou sobrescritos pelo refinamento.

## Ajuste de integração com Rich Edit

A criação usa `PARAFORMAT2` com espaçamento proporcional (`bLineSpacingRule = 5`, `dyLineSpacing = 24`). O tamanho é expresso em twips: DIP × 15.

O teste nativo encontrou que `EM_SETPARAFORMAT` devolvia zero ao tentar mudar a entrelinha absoluta com conteúdo no controle de texto simples. A implementação final mantém proporção estável desde a criação; o espaçamento acompanha as métricas da fonte durante resize. Não converte o editor para rich text e não substitui o buffer para obter a aparência desejada. Referências: [PARAFORMAT2](https://learn.microsoft.com/en-us/windows/win32/api/richedit/ns-richedit-paraformat2), [EM_SETPARAFORMAT](https://learn.microsoft.com/en-us/windows/win32/controls/em-setparaformat).

Resize/troca de escala alteram somente `CFM_SIZE`, sem reaplicar `CFM_FACE` a todos os runs. Isso evita eliminar escolhas de fallback. Durante composição IME, adiar mudança de tamanho até o fim da composição. Não limpar undo nem restaurar o texto a cada frame.

## Verificações executadas

| Verificação | Resultado |
| --- | --- |
| Suíte completa, fora do sandbox, serial | 289 testes do app + 175 do model aprovados; 6 testes manuais ignorados |
| Regressão gráfica dos demais widgets | Hashes existentes aprovados na suíte completa; golden files não foram atualizados |
| Verificação após os ajustes de entrelinha | 288 testes do app + 175 do model aprovados; 6 ignorados; galeria gráfica omitida nessa repetição por já ter passado e não ter sido alterada |
| Verificação final focada após atualização de métricas de DPI | Teste nativo de texto e teste de contraste Editorial aprovados |
| Build de distribuição | `cargo build --release --offline` concluído; executável em `target/release/pod-app.exe` |
| Layout | Nota mínima 160×120, compacta 188×188, padrão 320×240 e maiores mantêm editor/status/menu separados, inclusive com mensagem de limite de texto |
| Editor real, em HWND oculto de teste | Inserção Unicode, seleção, resize e mudança de tema preservam texto; a mudança visual não acrescenta ação ao undo nem limpa o histórico |
| Formato nativo consultado | Fonte compacta em 240 twips e espaçamento proporcional 24/20 confirmados por mensagens Rich Edit |
| Contraste | Texto principal ≥7:1, secundário ≥4,5:1 e acentos de foco ≥3:1, nos materiais claro/escuro e vidro sobre fundos preto/branco; alto contraste permanece idêntico aos tokens do sistema |

A primeira suíte no sandbox teve falhas de acesso negado em testes de armazenamento e Credential Manager, além de acesso restrito à mídia. Fora do sandbox a suíte passou. Warnings preexistentes de código não usado e nomes de constantes continuam presentes; não foram suprimidos para aparentar build limpo.

## Limites da validação

- A primeira tentativa de conferência interativa ficou inconclusiva. A continuação abaixo registra o que foi possível comprovar posteriormente; não equivale a aprovação em todos os temas e escalas.
- Não foram alteradas configurações de tema/DPI do Windows nem notas do perfil real. As instâncias de demonstração iniciadas para a revisão foram encerradas; fixtures ficam em `target/`.
- O teste nativo usa parâmetros de DPI do host, não uma movimentação física entre monitores com DPI misto.
- IME real, Narrator, aparência em 150/200%, toque e scroll prolongado ainda exigem revisão ao vivo desta nova composição. A preservação da string Unicode não comprova a renderização de todos os glifos; limitações antigas de emoji não foram declaradas resolvidas.
- Nenhuma medição nova de CPU/GPU ou latência foi feita. Não foram adicionados loops de animação, captura de wallpaper ou serviços de dados.

## Continuação — revisão nativa e acessibilidade

### O que foi observado ao vivo

Na instância isolada em `target/editorial-live-847feac6`, o modo de revisão tornou as notas selecionáveis pelo Computer Use. A inspeção exigiu ativar a janela antes da captura: sem ativação, o helper apresentou conteúdo de outra janela sobreposta. Somente as capturas posteriores à ativação foram usadas para avaliar a nota.

Foram observados, no tema escuro a 100%:

- Superfície grafite, cabeçalho `NOTA`, menu integrado, cantos arredondados e texto Editorial de uma nota grande.
- Digitação real de `Café, ação e revisão.`, wrap da linha e transição para `Salvando…`.
- Ctrl+Z removendo exatamente o trecho digitado no teste.
- Restauração do conteúdo após reinício, incluindo uma edição feita durante a sessão de revisão.
- Uma segunda geometria de 388×259 DIP restaurada, usando a escala intermediária de fonte. Isso não foi apresentado como prova de arraste automatizado: a dimensão foi alterada durante a interação com a sessão.

Atalhos posteriores foram interrompidos pelo helper ao detectar entrada do usuário. Não foram repetidos às cegas nem contabilizados como testes aprovados. Não foi feita troca de tema do Windows.

### Correções e testes adicionais

- `IRawElementProviderFragment::SetFocus` do editor e do menu agora seleciona também o filho lógico, em vez de apenas focar o HWND compartilhado.
- Mudanças de foco lógico e ativação da nota notificam clientes UIA inscritos; `GetFocus` não informa um filho de uma janela que não possui foco.
- O helper ainda informou a janela raiz em sua consulta global de foco após a correção. Portanto, notificações e delegação implementadas não foram tratadas como prova de que Narrator/foco global estejam resolvidos. `TextPattern` continua pendente.
- Um teste novo usa Rich Edit real com 100 linhas: move o caret ao fim, confirma deslocamento de scroll positivo, muda de tamanho compacto para grande e verifica que texto e seleção continuam intactos.

Verificação final desta continuação: **290 testes do app + 175 do model aprovados**, 6 testes manuais ignorados; a galeria gráfica já aprovada foi omitida nessa repetição. Log local: `target/editorial-followup-suite.log`.

Release atualizada com `cargo build --release --offline`; log em `target/editorial-followup-release.log`. O build concluiu com os warnings já descritos, sem novos erros de compilação.

### Modo de revisão reproduzível

Em um build de desenvolvimento:

```powershell
cargo run -- --review-notes --data-dir "C:\caminho\para\workspace-de-teste"
```

Usar um diretório separado, com um `workspace.json` de demonstração válido. O argumento só é habilitado quando `--data-dir` está explícito e não vazio; há teste dessa proteção. O modo expõe janelas normais na barra de tarefas e libera sua elevação durante a revisão. A pintura, editor e geometria são os mesmos do aplicativo. Isso não valida a política de z-order do desktop.

Na release o argumento não altera os estilos nem a política de janelas. Não é necessário habilitar o modo para usar o produto.

IME real, Narrator, aparência clara/alto contraste e 150/200%, DPI misto, toque e interação prolongada com scroll continuam pendentes nesta composição. O novo teste de scroll comprova o caminho do serviço nativo; não substitui essas verificações de interface.
