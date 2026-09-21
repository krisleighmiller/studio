Use Grok Imagine Video v1.5 Image-to-Video if a specific start frame is necessary.

Lip sync / talking-shot notes:
- VEED: bad timing when WAV longer than plate; avoid for hero shots.
- Sync.so: keeps plate timing but odd / rubber mouth on AI faces.
- Van 2.5 text-to-video + audio: invents identity (unusable); keep SC16_textsync.mp4 as failure sample. No character image field.
- Van/Wan I2V + audio: good mouth/identity when the still matches the character. **Output aspect follows the start image** — no I2V size/ratio override. `KrisinOffice.png` is 1024×1024, which is why Van returned square (1440×1440). Use a 9:16 talking still (e.g. `Characters/Kris/talking_stills/KrisinOffice_9x16.png`) or ffmpeg-pad before upload. Wan 2.7 docs: aspect follows input image/video.
- Seedance 2.0 I2V has `ratio: "9:16"` but no driving-audio URL (only invents audio) — not a drop-in for text-sync.
- Audio pad: honor `editing.head_handle_seconds` / `tail_handle_seconds` (usually 1s/1s) plus ~150ms sync — not 150ms-only. Prompt must say hold settle before/after speech.
- Max 5/10s; no out-frame continuity chain.
- Framing: do not prompt “tight close-up” from a medium desk still—Van will push into a face-filling CU mid-take (SC7_v1 jump scare). Lock to still distance; ban zoom/push-in in prompt + negative.
- Text-sync must poll + `download_media` audio locally before pad/upload/video; never leave download to the user.
