# M1 — Rich Edit windowless: fallback de fonte (CJK/emoji)

Ambiente: Windows 11, build de debug, 100% DPI, `Segoe UI` 11 pt, `TXTBIT_D2DDWRITE`.
Método: capturas de tela com texto inicial e texto digitado por `SendInput` (VK_PACKET).

| Caso | Resultado |
| --- | --- |
| PT-BR com acentos, digitado | OK |
| Japonês/coreano definido por código | OK (após `EM_SETLANGOPTIONS` + tipografia avançada + `EM_SETTEXTEX`) |
| Japonês digitado via `SendInput` | Caixas (fonte do teclado ativo, não de IME real) |
| Emoji/símbolos (😀 ★ ❤ 👍) definidos por código | **Caixas** |
| Emoji digitado via `SendInput` | Renderiza (monocromático) |

## Achados

- O Rich Edit atribui `Segoe UI Symbol` ao emoji **digitado** e renderiza. Para texto definido por
  código a face fica `Segoe UI` e o glifo vira caixa.
- `sync_fonts` aplica `Segoe UI Symbol` aos trechos (confirmado por `EM_GETCHARFORMAT`: a face muda),
  mas o desenho continua com caixas. Aplicar a face não basta; algo mais que o caminho de digitação
  define não foi identificado.
- Tentativas sem efeito sobre emoji definido por código: `IMF_AUTOFONT|DUALFONT|AUTOFONTSIZEADJUST`,
  `TO_ADVANCEDTYPOGRAPHY`, `EM_REPLACESEL`, `Segoe UI Emoji` por trecho, `Segoe UI Symbol` por trecho,
  `TXTBIT_CHARFORMATCHANGE` após formatar.
- Forçar a fonte base em todo o texto (`SCF_ALL`) piorou: o emoji digitado também virou caixa.

## Não testado

IME real (japonês), Narrator/UI Automation, clipboard, undo, DPI misto.

## Próximos passos possíveis

Inspecionar o `CHARFORMAT2` completo do emoji digitado (efeitos, idioma, `CFM_*` extras) e
reproduzi-lo; ou aceitar a limitação até o teste com IME real; ou, se o gate falhar, registrar ADR
com o editor HWND convencional (ARCHITECTURE.md §7).
