---
name: muesli
description: "Use when the user asks about their meetings, calls or transcripts recorded with Muesli, such as whether it is recording, what was said, a summary or the action items of a meeting, renaming a meeting, its attendees or a speaker, or when they correct how a name or term was transcribed."
---

# Muesli transcripts

Muesli is an on-device meeting transcriber. This plugin gives you its nine tools through the local `muesli-mcp` server. Everything stays on the user's Mac.

## Tools

- `get_status`: whether Muesli is running, detecting a meeting, recording, transcribing an audio file, or idle, and the latest finished transcript.
- `list_transcripts`: meetings, newest first. Filter by `state`.
- `get_transcript`: one meeting by id, with its segments (`start`, `end`, `speaker`, `text`).
- `rename_transcript`, `set_attendees`, `rename_speaker`: change a finished meeting.
- `delete_transcript`: moves a finished meeting's files to the Trash.
- `get_vocabulary`, `update_vocabulary`: the names and terms Muesli asks its speech model to spell right in future transcripts.

## Reading a meeting

1. Call `get_status` first when the user asks whether it is recording or about the last meeting.
2. A transcript is complete only when its `state` is `ready` and it is not `partial`. Say so when you summarise a meeting that is still `recording` or `finalizing`, and offer to look again later.
3. Speakers are "You" (the microphone) and "Others" (system audio) in live calls. A meeting with `source: "file"` came from an audio file and has one speaker, "Speaker".
4. Read the whole transcript before you write anything. Use the segment times to tell where a topic starts and ends.

## Summarising a meeting

Whenever you summarise a meeting, also make the meeting itself easier to find and read:

1. **Name the meeting.** If the title is generic ("Meeting", "Call", "Teams call", a file name or a date), give it a title that says what it was about, such as "Q3 budget review" or "Weekly sync: onboarding", with `rename_transcript`.
2. **Identify the people.** From what is said (introductions, people addressed by name, roles, company names, who answers whom), work out who took part. Set the attendee list with `set_attendees`, and replace "Others" or "Speaker" by a real name with `rename_speaker` when one person is clearly behind a label. When a label covers several people, leave it and describe who said what in the summary.
3. **Use the right words.** Speech recognition mishears names, product names, numbers and technical terms. In the summary, write them as the context shows they should be written, and use one spelling throughout. The stored transcript is not changed by this. When you are sure of a name or term that was misheard, add its right spelling to the vocabulary (see below).
4. **Only change what the transcript supports.** If you are not sure who someone is or what the meeting was about, say so in the summary instead of guessing, and leave the title, attendees and speaker names as they are.
5. **Tell the user** which title, attendees and speaker names you set, in one line after the summary, so they can correct them.

Write the summary with the template in [summary-template.md](summary-template.md), so every meeting is summarised in the same shape. Quote the transcript sparingly and only for exact wording that matters, such as a decision or a commitment. Keep to what was said.

## Vocabulary

Muesli prompts its speech model with a vocabulary of names and terms, so they come out spelled right in future transcripts. Keep it useful:

1. **When the user corrects a name or term** ("it's Anna Peeters, not Ana Peters", "we call it LocalVQE"), or you fixed a misheard one in a summary and are sure of it, call `get_vocabulary`, then `update_vocabulary` with `add` and the right spelling, exactly as it should appear.
2. **Only names and special terms:** people, companies, products, projects, places, jargon and acronyms. Never ordinary words, whole sentences or spellings you are guessing.
3. **No near-duplicates.** If a variant is already there ("Peeters, Anna", "LocalVqe"), replace it: `remove` the old one and `add` the new one in the same call.
4. **Tell the user** in one line what you added, and that it helps future transcripts; the stored transcript stays as it is.
5. Remove terms when the user asks, with `remove`. The list holds at most 200 terms of up to 60 characters; when it is full, ask which ones can go.

## Deleting

Ask the user to confirm before `delete_transcript`. You cannot start or stop a recording.
