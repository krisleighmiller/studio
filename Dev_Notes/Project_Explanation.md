# Purpose: "AI video production studio" 

## Tech stack: 
    - OpenCode for Custom AI agents
    - Atlas Cloud API for LLM models
    - Atlas Cloud MCP server for generative AI models
    - Tauri for Voice Generator app

## Reference Documentation: 
    - Atlas Cloud https://www.atlascloud.ai/docs
    - Open Code https://opencode.ai/docs/

## Overall Plans: 
    Studio/Dev_Notes/Workbench_Direction.md — product thesis
    Studio/Dev_Notes/Workbench_Spec.md — workbench spec / build plan (v0.1)

## Currently Finished:

    ### Character Reference Assistant: 
        Studio/System_Prompts/character-creation-system-prompt.md
        Input: a paragraph, brain dump, traits, dialogue, a list, an image, an existing sheet, or an incomplete idea.
        Output: A markdown file of character traits.

    ### Scene Prompt Assistant:
        Studio/System_Prompts/vid-prompt-system-prompt.md
        Input: A finished script and the path to any reference images (character, location, etc)
        Output: A list of scenes and the prompt required to generate that scene.

    ### Video/Audio Generator Assistant:
        Studio/System_Prompts/vid-gen-system-prompt.md
        Input: The handoff yaml for one scene and a path to any videos required for continuity; a wav file for a cloned voice, a yaml file for a prebuilt profile, or "no audio" for video only
        Output: A completed silent video and a completed narration/dialogue wav file
       
    ### Voice Generator App:
        Studio/Voice_Generator/dist/Voice_Profile_Studio_0.1.a 0_amd64.AppImage
        Input: None.
        Output: A voice profile consisting of a .wav file and yaml metadata.

    ### Model Sheet Generator:
        Studio/System_Prompts/model-sheet-system-prompt.md
        Input: Text based character description with or without reference photo(s); Specific output path
        Output: 12 images of character in various poses on a green screen. Text prompt with no references outputs a single front facing closeup for approval before creating the rest of the images.

    ### Image Generator:
        /home/kris/Content_Creation/Studio/System_Prompts/still-image-system-prompt.md
        Input: Text based description with or without reference photo(s); Specific output path including filename and extension.
        Output: 1 or more still images. 

    ### Script Assistant:
        /home/kris/Content_Creation/Studio/System_Prompts/script-assist-system-prompt.md
        Input: Book chapter, notes, or text description of a story or idea.
        Output: Markdown file of a complete script including character descriptions, scene descriptions and shot notes.

    ### Lip Sync Assistant (text-sync) (audio-driven talking shots):
        Studio/System_Prompts/text-sync-system-prompt.md
        Input: Pasted talking-shot handoff; voice_clone WAV or voice_profile YAML; character still; exact output audio + video paths.
        Output: Performance WAV (TTS from cue text) → ffmpeg-padded WAV → combined talking MP4 via Van/Wan image-to-video + audio. No continuity out-frame chains; spoken budget ≤9s (split ≤8s).

    ### Post Production Assistant:
        /home/kris/Content_Creation/Studio/System_Prompts/post-prod-system-prompt.md
        Input: File from Scene Prompt Assistant and a list of audio and video files in scene order
        Output: A complete video with audio dialogue/narration and optional music and captioning

    ### Production Helper:
        /home/kris/Content_Creation/Studio/System_Prompts/production-helper-system-prompt.md
        Input: Project docs (prod.md, sheets, scene sidecars, assemble maps, prompts, notes) plus a review / brainstorm / digest ask
        Output: Chat verdict and fix list; optional research/idea digest markdown when an output path is given. No media generation.

## Open Items:

    - (none currently tracked)

    