# M1 — Acessibilidade (UI Automation) e IME

Ambiente: Windows 11, build de debug. Idiomas instalados: en-US, pt-BR (ABNT2) e europeus;
**nenhum IME japonês/chinês/coreano**.

## Acessibilidade

| Etapa | Resultado |
| --- | --- |
| Antes | Janela expunha só um `Pane` "Pod", 0 descendentes UIA: o editor era invisível ao Narrator |
| Provedor UIA (`a11y.rs`) | Cliente UIA encontra `Edit` "Nota" (`pod.quick-notes.editor`), focalizável, retângulo correto |
| `ValuePattern` | Leitura devolve o texto Unicode completo; `SetValue` altera o editor |

Medido com cliente UIA (`System.Windows.Automation`), não com o Narrator.

**Pendente:** `TextPattern` (seleção, caret, leitura por linha/palavra); eventos
(`PropertyChanged` de valor, `AutomationFocusChanged`); botões e menu da nota quando existirem;
teste com Narrator real, que é o critério de aceitação do M1.

## IME

**Não testado.** Sem IME japonês instalado, a matriz exigida (candidato na posição correta,
composição, Esc durante composição, DPI misto) não pôde ser exercitada. O encaminhamento de
`WM_IME_*` ao serviço de texto e `TxImmGetContext` estão implementados, mas sem prova. Instalar um
idioma é uma alteração de configuração do Windows e fica a cargo do dono da máquina.

## Pendências registradas

- Emoji/símbolos definidos por código renderizam como caixas (ver `m1-text-fallback.md`).
- IME real, Narrator, clipboard Unicode, undo/redo, DPI misto, Alt+Tab e Win+D.
