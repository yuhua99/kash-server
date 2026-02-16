pub const DEFAULT_OPENAI_MODEL: &str = "gpt-4o-mini";
pub const DEFAULT_REASONING_EFFORT: &str = "low";
pub const DEFAULT_TIMEZONE: &str = "Asia/Taipei";
pub const DEFAULT_WHISPER_MODEL: &str = "whisper-1";

pub const MAX_VOICE_FILE_SIZE: usize = 3 * 1024 * 1024;
pub const MAX_PHOTO_FILE_SIZE: usize = 10 * 1024 * 1024;

pub const TOOL_MAX_ROUNDS: usize = 6;
pub const CONTEXT_MAX_TURNS: usize = 3;
pub const CONTEXT_TTL_SECONDS: i64 = 600;
