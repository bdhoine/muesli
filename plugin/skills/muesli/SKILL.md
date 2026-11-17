---
name: muesli
description: Use when the user asks about their meetings, calls or transcripts recorded with Muesli: whether it is recording, what was said, summaries and action items of a meeting, or renaming a meeting, its attendees or a speaker.
---

# Muesli transcripts

Muesli is an on-device meeting transcriber. This plugin gives you its seven tools through the local `muesli-mcp` server. Everything runs on the user's Mac; nothing is sent anywhere except what you read into this conversation.

## Tools

- `get_status`: whether Muesli is running, detecting a meeting, recording, transcribing an audio file, or idle, and the latest finished transcript.
- `list_transcripts`: meetings, newest first. Filter by `state`.
- `get_transcript`: one meeting by id, with its segments (`start`, `end`, `speaker`, `text`).
- `rename_transcript`, `set_attendees`, `rename_speaker`: change a finished meeting.
- `delete_transcript`: moves a finished meeting's files to the Trash.

## How to use them

1. Call `get_status` first when the user asks "is it recording" or about the last meeting.
2. A transcript is complete only when its `state` is `ready` and it is not `partial`. Say so when you summarise something that is still `recording` or `finalizing`, and offer to look again later. You cannot be notified when it finishes.
3. Speakers are "You" (the microphone) and "Others" (system audio) for live calls. A meeting with `source: "file"` came from an audio file and has a single speaker, "Speaker". Offer `rename_speaker` when the user names a person.
4. Quote the transcript when you give action items, and keep summaries to what was said.
5. Ask the user to confirm before `delete_transcript`. It cannot start or stop a recording, and neither can you.
