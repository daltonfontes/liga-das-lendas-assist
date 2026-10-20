# Liga das Lendas Assist

Assistente de partida para **League of Legends**, feito em **[Tauri](https://tauri.app) + Rust**.
Conversa com a API local do cliente (LCU — *League Client Update*) e com a
**Live Client Data API** do jogo. O app é um executável de ~5 MB: usa o WebView do sistema
(WebView2 no Windows, WebKit no macOS) em vez de embutir um Chromium.

## O que ele faz

| Fase | Recurso |
| --- | --- |
| Fila | **Aceita a partida** automaticamente (com atraso configurável; se você recusar antes, ele respeita). |
| Seleção de campeões | Mostra seu time, inimigos e bans, e um card com o plano do assistente (com botão para **pausar a automação** só naquela seleção). **Seleciona/declara** seu campeão pela lista de preferência da sua rota, **confirma** e **bane** automaticamente (opcional). Pula campeões banidos, já escolhidos, que você não tem, e nunca bane o que um aliado declarou. Se você já selecionou algo manualmente, ele respeita. |
| Em partida | Timers de **Dragão/Ancião, Arauto, Barão e inibidores**, placar de dragões, KDA/CS/CS por minuto, ouro em itens por time, tempo de renascimento dos inimigos e **dicas** (janela de objetivo com inimigos mortos, objetivo nascendo, farm baixo, vantagem/desvantagem de ouro). |

O assistente detecta sozinho quando o cliente abre/fecha e reconecta.

## Instalar

Baixe o instalador na página de *Releases* (gerado pelo workflow `release.yml` ao publicar
uma tag `v*`). A configuração fica salva na pasta de configuração do app
(`%APPDATA%\com.daltonfontes.ligadaslendasassist\config.json` no Windows); veja
`config.example.json`. Os campeões podem ser escritos pelo nome (`"Lee Sin"`) ou alias
(`"LeeSin"`); a lista da rota vem antes da lista `default` ("Qualquer rota").

Se o LoL estiver instalado num caminho fora do padrão, o app acha pelo processo do cliente;
dá para forçar com a variável `LOL_INSTALL_DIR`.

## Desenvolver

Requisitos: [Rust](https://rustup.rs) estável, Node.js 20+ (só para o CLI do Tauri) e os
[pré-requisitos do Tauri](https://tauri.app/start/prerequisites/) do seu sistema
(no Windows, o WebView2 já vem instalado no 10/11).

```bash
npm install          # instala o CLI do Tauri
npm run dev          # abre o app em modo desenvolvimento
npm run build        # gera o instalador em src-tauri/target/release/bundle

cd src-tauri
cargo test           # testes
cargo clippy --all-targets -- -D warnings
```

## Estrutura

```
ui/                        interface (HTML/CSS/JS puro, sem bundler; fontes Inter e
                           Barlow Condensed embutidas em ui/fonts, licença OFL)
src-tauri/
  src/lib.rs               app Tauri: comandos get_state / get_config / set_config
  src/assistant.rs         ciclo de vida: acha o cliente, segue a fase do jogo, reconecta
  src/lcu/credentials.rs   porta e token (lockfile ou argumentos do LeagueClientUx)
  src/lcu/tls.rs           TLS fixado na CA da Riot (certs/riotgames.pem)
  src/lcu/client.rs        REST + WebSocket WAMP (assina OnJsonApiEvent)
  src/features/            aceitar fila, seleção de campeões, índice de campeões
  src/live/                Live Client Data API (porta 2999) e timers de objetivos
```

Notas:

- **TLS:** o certificado do cliente é emitido para `rclient` e a CA da Riot é X.509 v1, que o
  verificador WebPKI padrão recusa. Por isso `tls.rs` tem um verificador próprio que confere a
  assinatura do certificado pela CA da Riot, a validade e a assinatura do handshake — não é um
  "aceita qualquer certificado".
- Os tempos de objetivos mudam entre patches: ajuste o módulo `times` em
  `src-tauri/src/live/objectives.rs`.
- A janela não tem acesso a nada além dos três comandos do app (`capabilities/default.json`)
  e roda com uma CSP restrita.

## Aviso

Projeto não oficial, sem relação com a Riot Games. Usa apenas as APIs locais
documentadas pela comunidade e não lê memória nem modifica o jogo, mas automações no
cliente (aceitar fila, selecionar campeão) ficam sob sua responsabilidade.
League of Legends é marca registrada da Riot Games, Inc.
