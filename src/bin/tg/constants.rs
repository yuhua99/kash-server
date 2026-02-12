pub const DEFAULT_OPENAI_MODEL: &str = "gpt-4o-mini";
pub const DEFAULT_REASONING_EFFORT: &str = "low";
pub const DEFAULT_TIMEZONE: &str = "Asia/Taipei";

pub const SIMILAR_RECORDS_DAYS: i64 = 180;
pub const SIMILAR_RECORDS_LIMIT: usize = 5;
pub const SIMILAR_AMOUNT_RATIO: f64 = 0.2;
pub const RECORD_CONTEXT_LIMIT: usize = 30;

pub const PENDING_ACTION_TTL_SECONDS: i64 = 180;
pub const CONTEXT_MAX_TURNS: usize = 3;
pub const CONTEXT_TTL_SECONDS: i64 = 600;

pub const EDIT_REQUEST_KEYWORDS: [&str; 4] = ["edit", "update", "change", "rename"];
pub const DELETE_REQUEST_KEYWORDS: [&str; 3] = ["delete", "remove", "erase"];

pub const CONFIRM_WORDS: [&str; 8] = [
    "yes", "confirm", "ok", "okay", "ok do it", "do it", "apply", "proceed",
];

pub const CANCEL_WORDS: [&str; 6] = [
    "cancel",
    "stop",
    "never mind",
    "nevermind",
    "don't do it",
    "dont do it",
];
