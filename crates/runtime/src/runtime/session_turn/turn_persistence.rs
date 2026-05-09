use crate::models::MessageExecution;

use super::voice_transcript::enrich_voice_message_with_transcript;

/// Build persisted turn payload (execution metadata + user input text).
pub(crate) fn build_turn_persistence_payload(
    input: &str,
    duration_ms: u64,
    iterations: u32,
) -> (MessageExecution, String) {
    let execution = MessageExecution::new().complete(duration_ms, iterations);
    let persisted_input =
        enrich_voice_message_with_transcript(input, &[]).unwrap_or_else(|| input.to_string());
    (execution, persisted_input)
}
