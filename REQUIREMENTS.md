# Muesli requirements: Claude integration

Status: draft. Scope: what must be built so Claude (Desktop, Claude Code, claude.ai web and mobile) can ask Muesli whether it is recording, whether a transcript is ready, and fetch that transcript.

## 1. Goals and non-goals

Goals

- G1. Muesli detects calls and meetings in progress, transcribes them on-device and writes a file. No cloud or API is involved in capture, detection or transcription.
- G2. Claude can read Muesli's state and transcripts from every Claude surface, including claude.ai in the browser and on mobile.
- G3. The Claude integration is an optional add-on. With it switched off, Muesli behaves exactly as it does today and makes no network connections.

Non-goals

- Starting or stopping a recording from Claude (read-only for now).
- Summaries, action items or any other processing of the transcript; Claude does that.
- Per-person speaker diarization.
- Storing transcripts anywhere other than the Mac.
- Availability while the Mac is asleep or offline.

## 2. Architecture

```
Muesli.app                 no network; capture, detection, whisper.cpp
   │ writes state + meetings to ~/Library/Application Support/Muesli/
   │ writes .md/.json to the transcripts folder (unchanged)
   ▼
muesli-mcp                 separate executable, read-only MCP server
   ├─ stdio                      ─► Claude Desktop, Claude Code      (fully local)
   └─ HTTP on 127.0.0.1:<port>   ─► cloudflared ─► Cloudflare edge (Access + OAuth)
                                                     ─► https://muesli.<your-domain>/mcp
                                                     ─► claude.ai custom connector
```

Swift package layout: a `MuesliCore` library (models, store, state), the `Muesli` app target, and a `muesli-mcp` executable target that depends only on `MuesliCore`.

## 3. App requirements (Muesli.app)

Lifecycle

- A1. Every meeting has a stable id and one of these states: `detected`, `recording`, `finalizing`, `ready`, `failed`.
- A2. The app writes `state.json` atomically on every state change and at least every 10 s as a heartbeat. Fields: `schemaVersion`, `updatedAt`, `pid`, `appState` (`idle` | `recording` | `finalizing` | `error` | `permissionMissing`), `detected` (title, source, since) or null, `current` (id, title, source, startedAt, segmentCount) or null, `lastReady` (id, title, endedAt) or null, `error` or null.
- A3. On launch, a meeting left in `recording` or `finalizing` by a crash is saved with what was transcribed and marked `ready` with `partial: true`, or `failed` if nothing was captured.

Detection

- A4. The detector keeps a current "meeting in progress" value for as long as the signal holds (calendar event running, mic in use), not only at the moment it prompts. This value is reported whether or not the user chose to record.
- A5. The detector keeps running during a recording and detects the end of a call (mic released for a configurable period, default 60 s). On end it prompts to stop, or stops automatically if the user enabled that setting.

Storage

- A6. Each meeting's canonical JSON is written to `~/Library/Application Support/Muesli/meetings/<id>.json`. During recording it is rewritten as chunks are transcribed, so partial transcripts survive a crash and can be read while recording.
- A7. The `.md` and `.json` files in the user's transcripts folder are still written when the meeting becomes `ready`. Their format does not change, apart from an added `state` field in the JSON.
- A8. Audio is never written to disk.

Bridge control

- A9. A menu item "Claude.ai bridge" (off by default) starts and stops `muesli-mcp` in HTTP mode and shows whether it is running. The setting persists across launches.
- A10. The app itself opens no sockets and makes no network requests.

## 4. MCP server requirements (muesli-mcp)

- M1. Reads only from `~/Library/Application Support/Muesli/`. It never touches audio and never writes meeting data.
- M2. Supports two transports from the same binary: stdio (default) and Streamable HTTP (`--http <port>`), bound to `127.0.0.1` only.
- M3. Tools, all read-only and annotated as such:

  | Tool | Input | Output |
  |---|---|---|
  | `get_status` | none | `appRunning` (heartbeat younger than 30 s), `appState`, `detected`, `current`, `lastReady`, `error` |
  | `list_transcripts` | `since?`, `limit?` (default 20), `state?` | id, title, source, startedAt, endedAt, state, durationMinutes, attendees |
  | `get_transcript` | `id`, `format?` (`markdown` \| `json`), `fromSeconds?`, `toSeconds?` | transcript text or segments, `state`, `partial`, `truncated`, `nextFromSeconds` |

- M4. `get_transcript` works on a meeting that is still `recording` and returns what exists so far with `partial: true`.
- M5. A single response is capped (default about 60,000 characters). Longer transcripts are paged with `fromSeconds` and `nextFromSeconds`.
- M6. If the heartbeat is stale, `get_status` reports `appRunning: false` and never reports `recording`.
- M7. In HTTP mode every request must carry a valid Cloudflare Access JWT (`Cf-Access-Jwt-Assertion`). The server checks the signature against the team's public keys, the application audience tag and the allowed email. Requests without one are rejected with 401, including requests that reach the port directly from the local machine.
- M8. In HTTP mode each tool call is appended to a local audit log: time, tool, meeting id, authenticated email.
- M9. Ships as a Claude Desktop extension (`.mcpb`) for the stdio path.

## 5. Cloudflare requirements

Prerequisites

- C1. A Cloudflare account with a domain whose DNS is on Cloudflare. Quick tunnels (`trycloudflare.com`) are not acceptable: the URL changes and Access cannot protect them.
- C2. A Zero Trust organization (the free plan should be enough for one user).
- C3. `cloudflared` installed on the Mac and running as a launchd service.

Configuration

- C4. One named tunnel with one ingress rule: `muesli.<domain>` to `http://127.0.0.1:<port>`, followed by a catch-all 404 rule.
- C5. A self-hosted Access application on `muesli.<domain>` with a single Allow policy for the owner's identity. No bypass or service-token policies.
- C6. Managed OAuth enabled on that Access application, so Cloudflare serves the MCP authorization endpoints (discovery, dynamic client registration, authorize, token) on the hostname. Claude's callback URLs must be allowed: `https://claude.ai/api/mcp/auth_callback` and `https://claude.com/api/mcp/auth_callback`.
- C7. In claude.ai: a custom connector pointing at `https://muesli.<domain>/mcp`. This needs a paid Claude plan.
- C8. A setup guide in the repo covering C1 to C7, plus a `muesli-mcp doctor` command that checks the tunnel, the Access configuration and the local server.

## 6. Security and privacy

- S1. Nothing is exposed until the user turns the bridge on and completes the Cloudflare setup.
- S2. Two independent checks protect the endpoint: Cloudflare Access at the edge and JWT validation in `muesli-mcp` (M7).
- S3. The server is read-only. No tool can start a recording, delete data or change settings.
- S4. The setup guide and the bridge menu state plainly that transcript text fetched by Claude passes through Cloudflare and is sent to Anthropic.
- S5. The existing notice stays: tell participants you are transcribing and check company policy.

## 7. Risks and open questions

- R1. **Managed OAuth with claude.ai is untested.** Cloudflare documents Managed OAuth for MCP servers behind a tunnel and says the client must support RFC 8707; whether claude.ai's connector flow completes against it must be proven first. Fallbacks, in order: a small Cloudflare Worker acting as the OAuth provider in front of the tunnel, or an OAuth authorization server inside `muesli-mcp`.
- R2. The Mac must be awake and online. A closed lid means claude.ai gets a connection error. Accepted for now.
- R3. Claude can only poll. Nothing can notify a conversation that a transcript became ready.
- R4. M7 requires `muesli-mcp` to fetch Cloudflare's signing keys, so the bridge process makes outbound requests. The app still does not.

## 8. Acceptance criteria

1. With the bridge off, a network monitor shows no connections from Muesli during detection, recording and saving.
2. In Claude Desktop with the extension installed: "Is Muesli recording?" returns the true state within 10 s of a change.
3. From claude.ai on a phone, while the Mac is awake: the same question works, and "fetch my last meeting transcript" returns the full text of a 60-minute meeting.
4. During a recording, Claude can fetch the transcript so far.
5. An unauthenticated request to `https://muesli.<domain>/mcp` and a direct request to `127.0.0.1:<port>` without a valid JWT are both rejected.
6. Killing the app mid-recording, then relaunching, yields a saved partial transcript and a correct status.

## 9. Build order

1. Spike R1: a stub MCP server behind the tunnel with Access and Managed OAuth, connected from claude.ai. This decides the auth design.
2. App: lifecycle, `state.json`, canonical meeting store, continuous detection (A1 to A8).
3. `muesli-mcp` over stdio and the `.mcpb` package (M1 to M6, M9).
4. HTTP transport, JWT validation, audit log, bridge toggle (M2, M7, M8, A9).
5. Setup guide and `doctor` command (C8).
