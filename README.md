# Liga das Lendas Assist

Assistente de partida para **League of Legends** que conversa com a API local do cliente
(LCU — *League Client Update*) e com a **Live Client Data API** do jogo. Roda no seu PC,
ao lado do cliente, e mostra tudo num painel em `http://127.0.0.1:3737`.

## O que ele faz

| Fase | Recurso |
| --- | --- |
| Fila | **Aceita a partida** automaticamente (com atraso configurável; se você recusar antes, ele respeita). |
| Seleção de campeões | Mostra seu time, inimigos e bans. **Seleciona/declara** seu campeão pela lista de preferência da sua rota, **confirma** e **bane** automaticamente (opcional). Pula campeões banidos, já escolhidos, que você não tem, e nunca bane o que um aliado declarou. Se você já selecionou algo manualmente, ele respeita. |
| Em partida | Timers de **Dragão/Ancião, Arauto, Barão e inibidores**, placar de dragões, KDA/CS/CS por minuto, ouro em itens por time, tempo de renascimento dos inimigos e **dicas** (janela de objetivo com inimigos mortos, objetivo nascendo, farm baixo, vantagem/desvantagem de ouro). |

O assistente detecta sozinho quando o cliente abre/fecha e reconecta.

## Como usar

Requer **Node.js 20+** no mesmo computador do cliente do LoL (Windows ou macOS).

```bash
npm install
npm run build
npm start
```

Abra `http://127.0.0.1:3737`, ajuste as opções no painel e clique em **Salvar**
(fica gravado em `config.json`; veja `config.example.json`). Os campeões podem ser
escritos pelo nome (`"Lee Sin"`) ou alias (`"LeeSin"`). A lista da rota vem antes da lista
`default` ("Qualquer rota").

Variáveis de ambiente opcionais:

- `LOL_INSTALL_DIR` — pasta de instalação do LoL, se não estiver no caminho padrão.
- `LOL_ASSIST_CONFIG` — caminho do arquivo de configuração.
- `LOL_ASSIST_PORT` — porta do painel.

## Como funciona

```
src/
  lcu/credentials.ts   acha porta e token (linha de comando do LeagueClientUx ou lockfile)
  lcu/http.ts          HTTPS para 127.0.0.1 validando a CA da Riot (certs/riotgames.pem)
  lcu/client.ts        REST + WebSocket WAMP (assina OnJsonApiEvent)
  features/autoAccept.ts   /lol-matchmaking/v1/ready-check
  features/champSelect.ts  /lol-champ-select/v1/session (decisão pura + execução)
  live/liveGame.ts     polling de https://127.0.0.1:2999/liveclientdata/allgamedata
  live/objectives.ts   timers de objetivos (valores em OBJECTIVE_TIMES)
  server/dashboard.ts  painel local com Server-Sent Events
  assistant.ts         ciclo de vida: conecta, segue a fase do jogo, reconecta
```

- O painel só escuta em `127.0.0.1`, recusa requisições com `Host` diferente de localhost
  (DNS rebinding) e só aceita alterar a configuração com `Content-Type: application/json`.
- Os tempos de objetivos mudam entre patches: ajuste `OBJECTIVE_TIMES` em
  `src/live/objectives.ts` se necessário.

## Desenvolvimento

```bash
npm test        # testes (vitest)
npm run typecheck
```

## Aviso

Projeto não oficial, sem relação com a Riot Games. Usa apenas as APIs locais
documentadas pela comunidade e não lê memória nem modifica o jogo, mas automações no
cliente (aceitar fila, selecionar campeão) ficam sob sua responsabilidade.
League of Legends é marca registrada da Riot Games, Inc.
