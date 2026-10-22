# ADR-0001 — Motor de widgets v2 (widgets de tela, não só notas)

**Status:** **aceita** em 18 de outubro de 2026 (ver "Decisões do dono" no fim da seção 11).
**Data:** 18 de outubro de 2026.
**Substitui parcialmente:** `ARCHITECTURE.md` §7 (contrato mínimo de view) e §17 (extensibilidade). Nada
aqui altera M1–M7 já entregues: Quick Notes continua funcionando como está.

## 1. Contexto

O MVP entregou um widget (Quick Notes) sobre um contrato deliberadamente pequeno: a view tem superfície,
rótulo, botão e campo de texto; o widget recebe eventos tipados e devolve estado e efeitos; só tem um
tamanho livre. Isso provou o host, a persistência, a plataforma e a acessibilidade.

O produto pretendido é outro: **widgets de tela** como os das referências enviadas (cartões arredondados
em grade, tamanhos pequeno/médio/grande, números grandes, anéis, gráficos, capas de álbum) e como os 37 do
Dockn't, projeto do mesmo autor. Lido diretamente do Dockn't (`WIDGETS.md`, `widgets.rs`, `widget_art.rs`):

- Cada widget tem **1 a 4 larguras** com layout próprio por largura (a 1 largura é um glifo com um valor;
  nas maiores, um valor grande, legenda e dados secundários ou um botão).
- Há **valores vivos** (CPU, memória, rede, bateria, mídia, notificações, relógio) amostrados por um timer
  único que só roda se algum widget precisar, e **animações** que redesenham só a própria faixa (ticker de
  notícias, lembrete de água, cenas do dia).
- Há **interação rica**: clique, botão direito, rolagem, arrastar arquivos, atalhos de teclado e 17 painéis
  de edição (notas, lembretes, calendário, conversor, ajustes de água, Pomodoro...).
- Há **serviços do SO** (mídia, notificações, área de transferência, pastas conhecidas) e **rede só por opt-in**
  (clima, notícias, GitHub, chat de IA com chave do usuário no Credential Manager).
- O estado é **local** (`.toml` por widget, alguns compartilhados, como eventos entre Calendar e Countdown).

O que **não** serve de modelo é a estrutura do Dockn't: uma única `struct Widget` com os campos de todos
os widgets, um `enum Page` de 41 variantes, um `enum Click` com ações do SO, painéis desenhados à mão (cerca
de 4,5 mil linhas) e estado global em `static`. Isso contradiz as regras do Pod (sem singleton mutável, sem
barramento global, widget sem APIs do Windows, ownership claro). Reaproveita-se o **catálogo e a lógica**,
não a forma.

### Lacunas do contrato v1 diante desse catálogo

| Lacuna | Exemplo que não cabe hoje |
| --- | --- |
| Primitivas de view | anel (Time progress, Activity), barra, gráfico de 1 min (Network), pontos (Pomodoro), segmentos (água), ícone, imagem (capa do Music), chip, lista |
| Tamanhos | só livre; falta a noção de "este widget tem estes formatos" e um layout por formato |
| Interação | só `Clicked` de botão; faltam botão secundário, roda, soltar arquivos, menu de contexto, teclado |
| Painéis e ajustes | nenhum mecanismo; cada ajuste (meta de água, minutos do Pomodoro, cidade) viraria UI própria |
| Serviços de dados | `Event` só tem tempo; nada de métricas, mídia, notificações, pastas, rede |
| Dados compartilhados | Calendar e Countdown usam os mesmos eventos |
| Animação | sem pedir quadros em faixa nem declarar transição |
| Acessibilidade | só o editor e o botão de menu têm provedor UIA escrito à mão |
| Visualização offline | não há como renderizar todos os widgets em todos os tamanhos para revisar/regredir |

## 2. Decisões

### D1. Widgets continuam internos, em Rust, sem runtime de plugin

Mesma posição do ARCHITECTURE §11 e §17: código confiável no processo, contrato evolui, **nenhum SDK
público**. A tabela de capacidades (D6) é escrita já como o futuro protocolo de um broker, mas só vale como
checagem interna agora.

### D2. Tamanhos: formatos em grade declarados pelo widget

Uma célula de grade `U = 88 DIP` com espaço de `12 DIP` entre células (valores ajustáveis, ver seção 11).
Um **formato** é `(colunas, linhas)`; largura = `c·U + (c−1)·12`. Formatos permitidos:
`1×1, 2×1, 3×1, 4×1, 2×2, 4×2, 4×4` (as quatro larguras do Dockn't viram `1×1 … 4×1`, o que permite portar
um layout por largura; `2×2`/`4×2` atendem cartões ricos como as referências).

- O `Descriptor` ganha `shapes: &[Shape]` (os formatos que o widget sabe desenhar), `default_shape` e
  `resize: Fixed | Free { min, max }`. Quick Notes é `Free` (mantém o redimensionamento de hoje) e ganha os
  formatos como atalhos; relógio, Timer, Activity etc. são `Fixed`.
- A janela tem **exatamente** o tamanho do formato escolhido no DPI do monitor (arredondado só na fronteira
  do Win32, como em §10). Trocar de formato é um comando do menu/painel; o preferido grava o formato, não
  pixels.
- A view é função de `(estado, ViewCtx { shape, size_dip, theme, locale })`: o widget escolhe o layout por
  formato (como o Dockn't faz por largura), em vez de o host escalar um layout único.
- **Snapping em grade entre widgets** e **modo de edição** (os "+/−" das referências) ficam para W6; D2 só
  garante que o modelo já os comporta.

### D3. Primitivas de view: conjunto fechado, enumerado, sem DOM nem desenho livre

O widget continua devolvendo **dados**, nunca comandos de GPU. `Element` passa a incluir, e só estes:

- **Layout:** `Stack { axis, gap, align, children }` com filhos `Fixed(dip) | Fill(peso) | Auto`; `Spacer`;
  `Overlay`; `Plate { fill: Plain | Accent | Identity(rgb) | Image(ImageId), radius }` (a "placa" colorida
  do Dockn't).
- **Conteúdo:** `Text { style: Display | Title | Body | Caption | Mono, color: Role, align, max_lines,
  overflow: Clip | Ellipsis | Marquee }`; `Icon { glyph: IconId, size, color: Role }` (Segoe Fluent com
  fallback Segoe MDL2, e emoji); `Image { id }` (bytes entregues por serviço, limitados e cacheados pelo host).
- **Dados:** `Ring { value, label }`, `Bar { value }`, `Chart { series: ≤120 pontos, range }`,
  `Dots { filled, total }`, `Segments { filled, total }`, `Badge`, `Divider`.
- **Controles:** `Button { id, content, kind }`, `IconButton`, `Toggle/Checkbox`, `Slider`, `TextField`
  (existe), `List { items, selectable }`.
- **Visuais do host:** `Visual { kind: VisualId, data }` para os poucos casos de arte de identidade
  (céu do Weather, cenas do dia, sprites do mascote). A lista é fechada e cada item é implementado no host;
  nada de callback de desenho.
- **Semântica:** todo elemento pode levar `a11y { nome, papel, valor }`. **O provedor UIA passa a ser gerado
  da árvore da view** (hoje é escrito à mão só para o editor e o botão de menu), o que dá acessibilidade
  básica a todo widget novo sem trabalho extra.
- **Cores por papel:** `Role = Surface | Text | TextSecondary | Accent | Positive | Warning | Danger`.
  O tema (claro/escuro/alto contraste, já testado por contraste WCAG no M6) resolve os papéis. Cores de
  identidade (`Identity(rgb)`, capa de álbum, céu, o amarelo da nota) e semânticas (laranja do timer, verde/
  vermelho de estado) não são recoloridas pelo tema, como no Dockn't; alertas não dependem só de cor.
- **Limites validados pelo host:** ≤256 elementos, texto ≤4 KiB por elemento, séries ≤120 pontos, imagens ≤
  512 KiB cada e ≤4 por widget; excedente é recusado com erro visível, não truncado em silêncio.
- **Regra de crescimento:** uma primitiva nova só entra com dois widgets que precisem dela, e com teste de
  layout e de acessibilidade.

*Alternativas rejeitadas:* (a) o widget desenha num `Canvas` D2D como no Dockn't (`widget_art.rs`, 5,3 mil
linhas): acopla ao renderer, impede testar sem GPU, impede gerar acessibilidade e entrega GPU ao widget,
contra §7; (b) HTML/WebView/CSS: já rejeitado em §6; (c) uma árvore de cena genérica estilo Skia: é um
framework de UI amplo, que §6 também rejeita.

### D4. Interação: eventos tipados e menu declarado

`Event` ganha (todos com o `LocalId` do elemento alvo): `Click { target, button: Primary | Secondary, mods }`,
`Wheel { target, delta }`, `Key` (só com foco), `DropFiles { target, tokens }` (capacidade, ver D6),
`MenuCommand { id }` e `PanelClosed`. O **menu de contexto é declarado na view** (`menu: Vec<MenuItem>`); o
host mostra o menu nativo e devolve `MenuCommand`. Substitui o `enum Click` do Dockn't (ações do SO) por
**efeitos** autorizados (D6), de modo que o widget pede e o host executa.

### D5. Painéis e ajustes: mesma árvore de view, não UI própria

- **Painel** = segunda view do mesmo widget, aberta como popup (mesmo window host, mesmo renderer, mesmo
  Rich Edit), fecha ao perder o foco ou com Esc. `Effect::OpenPanel` / `ClosePanel`; eventos do painel chegam
  ao mesmo `handle`. Isso elimina os 17 painéis desenhados à mão: lembretes, eventos, conversor etc. são
  `List` + `TextField` + `Button` + `Toggle`.
- **Ajustes por instância:** `Descriptor.settings: &[SettingSpec]` (`Enum`, `IntRange`, `Bool`, `Text`,
  `Choice`) renderizados por um **painel padrão de ajustes** (meta de água, minutos do Pomodoro, estilo do
  Activity, cidade do Weather, duração do Timer). Gravados em `settings` da instância (JSON versionado,
  validado pelo próprio widget, com migração por tipo).

### D6. Serviços de dados e capacidades

O widget **nunca** chama a API do Windows. Ele declara no `Descriptor` o que precisa
(`needs: &[Capability]`), o host liga os provedores correspondentes **só enquanto houver assinante** e entrega
`Event::Data(ServiceData)` (enums definidos em `pod-model`, puros). O que não foi declarado dá
`Denied`. A tela de adicionar widget mostra as capacidades em linguagem simples.

| Capacidade | Provedor (host) | Cadência / custo | Origem no Dockn't |
| --- | --- | --- | --- |
| `Time` | agendador existente; evento alinhado ao minuto/segundo | 0 quadros entre mudanças | Clock, Timer, Calendar... |
| `SystemMetrics` | `GetSystemTimes`, `GlobalMemoryStatusEx`, `GetIfTable2`, `GetSystemPowerStatus` | 2 s, só com assinante | Activity, Network, Battery |
| `KnownFolders` | contagem e nomes recentes de Downloads/Capturas (sem caminhos livres) | ao mudar a pasta | Files, Screenshots |
| `MediaSession` | GSMTC (sessão de mídia do Windows), com capa e comandos | por evento do sistema | Music |
| `Notifications` | `UserNotificationListener`, com a permissão do Windows | por evento | Notifications |
| `Clipboard` | prévia do último item; conteúdo marcado como privado não é lido | por evento | Clipboard |
| `Net(endpoint)` | corretor HTTPS: o **host** monta a URL a partir de um modelo registrado (Open-Meteo, RSS, ...), com prazo, limite de tamanho, política de redirecionamento e cache | só com "widgets de internet" ligado, desligado por padrão | Weather, News |
| `Secret(name)` | Credential Manager: o widget nunca vê a chave | — | AI chat |
| `ReadFiles(allowlist)` | leitura somente de caminhos permitidos explicitamente | 1/min, incremental | Claude usage |
| `Shared(name)` | coleções compartilhadas entre widgets (D7) | — | eventos, tarefas |
| Efeitos privilegiados | `OpenUri` (lista de `ms-settings:`), `CopyText`, `Media(cmd)`, atalho de captura | por pedido, com consentimento por widget | Screenshots, Color picker, Emoji |

Regras: provedores rodam na thread de UI quando baratos e em **workers com prazo, cancelamento e limite de
concorrência** quando não (§9); resultados voltam por fila e carregam geração da instância (resposta velha é
descartada); nada de `static` mutável; pausa/retorno da suspensão e tela cheia seguem M5.

### D7. Dados compartilhados e persistência (schema 3)

Em vez de um arquivo `.toml` por widget, o workspace ganha `shared: { nome: json }` e cada instância ganha
`shape` e `settings`. **Um só caminho de persistência**, com a garantia do M3 (gravação atômica, geração,
ack, recuperação, migração em cópia com o original preservado). Calendar e Countdown declaram
`Capability::Shared("events")`. Tipos desconhecidos continuam voltando intactos ao disco.

### D8. Desempenho: quadros só quando a view muda

- O runtime **compara a view nova com a última**; sem diferença, não invalida (protege contra widgets que
  pedem redesenho à toa e preserva a meta de "0 quadros em repouso" do M6).
- `Effect::Frames(class)` com `Static | Minute | Second | Strip30`; só `Strip30` (marquee, animação curta) usa
  timer de ~30 Hz, e para com a janela oculta, a tela cheia ou a suspensão.
- Redesenho parcial (retângulos sujos no `Present1`) fica como otimização medida em W1/W3, não como premissa.
- **Portão de escala:** com 40 janelas de widgets (uma HWND + uma swap chain cada) medir CPU, memória e GPU. Se
  falhar, reavaliar a alternativa "um host por monitor" já descrita em §5 *antes* de seguir.

### D9. Galeria e testes visuais

Um modo **offscreen** renderiza todo widget em todo formato com **dados de demonstração fixos**
(`Registration.demo`), como o `widget_gallery` do Dockn't. Serve a três coisas: revisão visual, documentação
e **teste de regressão** (comparação com imagens de referência com tolerância, por tema). A tela
"Adicionar widget" do Pod usa a mesma renderização. Modo de edição com "+/−" e arrastar entra em W6.

### D10. Vidro e translucidez

Continua pós-MVP (§6). Esta decisão só garante que o tema comporta um material "vidro" por papel e que
`Plate` já separa superfície de conteúdo. Experimento de backdrop do DWM (Windows 11 build ≥ 22621, com
fallback opaco) vira outro ADR, só depois de cumprir os orçamentos de D8.

### D11. Isolamento de estado e organização do código

Cada widget é **um módulo com a sua própria struct** (`widgets/<nome>/`), a sua factory e os seus testes;
proibido um tipo central com campos de vários widgets ou um enum de "páginas". Lógica pura (conversão de
unidades, fases do Pomodoro, progresso do tempo) vive no módulo do widget, sem Win32, e é testada com relógio
injetado. Provedores ficam em `platform/`, atrás de traits estreitas definidas no `pod-model`.

## 3. Consequências

**Ganhos:** acessibilidade e tema "de graça" para widgets novos; um único mecanismo de painel/ajuste; testes
de layout e de regressão visual sem GPU interativa; capacidades auditáveis; o visual das referências sem
desenho livre.

**Custos:** o motor de layout (Stack/Fill) e as primitivas de dados são trabalho real antes do primeiro
widget novo; o conjunto fechado limita arte muito específica (resolvido pelos `Visual` do host, que são
caros por unidade); o contrato muda e exige schema 3 e migração.

**Riscos:** (1) o contrato crescer sem freio, virando um framework (mitigação: regra de duas necessidades e
limites do D3); (2) a escala de janelas (D8, portão); (3) acessibilidade de gráficos (mitigação: `a11y.valor`
textual obrigatório em `Ring`/`Chart`); (4) relógio de parede vs monotônico em Timer/Pomodoro/Countdown
sobrevivendo à suspensão (o evento de tempo passa a levar os dois); (5) notificação de fim de timer sem
identidade de pacote no Windows (usar balão da bandeja e destaque do cartão; toast só se houver identidade);
(6) idioma: o Dockn't é em inglês, o Pod é pt-BR (tabela de textos simples por idioma em W2); (7)
licença/reuso do código do Dockn't (seção 11).

## 4. Catálogo mapeado

Do catálogo do Dockn't (37 itens, mais o Wallpaper engine das predefinições). "Marco" é o primeiro marco em
que o widget pode existir (seção 5). **Fora** = não faz sentido no Pod ou exige decisão sua.

| Widget | Capacidades / efeitos | Primitivas novas | Marco | Observações |
| --- | --- | --- | --- | --- |
| Clock | `Time` | `Text Display`, `Icon`/`Visual` (relógio analógico) | W1 | valida o motor com custo mínimo (§17 já o previa) |
| Time progress | `Time` | `Ring`, `Bar` | W1 | lógica pura |
| Quick Notes / Note | — | formatos, `Plate` amarela | W1 | a "Note" do Dockn't é um formato/pele do Quick Notes |
| Timer | `Time`, `OpenUri`? | `Ring` de contorno, `IconButton` | W2 | relógio de parede + monotônico |
| Stopwatch | `Time` | `List` de voltas | W2 | clique/direito, voltas |
| Pomodoro | `Time` | `Dots`, ajustes | W2 | fases e rodadas, ajustes por painel padrão |
| Countdown | `Time`, `Shared(events)` | `Chip` | W2 | datas relativas |
| Calendar | `Time`, `Shared(events)` | `Badge` de data | W2 | editor de eventos = painel |
| Reminders | `Shared(tasks)` | `Toggle`, `List` | W2 | painel de lista |
| Drink water | `Time`, ajustes | `Segments`, `Bar` | W2 | lembrete com marquee depende de W3 |
| Converter | — | `TextField` | W2 | lógica pura de unidades |
| Favorites | `OpenUri` (sites salvos) | `Chip` | W2 | links do usuário; revisar política de URLs |
| Parcels | — | `List` | W2 | só armazena códigos (sem consulta); valor baixo |
| Activity / CPU / Memory | `SystemMetrics` | `Ring`, `Chart` | W3 | estilos Auto/Rings/Graph/Numbers = ajuste |
| Network | `SystemMetrics` | `Chart` | W3 | |
| Battery | `SystemMetrics` | `Bar`, `Icon` | W3 | até 2 formatos |
| Files (Downloads) | `KnownFolders` | `List` curta | W3 | sem pilha de dock; só contagem/nomes |
| Screenshots | `KnownFolders`, atalho de captura | `List` | W3 | |
| Clipboard | `Clipboard`, `OpenUri` | — | W3 | respeitar conteúdo privado |
| Emoji | efeito privilegiado | — | W3+ | digitar no app em foco é sensível; talvez só abrir `Win + .` |
| Color picker | captura de tela (privilegiado) | `Plate` | W3+ | revisão de privacidade |
| Claude usage | `ReadFiles(~/.claude/projects)` | `Bar`, `Ring` | W4 | leitura local incremental; sem rede |
| Music | `MediaSession` | `Image`, `Plate(Image)`, `IconButton` | W4 | capa como fundo escurecido |
| Notifications | `Notifications` | `List` | W4 | permissão do Windows; privacidade |
| Weather | `Net`, ajustes (cidade) | `Visual(céu)`, `Icon` | W5 | Open-Meteo, opt-in |
| News | `Net` | `Text Marquee` | W5 | ticker em `Strip30`, 15 min |
| GitHub | `Net`/processo `gh` | grade de contribuições (`Chart`/`Visual`) | W5 | processo externo: revisar |
| AI chat | `Net`, `Secret` | `TextField`, `List` | W5 | por último; chave nunca vista pelo widget |
| Daily scenes | `Time` | `Visual(cena)`, `Marquee` | W6 | arte do host; textos do usuário |
| Mascot | `SystemMetrics` | `Visual(sprites)` | W6 | ativos da !StayAlone (licença MIT dos ativos) |
| Wallpaper engine | — | — | Fora | depende do IsNotAEngine; outro produto |
| App collections, Shortcut, Shelf, Nearby sharing | — | — | Fora | são itens de dock; sem sentido numa nota solta |
| System status | — | — | Fora | é o canto da barra de tarefas |
| Fans | — | — | Fora | "sem sensor" no próprio Dockn't |
| Keyboard lock | gancho de entrada | — | Fora | risco alto (bloqueia entrada); decidir depois |
| iFood | `OpenUri` | `Chip` | decidir | específico de um serviço |

## 5. Marcos

Cada marco segue as regras do projeto: um por vez, testes automatizados de regra, verificação em Windows real
sem substituir IME/Narrator/GPU por mocks, e um documento em `docs/validation/`.

### W0 — Decisões
Aceitar este ADR (ou ajustá-lo) e responder a seção 11. Sem código.

### W1 — Primitivas, formatos e galeria offscreen
**Entrega:** `Element` ampliado e validado; motor de layout `Stack`/`Fill`; estilos de texto; `Ring`, `Bar`,
`Icon`, `Plate`; `Shape` no `Descriptor` e na janela; papéis de cor; provedor UIA gerado da view; diff de view
no runtime; renderização offscreen com dados de demonstração; widgets **Clock** e **Time progress**.
**Portões:** layout testado sem Win32 em todos os formatos; Clock em 3 formatos com acessibilidade legível pelo
UIA; 0 quadros entre mudanças; **40 janelas de relógio** cumprem os orçamentos do M6 (D8); regressão visual
estável nos três temas; Quick Notes sem regressão.

### W2 — Interação, ajustes, painéis e dados compartilhados
**Entrega:** eventos de D4, menu declarado, painel e painel padrão de ajustes, schema 3 (formato, ajustes,
`shared`) com migração, catálogo de textos pt-BR/en; **Timer, Stopwatch, Pomodoro, Countdown, Calendar,
Reminders, Drink water, Converter**.
**Portões:** testes de lógica com relógio injetado (incluindo suspensão de horas); persistência e recuperação
cobrindo as coleções compartilhadas; painel acessível por teclado; nenhum painel com UI própria.

### W3 — Serviços locais e métricas
**Entrega:** provedores `SystemMetrics`, `KnownFolders`, `Clipboard`; `Chart`; **Activity/CPU/Memory,
Network, Battery, Files, Screenshots, Clipboard**.
**Portões:** provedores desligados sem assinante (verificado por contador); custo em repouso medido com 10
widgets de métricas; `Strip30` só em animação ativa.

### W4 — Mídia, notificações e leitura local
**Entrega:** `MediaSession` (com imagens), `Notifications`, `ReadFiles`; **Music, Notifications, Claude usage**.
**Portões:** permissão do Windows tratada; conteúdo privado respeitado; limites de imagem; testes com mídia real.

### W5 — Rede por opt-in
**Entrega:** corretor HTTPS com modelos de endpoint, prazos, limites e cache; `Secret`; **Weather, News,
GitHub, AI chat**.
**Portões:** nada de rede sem a opção ligada (verificado por captura de tráfego e por importações); falhas e
modo offline com mensagem clara; chaves fora de arquivos e de logs.

### W6 — Galeria interativa, grade e arte do host
**Entrega:** "Adicionar widget" com a galeria, modo de edição "+/−", snapping em grade, `Visual` (céu, cenas,
sprites), experimento de vidro em ADR próprio; **Daily scenes, Mascot**.

## 6. Mudanças em outros documentos

- `ARCHITECTURE.md` §7 e §17 passam a apontar para este ADR; a regra "não começar pela galeria" vale para o
  *motor*: a galeria entra em W1 (offscreen) e W6 (interativa), depois que o contrato estiver provado.
- Um ADR por decisão grande deste plano (vidro, corretor de rede) em `docs/decisions/`.

## 7. Fora do escopo deste ADR

Plugins de terceiros e SDK público; sincronização; loja; widgets em modo "sempre acima" ou clique-através
(seguem a lista de §17); empacotamento MSIX; o conteúdo exato dos textos em cada idioma.

## 8. Critérios para considerar o motor v2 pronto

Todos os marcos W1–W3 concluídos e verificados; orçamentos do M6 mantidos com o conjunto de widgets
ligados; nenhum widget com estado global nem com chamada direta ao Windows; todo widget com teste de
layout em todos os formatos, teste de lógica e verificação UIA; documento de limites por marco.

## 9. Referências lidas

`smartDock/WIDGETS.md` (catálogo e comportamento), `smartDock/widget_gallery.png` (galeria renderizada),
`smartDock/src/platform/windows/widgets.rs` (tamanhos, `Click`, `PanelKind`, struct `Widget`, amostragem) e
`smartDock/src/bin/smartdock/widget_state.rs` (fontes de dados e feeds). Não li `widget_art.rs` nem
`widget_panel.rs` por inteiro; os números de linhas vêm de `wc -l`.

## 10. Alternativas consideradas e descartadas

1. **Portar o `Widget` do Dockn't como está.** Mais rápido para o primeiro widget; viola as regras de
   ownership/estado global do Pod e não escala em manutenção.
2. **Cada widget desenha com D2D.** Máxima liberdade visual; perde acessibilidade gerada, testes sem GPU e
   o limite de confiança entre widget e renderer.
3. **Camada web (WebView/HTML).** Rejeitada em §6; custo de memória por janela incompatível com dezenas de
   widgets.
4. **Um painel nativo por widget.** É o que o Dockn't faz (17 painéis); descartado pelo custo e pela
   inconsistência de acessibilidade e tema.

## 11. Questões abertas (precisam de resposta sua)

1. **Reuso de código do Dockn't.** O `LICENSE` dele é "todos os direitos reservados" em nome de Atylla Azevedo
   (Arkhenet Labs), e o do Pod está como `UNLICENSED`. Como o autor é o mesmo, a decisão é sua: portar a
   lógica pura (conversor, Pomodoro, progresso do tempo, água) copiando, ou reescrever? E qual licença o Pod
   terá (isso também bloqueia redistribuir o pacote do M7)?
2. **Quais widgets ficam de fora?** A tabela da seção 4 propõe "Fora" para App collections, Shortcut, Shelf,
   Nearby sharing, System status, Fans, Keyboard lock, Wallpaper engine, e "decidir" para iFood.
3. **Grade:** célula de 88 DIP e espaço de 12 DIP servem, ou prefere menor/maior (as referências sugerem
   cartões entre 150 e 330 DIP)?
4. **Idiomas:** só pt-BR agora, ou pt-BR e inglês desde W2?
5. **Rede:** "widgets de internet" desligado por padrão, como no Dockn't, e sem telemetria. Confirma?
6. **Prioridade do vidro:** manter pós-MVP (recomendado) ou antecipar o experimento de backdrop antes de W3?
7. **Mascote e cenas:** entram no Pod? (os ativos da !StayAlone têm licença própria.)
8. **Ordem:** a sequência W1→W6 serve, ou algum widget específico precisa vir antes (por exemplo Music ou
   Weather)?

### Decisões do dono (18 de outubro de 2026)

Aceito com as recomendações padrão: célula de grade de **88 DIP com espaço de 12 DIP**; **pt-BR primeiro**
(inglês depois, por tabela de textos); **internet desligada por padrão**, sem telemetria; **vidro pós-MVP**.
**Reuso do Dockn't: reescrever a lógica, não copiar código** (o catálogo e o comportamento observado servem
de especificação). Seguem **em aberto**: a licença do Pod (bloqueia redistribuir o pacote do M7), a lista final
de widgets "Fora" da seção 4, mascote/cenas e a ordem exata a partir de W2.
