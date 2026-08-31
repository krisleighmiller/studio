//! Bundled specialist prompts. Source of truth lives in Studio/System_Prompts/;
//! these are compiled into the binary so average users are not editing contracts.
//!
//! Live Director turns use the thin planner in `director_actions::planner_system_prompt`.
//! Text specialists (`script`, `character_creation`, `scene_plan`, `production_helper`)
//! load the matching contract below inside the host runner. Media specialists are
//! host-owned pipelines (Settings models + approval); their contracts remain bundled
//! here as the product source of truth / future reference.

pub const SCRIPT_ASSISTANT_PROMPT: &str =
    include_str!("../../../System_Prompts/script-assist-system-prompt.md");

pub const CHARACTER_CREATION_PROMPT: &str =
    include_str!("../../../System_Prompts/character-creation-system-prompt.md");

pub const SCENE_PLAN_PROMPT: &str =
    include_str!("../../../System_Prompts/vid-prompt-system-prompt.md");

pub const MODEL_SHEET_PROMPT: &str =
    include_str!("../../../System_Prompts/model-sheet-system-prompt.md");

pub const STILL_IMAGE_PROMPT: &str =
    include_str!("../../../System_Prompts/still-image-system-prompt.md");

pub const SCENE_GENERATION_PROMPT: &str =
    include_str!("../../../System_Prompts/vid-gen-system-prompt.md");

pub const TEXT_SYNC_PROMPT: &str =
    include_str!("../../../System_Prompts/text-sync-system-prompt.md");

pub const ASSEMBLE_PROMPT: &str =
    include_str!("../../../System_Prompts/post-prod-system-prompt.md");

pub const PRODUCTION_HELPER_PROMPT: &str =
    include_str!("../../../System_Prompts/production-helper-system-prompt.md");
