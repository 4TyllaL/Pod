# W5 — corretor HTTPS com opt-in e widget Weather

Ambiente: Windows 11 build 26200, 150% de DPI, release com asserts de debug, internet real.

## O que existe

- **Opt-in "Widgets de internet"**: item no menu da bandeja (marcado quando ligado), **desligado por padrão**, gravado em
  `preferences.internet`. Sem ele **nenhuma thread de rede é iniciada e nenhum pedido sai**: os widgets de internet só
  recebem `WeatherState::Off` e explicam como ligar. Ao desligar, a thread para e o último estado é esquecido.
- **Corretor HTTPS** (`netbroker.rs`): a única porta de saída de rede. `GET` por **WinHTTP** (certificado validado pelo
  Windows, proxy automático), **só `https`**, só para os hosts da lista (`geocoding-api.open-meteo.com`,
  `api.open-meteo.com`), **sem redirecionamento** (um 3xx é erro), sem cookies, 8 s por etapa e **teto de 256 KiB** na
  resposta. Quem monta a URL é o host (`Endpoint::url`, a partir de modelos registrados e com parâmetros validados e
  codificados); o widget nunca fornece URL. Qualquer URL fora da lista é recusada antes de abrir conexão.
- **Provedor Weather** (`weather.rs`): thread própria, só com opt-in ligado e algum Weather visível; geocodifica o nome
  (cache na vida da thread) e busca a previsão; atualiza a cada 15 min e tenta de novo em 1 min após falha. Várias
  instâncias da mesma cidade compartilham os pedidos.
- **Widget `pod.weather`** (1×1, 2×1, 2×2): temperatura, condição em português, máx/mín, sensação, umidade e vento (2×2).
  A cidade (único estado durável) se define pelo menu do cartão ou pelo botão "Definir cidade"; o texto é validado
  (2 a 60 caracteres). Sem cidade, nada é assinado e nada é enviado. Estados: internet desligada, buscando, escolha a
  cidade, cidade não encontrada, sem conexão (com nova tentativa).
- **O que sai do computador**: o nome da cidade (geocodificação) e as coordenadas dela (previsão). Nada mais:
  sem telemetria, sem identificador. Categoria nova **Internet** na galeria "Adicionar widget".

## Verificado

- 243 testes no `pod-app` (4 `#[ignore]`) e 142 no `pod-model`: URLs montadas dos modelos e parâmetros codificados
  (tentativa de injetar `&count=999#` vira texto), parâmetros inválidos sem URL, **lista de hosts** (http, host
  parecido, `@`, porta, usuário e `//` recusados), leitura das respostas e rejeição de respostas quebradas, descrições
  em português, widget (sem cidade nada assinado, cidade validada e persistida, estados e formatos cabem) e provedor sem
  start ou sem cidades **não faz nenhum pedido**.
- **Internet real**: `netbroker::real_https_get_to_open_meteo` e `weather::real_weather_for_a_city_and_a_missing_one`
  (`--ignored`) geocodificam "Recife", leem a previsão e tratam uma cidade inexistente como `NotFound`.
- **Ao vivo, pelo menu da bandeja** (2 cartões de Recife): desligado → "Internet desligada" e **0 pedidos** em 5 s; ao
  ligar, em poucos segundos "Recife, Brasil · 27° · Nublado · máx 31° · mín 25° · Sensação 30° · Umidade 73% · Vento
  10 km/h" com **exatamente 2 pedidos** (geocodificação e previsão, compartilhados pelos dois cartões); ao desligar
  de novo, voltou a "Internet desligada" e os pedidos **ficaram em 2** por mais de 8 s.

## Não verificado / limites

- **Proxy corporativo, VPN e rede lenta** não foram testados (só a rede desta máquina). Falha de TLS/DNS cai em "Sem
  conexão" sem detalhe.
- O fuso/idioma da previsão vem do serviço (`timezone=auto`, `language=pt` na geocodificação); a condição é traduzida
  localmente por código WMO (códigos raros caem em "Tempo instável").
- Os termos de uso do Open-Meteo (uso gratuito não comercial) não foram revistos para distribuição comercial: **antes de
  distribuir o Pod, confirmar licença/termos** e considerar um plano pago.
- Sem previsão por hora nem por vários dias, sem ícones desenhados (símbolos Unicode do Windows) e sem localização
  automática (só cidade digitada, para não pedir permissão de localização).
- News, GitHub e AI chat do W5 **não** foram feitos; GitHub e chave de API (`Secret`) ainda exigem o cofre de credenciais.
- O disco C: ficou cheio durante o trabalho (ver abaixo); limpei caches de build do projeto duas vezes.

## News (segunda rodada do W5): manchetes por RSS/Atom

- **Catálogo fechado** (`pod-model::news::FEEDS`): BBC Brasil, Agência Brasil, UOL Notícias, BBC World, Hacker News e
  The Verge, todos verificados (200 direto, sem redirecionamento, < 256 KiB). O widget guarda só o **id** do feed; a URL
  vem do catálogo (`Endpoint::Feed`) e os hosts entram na lista do corretor (agora 7). G1 e NASA ficaram de fora: os feeds
  passam de 256 KiB. **Não há feed digitado pelo usuário** (qualquer URL arbitrária fugiria da lista permitida).
- **Leitor de XML mínimo** (sem entidades externas, só texto): RSS 2.0 e Atom; CDATA, entidades (`&amp;`, `&#39;`,
  `&#xE9;`), marcações HTML removidas dos títulos, UTF-8 ou ISO-8859-1 (o UOL é Latin-1), datas RFC 822 e ISO 8601 com
  fuso, no máximo 10 manchetes de até 140 caracteres; XML truncado ou HTML de erro não quebram (devolvem vazio).
- **Widget `pod.news`** (1×1, 2×1, 2×2): fonte, idade, posição (3/10) e a manchete em até 5 linhas; ◀ ▶ navegam, **Abrir**
  abre a matéria no navegador padrão e as manchetes **passam sozinhas a cada 12 s** (a navegação manual reinicia a
  contagem). A fonte se escolhe no menu do cartão (submenu "Fonte"). Atualiza a cada 15 min; falha tenta em 1 min.
- **Abrir é seguro**: o widget só pede "abrir a manchete N"; o host guarda os endereços que ele mesmo buscou e os
  revalida (`clean_url`: só http/https, sem usuário) antes do `ShellExecute`.
- Mesmas regras de privacidade e opt-in do clima: sem "Widgets de internet" não há thread nem pedido, e os dois widgets
  compartilham a mesma chave de ligar/desligar.

### Verificado

- 252 testes no `pod-app` (5 `#[ignore]`) e 150 no `pod-model`: leitor (RSS, Atom, datas nos dois formatos, entidades,
  Latin-1, limites, lixo), catálogo (todo feed é `https` na lista de hosts; ids únicos), widget (assina e esquece,
  rotação por timer e reinício na navegação manual, uma manchete só não gira, perda de dados cancela o timer, abrir pede
  só o índice, troca de fonte persiste, estado gravado desconhecido cai no padrão, todos os estados e formatos cabem).
- **Internet real**: `news::every_catalog_feed_works_for_real` (`--ignored`) busca os 6 feeds (10 manchetes cada).
- **Ao vivo** (2 cartões do mesmo feed): desligado → "Internet desligada" e 0 pedidos; ligado → manchetes da BBC Brasil
  com **1 único pedido** para os dois cartões; ▶ foi à manchete 2/10; "Abrir" registrou a URL exata da matéria (modo
  simulado, sem abrir o navegador); a troca automática aconteceu sozinha em < 14 s; desligado de novo → "Internet
  desligada" e os pedidos pararam.

### Não verificado / limites

- **Abrir de verdade no navegador**: só o modo simulado foi exercitado (o `ShellExecute` é o mesmo dos links do widget
  Favorites, já verificado antes).
- Alguns itens não trazem data (os "ao vivo" da BBC Brasil): aparecem sem idade. Sem imagens das matérias.
- Feeds que exigem cabeçalhos especiais, `ETag`/`If-Modified-Since` (sem cache condicional: 1 pedido a cada 15 min).
- Direitos de uso do conteúdo dos feeds: o Pod só mostra o **título** e abre o link; mesmo assim vale confirmar os termos
  de cada fonte antes de distribuir.
