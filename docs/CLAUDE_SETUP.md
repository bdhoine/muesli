# Connecting Muesli to Claude

Muesli ships a small read-only MCP server, `muesli-mcp`, inside the app bundle. Claude uses it to ask whether Muesli is recording, to list transcripts and to read one. Switch it on in **Preferences… → Claude** and pick a connection.

Whatever you pick: a transcript Claude reads is sent to Anthropic, and in remote mode it also passes through Cloudflare. Claude can only read; it cannot start or stop a recording.

## Local: Claude Desktop and Claude Code

Nothing is exposed to the network. Claude starts `muesli-mcp` itself and talks to it over stdio.

1. Preferences → Claude → switch on, choose **Local**.
2. Muesli adds a `muesli` entry to Claude Desktop's `claude_desktop_config.json` (other entries are left alone). Quit and reopen Claude Desktop.
3. For Claude Code, run the command shown in Preferences: `claude mcp add muesli -- "/Applications/Muesli.app/Contents/MacOS/muesli-mcp"`.

Switching the integration off, or to Remote, removes the Claude Desktop entry again.

## Remote: claude.ai in the browser and on mobile

claude.ai calls connectors from Anthropic's servers, so Muesli must be reachable at a public HTTPS address. A Cloudflare Tunnel provides the address and Cloudflare Access guards it. The Mac must be awake and online for this to work.

```
claude.ai ─► https://muesli.example.com/mcp ─► Cloudflare Access ─► cloudflared ─► 127.0.0.1:8747 (muesli-mcp)
```

**This flow has not been tested end to end against claude.ai yet.** The open question is whether claude.ai's connector sign-in completes against Cloudflare's Managed OAuth (step 3). Everything on the Mac side is tested.

You need a Cloudflare account, a domain whose DNS is on Cloudflare, and a Zero Trust organization (the free plan should do).

### 1. Tunnel

```bash
brew install cloudflared
cloudflared tunnel login
cloudflared tunnel create muesli
cloudflared tunnel route dns muesli muesli.example.com
```

`~/.cloudflared/config.yml`:

```yaml
tunnel: muesli
credentials-file: /Users/YOU/.cloudflared/<tunnel-id>.json
ingress:
  - hostname: muesli.example.com
    service: http://127.0.0.1:8747
  - service: http_status:404
```

Run it at login with `sudo cloudflared service install`. Do not use a quick tunnel (`trycloudflare.com`): its address changes and Access cannot protect it.

### 2. Access application

In the Zero Trust dashboard, under Access → Applications, add a **Self-hosted** application:

- Application domain: `muesli.example.com`
- One policy: **Allow**, Include → Emails → your own address. No Bypass or Service Auth policies.

After saving, copy the **Application Audience (AUD) tag** from the application's overview. Your team domain is shown under Settings → Custom Pages, in the form `myteam.cloudflareaccess.com`.

### 3. Managed OAuth

In the same application's advanced settings, enable **Managed OAuth**. Cloudflare then serves the sign-in endpoints that claude.ai expects on your hostname. If it asks for allowed redirect addresses, add both:

```
https://claude.ai/api/mcp/auth_callback
https://claude.com/api/mcp/auth_callback
```

### 4. Muesli

Preferences → Claude → switch on, choose **Remote**, and fill in:

| Field | Value |
|---|---|
| Public hostname | `muesli.example.com` |
| Team domain | `myteam.cloudflareaccess.com` |
| Audience (AUD) tag | from step 2 |
| Allowed email | the address in your Access policy |
| Local port | `8747`, matching `config.yml` |

Press **Apply**. The status line should read "Running on 127.0.0.1:8747".

`muesli-mcp` does not trust the tunnel blindly. It checks the Cloudflare Access token on every request (signature, audience, expiry, and that the email is yours) and rejects anything else, including calls made directly to the local port.

### 5. Check

```bash
/Applications/Muesli.app/Contents/MacOS/muesli-mcp doctor
```

It reports each link in the chain: the app, the local server, Cloudflare's signing keys, `cloudflared`, and whether the public address is up and asks for sign-in. If it says the public address answered without sign-in, switch the integration off and fix the Access application first.

### 6. claude.ai

Settings → Connectors → Add custom connector, with `https://muesli.example.com/mcp`. Sign in when Cloudflare asks. This needs a paid Claude plan. Then ask Claude: "Is Muesli recording?"

## What Claude can call

| Tool | Returns |
|---|---|
| `get_status` | Whether the app is running, whether a call or meeting is detected, the current recording, the last finished transcript |
| `list_transcripts` | Meetings, newest first; filter with `since`, `state`, `limit` |
| `get_transcript` | One transcript as Markdown or JSON; works during a recording (`partial: true`); long ones are paged with `fromSeconds` |

A transcript is complete when its `state` is `ready`. Claude has to ask; Muesli cannot notify a conversation.

Remote tool calls are logged to `~/Library/Application Support/Muesli/audit.log`: time, tool, meeting id, email.

## Troubleshooting

- **Claude Desktop does not show Muesli.** Restart Claude Desktop. Check that the `muesli` entry in `~/Library/Application Support/Claude/claude_desktop_config.json` points at the app's current location; toggling the integration off and on rewrites it.
- **Tools answer "switched off" or "set to remote mode".** The stdio server only answers in Local mode.
- **"Stopped unexpectedly".** Another process holds the port, or the remote fields are incomplete. Run `muesli-mcp --http 8747` in a terminal to see the message.
- **claude.ai cannot connect.** Run `doctor`. HTTP 502 or 530 from the public address means the tunnel is not reaching the Mac.
