use std::collections::HashMap;
use std::time::Duration;

use axum::http::StatusCode;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::models::{Category, FriendshipRelation};

use super::extraction::{ExtractedDraft, extraction_request};
use super::judgments::{Decision, decision_from_answers, questions};

type Error = (StatusCode, String);

/// Server-only credentials and endpoints. Endpoints can be replaced by a local mock in tests.
#[derive(Clone)]
pub struct VoiceProviders {
    pub openai_api_key: String,
    pub typesafe_api_key: String,
    pub openai_base_url: String,
    pub typesafe_base_url: String,
}

impl VoiceProviders {
    pub fn from_env() -> Option<Self> {
        let openai_api_key = std::env::var("OPENAI_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty())?;
        let typesafe_api_key = std::env::var("TYPESAFE_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty())?;
        Some(Self {
            openai_api_key,
            typesafe_api_key,
            openai_base_url: "https://api.openai.com/v1".into(),
            typesafe_base_url: "https://api.typesafe.ai/v1".into(),
        })
    }

    pub(super) async fn transcribe(
        &self,
        client: &reqwest::Client,
        audio: Vec<u8>,
        mime: &str,
        extension: &str,
    ) -> Result<String, Error> {
        let part = reqwest::multipart::Part::bytes(audio)
            .file_name(format!("voice.{extension}"))
            .mime_str(mime)
            .map_err(|_| super::bad_input("Unsupported audio format"))?;
        let form = reqwest::multipart::Form::new()
            .text("model", "gpt-transcribe")
            .text("languages[]", "zh-tw")
            .text("languages[]", "en")
            .text("prompt", "A short personal expense or income entry in Taiwanese Mandarin, possibly with English names. Preserve all spoken amounts and names.")
            .part("file", part);
        #[derive(Deserialize)]
        struct Transcript {
            text: String,
        }
        let response: Transcript = provider_json(
            client
                .post(format!("{}/audio/transcriptions", self.openai_base_url))
                .bearer_auth(&self.openai_api_key)
                .multipart(form),
            "Speech transcription",
        )
        .await?;
        let text = response.text.trim().to_string();
        crate::validation::validate_string_length(&text, "Transcript", 8000)?;
        Ok(text)
    }

    pub(super) async fn judge(
        &self,
        client: &reqwest::Client,
        transcript: &str,
        categories: &[Category],
        friends: &[FriendshipRelation],
    ) -> Result<Decision, Error> {
        #[derive(Deserialize)]
        struct Judgments {
            answers: HashMap<String, Value>,
        }
        let response: Judgments = provider_json(
            client.post(format!("{}/systemone", self.typesafe_base_url))
                .bearer_auth(&self.typesafe_api_key)
                .json(&json!({
                    "model": "jev-1.13.0",
                    "state": {
                        "transcript": transcript,
                        "categories": categories,
                        "friends": friends.iter().map(|f| json!({"id": f.user_id, "nickname": f.nickname})).collect::<Vec<_>>()
                    },
                    "questions": questions(categories, friends)
                })),
            "Jev classification",
        ).await?;
        decision_from_answers(response.answers, categories, friends)
    }

    pub(super) async fn extract(
        &self,
        client: &reqwest::Client,
        transcript: &str,
        reference_date: time::Date,
        currency: &str,
        decision: &Decision,
    ) -> Result<ExtractedDraft, Error> {
        let response: Value = provider_json(
            client
                .post(format!("{}/chat/completions", self.openai_base_url))
                .bearer_auth(&self.openai_api_key)
                .json(&extraction_request(
                    transcript,
                    reference_date,
                    currency,
                    decision,
                )),
            "Voice field extraction",
        )
        .await?;
        let content = response["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| {
                provider_error("Voice field extraction returned no draft; please try again")
            })?;
        super::extraction::parse_extraction(content)
    }
}

async fn provider_json<T: DeserializeOwned>(
    request: reqwest::RequestBuilder,
    stage: &str,
) -> Result<T, Error> {
    let response = request
        .timeout(Duration::from_secs(40))
        .send()
        .await
        .inspect_err(|error| tracing::warn!(%stage, %error, "voice provider connection failed"))
        .map_err(|_| provider_error(&format!("{stage} could not connect; please try again")))?;
    if !response.status().is_success() {
        tracing::warn!(%stage, status = %response.status(), "voice provider request failed");
        return Err(provider_error(&format!(
            "{stage} is unavailable; please try again"
        )));
    }
    response
        .json()
        .await
        .inspect_err(|error| {
            tracing::warn!(%stage, %error, "voice provider response decoding failed");
        })
        .map_err(|_| {
            provider_error(&format!(
                "{stage} returned an invalid response; please try again"
            ))
        })
}

pub(super) fn provider_error(message: &str) -> Error {
    (StatusCode::BAD_GATEWAY, message.to_string())
}
