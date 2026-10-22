# POD — Direção visual

> Finally, a name that actually tells you what it is.

**Data:** 20 de outubro de 2026.  
**Status:** direção Editorial escolhida pelo usuário em 20/10/2026 e aplicada ao Quick Notes. Resultados e limites em [validação Editorial](validation/editorial-quick-notes.md). As demais aplicações da linguagem continuam propostas.  
**Referências:** as quatro imagens fornecidas pelo usuário nesta conversa. Não reutilizar capas, marcas, ícones ou artes dessas imagens.

## 1. Intenção

**Objetos pessoais no desktop.** Pod deve parecer uma coleção de pequenas superfícies cuidadosamente compostas. O conteúdo aparece primeiro; os controles ficam disponíveis sem dominar o cartão.

A unidade visual vem de proporção, ritmo, tipografia, acabamento e comportamento. Cada widget pode ter personalidade própria: Notes lembra papel, Weather explora tipografia e atmosfera, Music deixa a capa do álbum definir a composição. Não aplicar a mesma faixa colorida e o mesmo cabeçalho a todos.

As referências indicam três qualidades a perseguir: números e texto com presença, superfícies simples com acabamento preciso e conteúdo visual ocupando espaço real. Perspectiva, desfoque fotográfico e inclinação das imagens de apresentação não fazem parte do funcionamento do produto.

## 2. Relação com o projeto atual

O repositório já tem implementação Rust, motor de widgets, grade, formatos, temas, galeria e vários tipos de widget. A arquitetura de 17/10 é o baseline histórico; os ADRs e documentos de validação registram sua evolução.

Esta direção complementa esses contratos. Não reinicia o projeto nem converte o renderer em HTML. A prévia apresentada na conversa é um estudo de composição; não comprova rendering, desempenho, input nativo ou acessibilidade do executável.

Manter os contratos existentes: view como dados, adaptadores Windows no host, estado persistente separado do desenho e frames sob demanda. O vidro simulado e a camada desktop existentes continuam com as limitações registradas nas respectivas validações.

## 3. Linguagem visual

### Silhueta

Cartões de frente, sem inclinação ou perspectiva. Cantos com raio constante de **20 DIP** em cartões a partir de 188 DIP e **14 DIP** em 88 DIP. Em dimensões livres pequenas, limitar o raio à geometria disponível. O raio de 20 é uma proposta de evolução do atual 14; não alterar todos os widgets numa única troca global sem revisar o conteúdo de cada formato.

Uma borda de 1 DIP resolve o limite da superfície. No claro, ela deve ser apenas perceptível; no escuro, um realce interno ajuda a separar o cartão. Evitar borda dupla e brilho de plástico. Contorno de foco é um estado distinto e deve continuar identificável.

A primeira implementação conserva a sombra curta compatível com o envelope atual da janela. A sombra ampla da prévia é uma intenção de composição, condicionada à validação da margem de janela e da região de input. Não aumentar a área invisível que intercepta cliques nem criar uma janela extra por sombra.

### Ritmo

Escala de espaçamento: **4, 8, 12, 16, 20 e 24 DIP**. Usar 24 DIP de padding em cartões grandes, 16 nos de 188 DIP e 8–12 nos compactos. Escolher pelo espaço e conteúdo; não escalar tudo proporcionalmente.

Conservar a grade existente: célula de 88 DIP, intervalo de 12 DIP, passo de 100 DIP. O espaço entre cartões continua visualmente menor que o espaço de uma seção inteira. Não adicionar outra grade só para o novo estilo.

### Tipografia

Família principal: **Segoe UI Variable**, com fallback Segoe UI e o caminho de fallback de glifos já tratado pelo editor/renderizador. Sem nova fonte obrigatória para a instalação.

| Papel | Tamanho inicial | Peso | Uso |
| --- | --- | --- | --- |
| Microinformação | 12 DIP / entrelinha 16 | Regular | Estado secundário e legenda curta |
| Controle | 13 DIP / 18 | Semibold quando selecionado | Menus e ações |
| Corpo | 16 DIP / 24 | Regular | Conteúdo de leitura e formatos compactos |
| Título de conteúdo | 22 DIP / 28 | Semibold | Cabeçalho semântico em widgets que o possuam |
| Editorial | 28 DIP / 34 | Regular | Composição principal de Notes em formato grande |
| Valor principal | 40 DIP / 44 | Regular ou Semibold | Temperatura, tempo restante, métrica |
| Valor de grande destaque | 56 DIP / 60 | Regular | Apenas em formatos grandes e com espaço medido |

No Windows, DIP é 1/96 de polegada; não confundir 16 DIP com 16 pt do Rich Edit. 16 DIP equivalem a 12 pt antes da conversão pelo mecanismo de texto. O protótipo web usa CSS pixels como aproximação de DIPs para estudo, sem validar a rasterização nativa.

Não reduzir a fonte indefinidamente para caber. Cada formato tem hierarquia própria. Não produzir todo o cartão ampliando um bitmap ou esticando um layout compacto.

## 4. Materiais e cores

### Material padrão: Papel

Opaco e levemente aquecido no tema claro; grafite discreto no escuro. O texto permanece estável independentemente do wallpaper. Não depende de blur, captura de tela ou leitura do papel de parede.

| Token proposto | Claro | Escuro |
| --- | --- | --- |
| `surface.paper` | `#F6F4EE` | `#252827` |
| `surface.neutral` | `#FAFAF8` | `#242629` |
| `text.primary` | `#252B28` | `#F1F1E9` |
| `text.secondary` | `#59635C` | `#BCC4BD` |
| `accent.notes` | `#385A43` | `#ACCAA9` |
| `surface.sage` | `#E7EEDF` | `#29352E` |
| `surface.lavender` | `#ECE7F3` | `#312C3B` |

Esses pares são candidatos, não uma aprovação automática de contraste. Antes de usar em produção, medir os pares efetivamente compostos pelo renderer, inclusive foco, estados desabilitados, subtítulos e cores auxiliares. Reusar os testes existentes; não trocar seus limites para acomodar uma paleta.

Acento é ponto de identificação, foco ou detalhe localizado. Não pintar uma faixa inteira por hábito. Alertas têm ícone/texto além da cor. Verde decorativo não significa “salvo”.

### Material opcional: Vidro

Conservar como opção, sem torná-lo a condição para o design funcionar. Os mesmos espaçamentos, hierarquia e contraste devem funcionar em superfície opaca. Não equiparar translucidez a blur real; seguir ADR-0002 e registrar provas do compositor separadamente.

Em alto contraste, usar cores do sistema, borda robusta, superfícies opacas, sem sombra/brilho/tinta que carreguem informação. A ausência de foco da janela não pode tornar texto ilegível ou apagar erros.

## 5. Formatos

Usar os formatos reais do motor, sem inventar um novo tamanho de janela para cada screenshot.

| Formato | Dimensões DIP | Intenção |
| --- | --- | --- |
| 1×1 | 88 × 88 | Um valor/símbolo; inadequado como editor completo |
| 2×1 | 188 × 88 | Resumo horizontal curto |
| 2×2 | 188 × 188 | Composição compacta |
| 2×3 | 188 × 288 | Texto/lista em coluna |
| 2×4 | 188 × 388 | Coluna editorial |
| 4×2 | 388 × 188 | Faixa larga com conteúdo organizado |
| 4×4 | 388 × 388 | Nota ou conteúdo visual amplo |

Quick Notes continua com resize livre e seus atalhos de formato. O estudo usa a área de um 4×4 para explorar a composição com calma; não modifica o tamanho inicial já salvo pelo usuário. A variante pequena de uma nota pode precisar de scroll durante edição; “glanceable” não autoriza descartar ou truncar o conteúdo persistido.

Alguns formatos largos do motor atual reaproveitam o layout de duas colunas. Antes de mudar esse comportamento global, melhorar Notes e depois um segundo widget como prova de hierarquia por formato.

## 6. Quick Notes: composição escolhida

### Editorial — direção principal

Uma superfície contínua e opaca com texto como protagonista. Cabeçalho de 32 DIP integrado à superfície, sem barra tingida ou separador permanente. À esquerda, rótulo discreto “NOTA”; à direita, menu de reticências. Em formato 4×4, usar padding lateral de 28 DIP, texto de 28 DIP com entrelinha de 34 DIP e espaço generoso entre cabeçalho e conteúdo. Peso Regular, alinhamento à esquerda e ausência de ícones decorativos grandes mantêm a composição editorial.

O material continua sendo Papel (claro aquecido / grafite escuro); “Editorial” define a composição, não obriga transparência ou troca do material. A implementação usa 28 DIP a partir de 360×320 DIP, 22 DIP a partir de 288×200 e 16 DIP nas compactas, sempre por dimensão. A entrelinha usa 1,2 linhas nativas para acompanhar a fonte sem reformatar o documento de texto simples; valores absolutos deste estudo são referências ópticas. Não reduzir fonte em função da quantidade digitada nem escalar automaticamente o cartão inteiro. Notas longas continuam completas e roláveis durante edição.

Um único campo de texto simples ocupa a área restante. A primeira linha é conteúdo do usuário, não um novo campo “título”. Não aplicar formatação rica implícita à primeira linha, não transformar texto em checklist e não inserir linhas decorativas no fundo do editor.

Estado vazio: placeholder “Anote alguma coisa.” some ao digitar e não é persistido. Editor sem borda própria; o contorno pertence ao cartão. Não desenhar um editor alternativo por cima do Rich Edit.

Durante leitura, o conteúdo deve dominar. Controles podem ter baixo destaque, mas o menu continua encontrável e operável sem hover. Ao editar, preservar o texto e sua posição: nenhuma transformação, mudança de fonte ou animação de layout.

### Alternativas arquivadas para comparação

**Papel:** composição com fonte de corpo menor e cabeçalho com símbolo. Permanece como referência comparativa; não é a direção principal escolhida.

**Margem:** trilho estreito de acento lateral e cabeçalho discreto, lembrando um marcador em uma folha. O trilho é decorativo e não carrega estado. Não implica categorias, pastas ou marcações novas.

Implementar Editorial primeiro, conforme a escolha do usuário. As alternativas não exigem um seletor de estilos no produto. Antes de aplicar o novo padrão, resolver overflow/scroll, IME e seleção; não sobrescrever preferências explícitas de fonte ou dimensões que já existam em instâncias salvas.

## 7. Estados e interação

| Estado | Representação |
| --- | --- |
| Repouso | Superfície calma, sem animação, controles discretos disponíveis |
| Hover no menu | Mudança localizada do fundo do botão; sem levantar o cartão |
| Foco por teclado | Anel visível, respeitando os tokens acessíveis existentes |
| Edição de texto | Caret/seleção nativos; nenhuma recoloração ou movimento do conteúdo |
| Arraste | Iniciado somente na região reservada; seleção continua funcionando no editor |
| Resize | Conteúdo relayouta; fonte e peso não mudam para encaixar |
| Alteração pendente | Estado textual curto, sem prometer salvamento |
| Salvando | “Salvando…” somente quando a operação estiver em andamento |
| Gravação confirmada | “Salvo” após ack correspondente à revisão, depois volta ao silêncio |
| Erro de gravação | Mensagem persistente “Não foi possível salvar” e acesso à recuperação existente |
| Modo de edição global | Selo de remoção do host; não inserir um segundo botão de fechar na nota |

Não reservar rodapé grande permanente só para “Salvo”. Uma região de status temporária pode ser necessária; seu aparecimento não deve deslocar seleção/caret ou cobrir texto. Definir espaço estável ou exibir status fora do retângulo editável sem alterar seu layout. Essa integração depende do lifecycle atual e deve ser feita depois do refinamento estático.

Menu visual: ícone pequeno, alvo mínimo 28×28 DIP para mouse e aproximadamente 44×44 quando entrada por toque for suportada. Ícones são monocromáticos e coerentes, sem depender de emoji para ações essenciais. O placeholder não substitui nome acessível.

Escape, Tab, F6 e comandos de teclado devem preservar os contratos já existentes. A nova estética não justifica remover UI Automation ou depender de hover para ações essenciais. Na prévia, o botão “⋯” abre apenas escolhas de cor de demonstração; não representa um menu de produção completo.

## 8. Movimento

Reusar a infraestrutura de transição existente: **150 ms, ease-out**, para hover e alterações decorativas. Estado de erro aparece imediatamente. Movimento reduzido elimina interpolação, sem eliminar feedback.

Sem bounce, tremer contínuo, vinil girando em repouso, cursor customizado ou animação ao trocar foco. O custo de pintura continua motivado por mudança real de conteúdo. Texto selecionado não deve se mover para dar sensação de fluidez.

## 9. Extensão da linguagem aos demais widgets

| Widget | Protagonista | Reutilização da linguagem |
| --- | --- | --- |
| Weather | Temperatura + condição; atmosfera visual opcional | Padding, tipografia e materiais comuns |
| Music | Capa real da sessão e controles claros | Silhueta e alvos de interação comuns; scrim de contraste se houver texto sobre arte |
| Calendar | Data e distribuição de dias | Ritmo, hierarquia e cor semântica legível |
| Pomodoro | Tempo restante e ação principal | Números tabulares, acento localizado, feedback sem loop decorativo |
| System | Um valor ou gráfico curto | Legendas consistentes e cores semânticas com significado |

Não copiar o disco de vinil, gradientes, capas ou tipografia específica das referências. Music não precisa de rotação contínua para comunicar reprodução; Weather não precisa de vídeo de fundo para parecer refinado.

## 10. Ordem de aplicação

1. **Notes/Editorial estático:** padding, fonte de destaque, integração do cabeçalho, raio e menu, preservando conteúdo e tamanho das instâncias existentes.
2. **Legibilidade e input:** conferir Rich Edit/D2D, foco, seleção e IME nos novos bounds antes de mudar outros widgets.
3. **Estados:** associar status às revisões reais e validar que não há deslocamento de texto.
4. **Segundo widget:** aplicar a linguagem a Clock ou Pomodoro para separar tokens reutilizáveis de decisões exclusivas de Notes.
5. **Galeria e demais tipos:** atualizar gradualmente, com previews fiéis aos mesmos tokens/layouts.
6. **Materiais opcionais:** avaliar vidro/sombra ampliada em uma etapa própria, sem bloquear a versão opaca.

## 11. Critérios de revisão

- Rever claro, escuro e alto contraste, em 100/150/200% e monitor com DPI misto.
- Nenhuma informação essencial abaixo de 12 DIP; texto de Notes em 28 DIP no formato grande e layout próprio de 16–18 DIP nos compactos, sem redução automática por volume de texto.
- Texto normal/legendas com contraste mínimo 4,5:1 e foco/controles distinguíveis com 3:1; preservar exigências mais fortes dos testes já existentes.
- Texto longo, uma linha muito longa, nota vazia e emoji não causam sobreposição com menu/status.
- Drag do cabeçalho não seleciona texto; seleção no conteúdo não move a janela.
- Alvos de input acompanham a nova silhueta sem bordas invisíveis que bloqueiem o desktop.
- Mudanças de tema não alteram conteúdo, seleção, posição ou preferências do usuário.
- Repouso continua sem apresentações contínuas; não criar uma segunda cadeia de render para decoração.
- Comparar galeria/golden images conscientemente; não regenerar hashes sem revisar a diferença.
- Validar no executável: a prévia web não encerra nenhum item de IME, acessibilidade, composição ou desempenho.

## 12. Limite desta entrega

Quick Notes/Editorial foi aplicado ao código Rust e tem testes de geometria, contraste, scroll e integração com Rich Edit. A conferência visual nativa no escuro a 100% e os testes interativos de edição estão registrados; outros temas, escalas, IME real e acessibilidade completa permanecem pendentes, conforme o [registro de validação](validation/editorial-quick-notes.md). O estudo interativo continua sendo referência de intenção, não captura do executável. Papel e Margem permanecem alternativas de estudo; refinamentos dos demais widgets continuam fora desta implementação.

## 13. Atualização de 21/10/2026 — estilo macOS (pedido do usuário)

Aprovada a partir do estudo "Pod – Mockup de widgets" (base neutra do macOS, cor viva só em Clima e Música). Implementado:

- **Material:** cartão sem lavagem do acento e sem filete de brilho; só o gradiente sutil do tema e o contorno de 1 DIP. Barras sólidas (sem degradê).
- **Cabeçalho:** `tkit::chip_header` = símbolo vetorial colorido + título `TextStyle::Label` (13 DIP, semibold, cinza). Substitui a ideia de chip preenchido. Cores vêm dos novos papéis `Role::{Blue, Indigo, Orange, Pink, Red, Clay, Cyan}`, que viram a cor do texto no alto contraste. Aplicado em CPU/Memória/Disco, Bateria, progresso do tempo, Rede, Claude, Tarefas e Água; outros widgets seguem sem cabeçalho (§1 continua valendo: não impor o mesmo cabeçalho a todos).
- **Relógio analógico:** face preenchida, números 12/3/6/9, ponteiro de segundos e centro laranja, sem sombra nos ponteiros.
- **Ícone do app:** cápsula com três grãos sobre gradiente (`tools/make-icon.ps1`).

Segunda entrega (mesma data): novas primitivas de view `Segments` (barra em partes iguais) e `Rings` (anéis concêntricos, um item de acessibilidade com os três valores). Novo widget **Sistema** (`pod.overview`: disco, memória e CPU em anéis; 1×1, 2×1, 2×2), **Ano** em 12 segmentos no progresso do tempo (faixas de uma linha, 2×1 e 4×1; o 2×2 segue em anel) e **Calendário 1×1** com dia da semana em vermelho e próximo evento. A previsão por hora do Clima já existia nos formatos 4×1 e 4×2 (a largura de 2 colunas do estudo corresponde a 4 colunas aqui), então não mudou.

Terceira entrega: **Música tingida pela capa** (com capa, o cartão inteiro usa a cor média dela, escurecida até o texto branco passar de 4,5:1; texto em `Role::OnFill`/`OnFillDim`; sobre qualquer preenchimento colorido o acento vira branco, então botões e ícones ficam legíveis; alto contraste não tinge).

Fora desta entrega: vidro como padrão e sombra ampla. Ambos continuam condicionados à validação de plataforma (ADR-0002 e §2 deste documento) e têm ADR próprio, porque mudam a margem da janela e a região de input: [ADR-0005](decisions/0005-glass-default-and-wide-shadow.md) (proposta, não implementada).

## 14. Foco e ativação dos cartões (21/10/2026)

Cartões de formato fixo são criados com `WS_EX_NOACTIVATE` (`host::window_ex_style`). Clicar num cartão **não tira mais o foco do aplicativo em uso**
(antes o primeiro plano virava "nenhuma janela" e o teclado ficava sem destino). Notas continuam ativando (são para digitar). A ação principal por
UI Automation (`Invoke`) e o F6 das notas não mudaram. **Lacuna que já existia:** não há como pôr o foco do teclado num cartão, então Espaço/Enter
no cartão não são alcançáveis; a acessibilidade dos botões depende do `Invoke`. Medidas e limites em [validação](validation/focus-click.md).

Atualização (mesma data): verificado também o **modo de edição** e o **menu da bandeja** com os cartões sem ativação (resultados idênticos aos de antes da mudança).
Nota de código: os cartões **não têm selo "−" nem alça**; excluir é pelo menu do botão direito (sem confirmação para widgets sem conteúdo) e o tamanho muda
puxando as bordas.
