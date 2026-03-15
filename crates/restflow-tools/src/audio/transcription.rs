use anyhow::{Context, Result, anyhow};
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::default::{get_codecs, get_probe};
use tokio::fs;
use tokio::task::spawn_blocking;
use tokio::time::timeout;

const DEFAULT_TRANSCRIPTION_MODEL: &str = "whisper-1";
const DEFAULT_MAX_DIRECT_UPLOAD_BYTES: u64 = 25 * 1024 * 1024;
const DEFAULT_CHUNK_DURATION_MS: u64 = 8 * 60 * 1_000;
const DEFAULT_TARGET_SAMPLE_RATE: u32 = 16_000;
const DEFAULT_TARGET_CHANNELS: u16 = 1;
const TRANSCRIPTION_ENDPOINT: &str = "https://api.openai.com/v1/audio/transcriptions";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TimestampGranularity {
    None,
    Segment,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimedTextSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SegmentedTranscriptionOutput {
    pub text: String,
    pub model: String,
    #[serde(default)]
    pub segments: Vec<TimedTextSegment>,
}

#[derive(Debug, Clone)]
pub struct SegmentedTranscriptionOptions {
    pub model: String,
    pub language: Option<String>,
    pub timestamp_granularity: TimestampGranularity,
    pub chunk_long_audio: bool,
}

impl Default for SegmentedTranscriptionOptions {
    fn default() -> Self {
        Self {
            model: DEFAULT_TRANSCRIPTION_MODEL.to_string(),
            language: None,
            timestamp_granularity: TimestampGranularity::None,
            chunk_long_audio: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SegmentedTranscriptionConfig {
    pub max_direct_upload_bytes: u64,
    pub chunk_duration_ms: u64,
    pub target_sample_rate: u32,
    pub target_channels: u16,
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    pub user_agent: String,
    pub temp_root: PathBuf,
}

impl Default for SegmentedTranscriptionConfig {
    fn default() -> Self {
        Self {
            max_direct_upload_bytes: DEFAULT_MAX_DIRECT_UPLOAD_BYTES,
            chunk_duration_ms: DEFAULT_CHUNK_DURATION_MS,
            target_sample_rate: DEFAULT_TARGET_SAMPLE_RATE,
            target_channels: DEFAULT_TARGET_CHANNELS,
            connect_timeout: Duration::from_secs(12),
            request_timeout: Duration::from_secs(180),
            user_agent: "RestFlow/0.1".to_string(),
            temp_root: std::env::temp_dir().join("restflow-transcription-chunks"),
        }
    }
}

#[derive(Debug, Clone)]
struct PreparedAudioUpload {
    path: PathBuf,
    start_ms: u64,
    duration_ms: u64,
}

#[derive(Debug)]
struct PreparedTranscriptionInput {
    uploads: Vec<PreparedAudioUpload>,
    cleanup_dir: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct DecodedMonoAudio {
    sample_rate: u32,
    samples: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct OpenAITranscriptionResponse {
    text: String,
    #[serde(default)]
    segments: Vec<OpenAITranscriptionSegment>,
}

#[derive(Debug, Deserialize)]
struct OpenAITranscriptionSegment {
    start: f64,
    end: f64,
    text: String,
}

pub async fn transcribe_audio_file(
    api_key: &str,
    file_path: &str,
    options: &SegmentedTranscriptionOptions,
    config: &SegmentedTranscriptionConfig,
) -> Result<SegmentedTranscriptionOutput> {
    if api_key.trim().is_empty() {
        return Err(anyhow!("API key is required."));
    }

    if file_path.trim().is_empty() {
        return Err(anyhow!("Audio file path cannot be empty."));
    }

    ensure_supported_model(options)?;

    let prepared = prepare_transcription_input(file_path, options, config).await?;
    let output = transcribe_prepared_input(api_key, options, config, &prepared).await;

    if let Some(directory) = prepared.cleanup_dir {
        let _ = fs::remove_dir_all(directory).await;
    }

    output
}

async fn transcribe_prepared_input(
    api_key: &str,
    options: &SegmentedTranscriptionOptions,
    config: &SegmentedTranscriptionConfig,
    prepared: &PreparedTranscriptionInput,
) -> Result<SegmentedTranscriptionOutput> {
    let mut merged_segments = Vec::new();
    let mut merged_text_parts = Vec::new();

    for upload in &prepared.uploads {
        let output = timeout(
            config.request_timeout,
            transcribe_chunk(api_key, options, config, upload),
        )
        .await
        .map_err(|_| anyhow!("Transcription request timed out."))??;

        if !output.text.trim().is_empty() {
            merged_text_parts.push(output.text.trim().to_string());
        }
        merged_segments.extend(output.segments);
    }

    let text = if merged_segments.is_empty() {
        merged_text_parts.join("\n\n")
    } else {
        join_segment_text(&merged_segments)
    };

    if text.trim().is_empty() {
        return Err(anyhow!("Transcription completed without any text."));
    }

    Ok(SegmentedTranscriptionOutput {
        text,
        model: options.model.clone(),
        segments: merged_segments,
    })
}

async fn transcribe_chunk(
    api_key: &str,
    options: &SegmentedTranscriptionOptions,
    config: &SegmentedTranscriptionConfig,
    upload: &PreparedAudioUpload,
) -> Result<SegmentedTranscriptionOutput> {
    let client = reqwest::Client::builder()
        .connect_timeout(config.connect_timeout)
        .timeout(config.request_timeout)
        .user_agent(&config.user_agent)
        .build()
        .context("Failed to build transcription HTTP client")?;

    let file_name = upload
        .path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("audio.wav")
        .to_string();
    let bytes = fs::read(&upload.path)
        .await
        .with_context(|| format!("Failed to read audio chunk '{}'.", upload.path.display()))?;

    let mut form = Form::new().text("model", options.model.clone()).part(
        "file",
        Part::bytes(bytes)
            .file_name(file_name)
            .mime_str(&mime_type_for_audio_path(&upload.path))
            .context("Failed to set audio chunk MIME type")?,
    );

    if options.timestamp_granularity == TimestampGranularity::Segment {
        form = form
            .text("response_format", "verbose_json".to_string())
            .text("timestamp_granularities[]", "segment".to_string());
    }

    if let Some(language) = options
        .language
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        form = form.text("language", language.to_string());
    }

    let response = client
        .post(TRANSCRIPTION_ENDPOINT)
        .header("Authorization", format!("Bearer {api_key}"))
        .multipart(form)
        .send()
        .await
        .context("Failed to reach OpenAI transcription API")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!(
            "OpenAI transcription request failed with HTTP {}: {}",
            status,
            body
        ));
    }

    let payload: OpenAITranscriptionResponse = response
        .json()
        .await
        .context("Failed to parse transcription response")?;

    let mut segments = payload
        .segments
        .into_iter()
        .map(|segment| TimedTextSegment {
            start_ms: seconds_to_ms(segment.start) + upload.start_ms,
            end_ms: seconds_to_ms(segment.end) + upload.start_ms,
            text: segment.text.trim().to_string(),
        })
        .filter(|segment| !segment.text.is_empty())
        .collect::<Vec<_>>();

    if options.timestamp_granularity == TimestampGranularity::Segment
        && segments.is_empty()
        && !payload.text.trim().is_empty()
    {
        segments.push(TimedTextSegment {
            start_ms: upload.start_ms,
            end_ms: upload.start_ms + upload.duration_ms.max(1_000),
            text: payload.text.trim().to_string(),
        });
    }

    let text = if segments.is_empty() {
        payload.text.trim().to_string()
    } else {
        join_segment_text(&segments)
    };

    Ok(SegmentedTranscriptionOutput {
        text,
        model: options.model.clone(),
        segments,
    })
}

async fn prepare_transcription_input(
    file_path: &str,
    options: &SegmentedTranscriptionOptions,
    config: &SegmentedTranscriptionConfig,
) -> Result<PreparedTranscriptionInput> {
    let path = PathBuf::from(file_path);
    let metadata = fs::metadata(&path)
        .await
        .with_context(|| format!("Failed to inspect audio file '{}'.", path.display()))?;

    if metadata.len() <= config.max_direct_upload_bytes {
        return Ok(PreparedTranscriptionInput {
            uploads: vec![PreparedAudioUpload {
                path,
                start_ms: 0,
                duration_ms: 1_000,
            }],
            cleanup_dir: None,
        });
    }

    if !options.chunk_long_audio {
        return Err(anyhow!(
            "File too large ({} bytes). Maximum size is {} bytes.",
            metadata.len(),
            config.max_direct_upload_bytes
        ));
    }

    fs::create_dir_all(&config.temp_root)
        .await
        .with_context(|| format!("Failed to create '{}'.", config.temp_root.display()))?;

    let chunk_directory = next_chunk_directory(&config.temp_root, &path);
    fs::create_dir_all(&chunk_directory)
        .await
        .with_context(|| format!("Failed to create '{}'.", chunk_directory.display()))?;

    let chunk_directory_for_worker = chunk_directory.clone();
    let path_for_worker = path.clone();
    let config_for_worker = config.clone();
    let uploads = spawn_blocking(move || {
        build_chunked_audio_uploads(
            &path_for_worker,
            &chunk_directory_for_worker,
            &config_for_worker,
        )
    })
    .await
    .context("Chunk processing task failed")??;

    Ok(PreparedTranscriptionInput {
        uploads,
        cleanup_dir: Some(chunk_directory),
    })
}

fn build_chunked_audio_uploads(
    file_path: &Path,
    chunk_directory: &Path,
    config: &SegmentedTranscriptionConfig,
) -> Result<Vec<PreparedAudioUpload>> {
    let decoded = decode_audio_file(file_path)?;
    let resampled = resample_to_pcm16_mono(
        &decoded.samples,
        decoded.sample_rate,
        config.target_sample_rate,
    );

    if resampled.is_empty() {
        return Err(anyhow!("Decoded audio did not contain any samples."));
    }

    write_pcm_chunks(
        &resampled,
        config.target_sample_rate,
        config.chunk_duration_ms,
        config.target_channels,
        chunk_directory,
    )
}

fn decode_audio_file(file_path: &Path) -> Result<DecodedMonoAudio> {
    let mut hint = Hint::new();
    if let Some(extension) = file_path.extension().and_then(|value| value.to_str()) {
        hint.with_extension(extension);
    }

    let file = File::open(file_path)
        .with_context(|| format!("Failed to open audio file '{}'.", file_path.display()))?;
    let source = MediaSourceStream::new(Box::new(file), Default::default());
    let probed = get_probe()
        .format(
            &hint,
            source,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .context("Failed to read audio container")?;
    let mut format = probed.format;
    let (track_id, codec_params) = {
        let track = format
            .default_track()
            .ok_or_else(|| anyhow!("No audio track was found in the media file."))?;
        (track.id, track.codec_params.clone())
    };
    let sample_rate = codec_params
        .sample_rate
        .ok_or_else(|| anyhow!("Unable to determine the audio sample rate."))?;
    let mut decoder = get_codecs()
        .make(&codec_params, &DecoderOptions::default())
        .context("Failed to initialize audio decoder")?;
    let mut samples = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(_)) => break,
            Err(SymphoniaError::ResetRequired) => {
                return Err(anyhow!(
                    "Audio stream reset is not supported for transcription."
                ));
            }
            Err(error) => return Err(anyhow!("Failed to read audio packet: {error}")),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymphoniaError::IoError(_)) => break,
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(error) => return Err(anyhow!("Failed to decode audio packet: {error}")),
        };

        let spec = *decoded.spec();
        let channels = spec.channels.count();
        let duration = decoded.capacity() as u64;
        let mut buffer = SampleBuffer::<f32>::new(duration, spec);
        buffer.copy_interleaved_ref(decoded);

        for frame in buffer.samples().chunks(channels) {
            let mixed = if channels == 1 {
                frame[0]
            } else {
                frame.iter().copied().sum::<f32>() / channels as f32
            };
            samples.push(mixed.clamp(-1.0, 1.0));
        }
    }

    Ok(DecodedMonoAudio {
        sample_rate,
        samples,
    })
}

fn resample_to_pcm16_mono(samples: &[f32], source_rate: u32, target_rate: u32) -> Vec<i16> {
    if samples.is_empty() {
        return Vec::new();
    }

    let resampled = if source_rate == target_rate {
        samples.to_vec()
    } else {
        let ratio = target_rate as f64 / source_rate as f64;
        let output_len = ((samples.len() as f64) * ratio).ceil() as usize;
        let mut output = Vec::with_capacity(output_len);

        for index in 0..output_len {
            let source_position = index as f64 / ratio;
            let left_index = source_position.floor() as usize;
            let right_index = (left_index + 1).min(samples.len().saturating_sub(1));
            let fraction = (source_position - left_index as f64) as f32;
            let left = samples[left_index];
            let right = samples[right_index];
            output.push(left + ((right - left) * fraction));
        }

        output
    };

    resampled
        .into_iter()
        .map(|sample| {
            let clamped = sample.clamp(-1.0, 1.0);
            (clamped * i16::MAX as f32).round() as i16
        })
        .collect()
}

fn write_pcm_chunks(
    samples: &[i16],
    sample_rate: u32,
    chunk_duration_ms: u64,
    target_channels: u16,
    output_directory: &Path,
) -> Result<Vec<PreparedAudioUpload>> {
    let samples_per_chunk = ((sample_rate as u64 * chunk_duration_ms) / 1_000) as usize;
    if samples_per_chunk == 0 {
        return Err(anyhow!("Chunk duration must be greater than zero."));
    }

    let spec = hound::WavSpec {
        channels: target_channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut uploads = Vec::new();

    for (index, chunk_samples) in samples.chunks(samples_per_chunk).enumerate() {
        let chunk_path = output_directory.join(format!("chunk-{index:04}.wav"));
        let mut writer = hound::WavWriter::create(&chunk_path, spec)
            .with_context(|| format!("Failed to create WAV chunk '{}'.", chunk_path.display()))?;

        for sample in chunk_samples {
            writer.write_sample(*sample).with_context(|| {
                format!("Failed to write WAV chunk '{}'.", chunk_path.display())
            })?;
        }

        writer
            .finalize()
            .with_context(|| format!("Failed to finalize WAV chunk '{}'.", chunk_path.display()))?;

        let duration_ms = ((chunk_samples.len() as u64) * 1_000) / sample_rate as u64;
        uploads.push(PreparedAudioUpload {
            path: chunk_path,
            start_ms: index as u64 * chunk_duration_ms,
            duration_ms,
        });
    }

    Ok(uploads)
}

fn mime_type_for_audio_path(path: &Path) -> String {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
    {
        Some(extension) if extension == "mp3" => "audio/mpeg".to_string(),
        Some(extension) if extension == "m4a" || extension == "mp4" => "audio/mp4".to_string(),
        Some(extension) if extension == "wav" => "audio/wav".to_string(),
        Some(extension) if extension == "ogg" => "audio/ogg".to_string(),
        Some(extension) if extension == "webm" => "audio/webm".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}

fn next_chunk_directory(temp_root: &Path, source_path: &Path) -> PathBuf {
    let mut digest = Sha256::new();
    digest.update(source_path.to_string_lossy().as_bytes());
    digest.update(format!("{:?}", SystemTime::now()).as_bytes());
    let hash = format!("{:x}", digest.finalize());
    temp_root.join(&hash[..16])
}

fn ensure_supported_model(options: &SegmentedTranscriptionOptions) -> Result<()> {
    if options.timestamp_granularity == TimestampGranularity::Segment
        && options.model != DEFAULT_TRANSCRIPTION_MODEL
    {
        return Err(anyhow!(
            "Segmented timestamps require whisper-1. Leave the model unset or choose whisper-1."
        ));
    }

    Ok(())
}

fn join_segment_text(segments: &[TimedTextSegment]) -> String {
    segments
        .iter()
        .map(|segment| segment.text.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn seconds_to_ms(seconds: f64) -> u64 {
    if !seconds.is_finite() || seconds <= 0.0 {
        return 0;
    }

    (seconds * 1_000.0).round() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn write_pcm_chunks_preserves_offsets() {
        let dir = tempfile::tempdir().expect("tempdir");
        let samples = vec![1i16; 12];

        let chunks = write_pcm_chunks(&samples, 4, 1_000, 1, dir.path()).expect("write chunks");

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].start_ms, 0);
        assert_eq!(chunks[1].start_ms, 1_000);
        assert_eq!(chunks[2].start_ms, 2_000);
        assert!(chunks.iter().all(|chunk| chunk.path.exists()));
    }

    #[test]
    fn resample_to_pcm16_mono_keeps_exact_length_when_sample_rates_match() {
        let samples = vec![0.0f32, 0.25, -0.25, 1.0];
        let output = resample_to_pcm16_mono(&samples, 16_000, 16_000);

        assert_eq!(output.len(), samples.len());
        assert!(output[3] > 30_000);
    }

    #[test]
    fn decode_and_chunk_pipeline_can_process_generated_wav() {
        let dir = tempfile::tempdir().expect("tempdir");
        let input = dir.path().join("input.wav");
        let output = dir.path().join("chunks");
        fs::create_dir_all(&output).expect("create output dir");

        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 8_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&input, spec).expect("create wav");
        for _ in 0..16_000 {
            writer.write_sample(1_024i16).expect("write sample");
        }
        writer.finalize().expect("finalize wav");

        let config = SegmentedTranscriptionConfig::default();
        let uploads = build_chunked_audio_uploads(&input, &output, &config).expect("chunk audio");

        assert_eq!(uploads.len(), 1);
        assert!(uploads[0].path.exists());
        assert!(uploads[0].duration_ms > 0);
    }

    #[test]
    fn segment_fallback_uses_text_when_segments_missing() {
        let payload = OpenAITranscriptionResponse {
            text: "Hello world".to_string(),
            segments: Vec::new(),
        };

        let upload = PreparedAudioUpload {
            path: PathBuf::from("/tmp/example.wav"),
            start_ms: 2_000,
            duration_ms: 5_000,
        };

        let mut segments = payload
            .segments
            .into_iter()
            .map(|segment| TimedTextSegment {
                start_ms: seconds_to_ms(segment.start) + upload.start_ms,
                end_ms: seconds_to_ms(segment.end) + upload.start_ms,
                text: segment.text.trim().to_string(),
            })
            .filter(|segment| !segment.text.is_empty())
            .collect::<Vec<_>>();

        if segments.is_empty() && !payload.text.trim().is_empty() {
            segments.push(TimedTextSegment {
                start_ms: upload.start_ms,
                end_ms: upload.start_ms + upload.duration_ms.max(1_000),
                text: payload.text.trim().to_string(),
            });
        }

        assert_eq!(
            segments,
            vec![TimedTextSegment {
                start_ms: 2_000,
                end_ms: 7_000,
                text: "Hello world".to_string(),
            }]
        );
    }

    #[test]
    fn segment_mode_requires_whisper_one() {
        let options = SegmentedTranscriptionOptions {
            model: "gpt-4o-mini-transcribe".to_string(),
            language: None,
            timestamp_granularity: TimestampGranularity::Segment,
            chunk_long_audio: true,
        };

        let error = ensure_supported_model(&options).expect_err("segment model mismatch");
        assert!(
            error
                .to_string()
                .contains("Segmented timestamps require whisper-1")
        );
    }
}
