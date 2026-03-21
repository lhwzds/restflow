use crate::define_simple_storage;

define_simple_storage! {
    /// Byte-level telemetry metric sample storage.
    pub struct TelemetryMetricSampleStorage { table: "telemetry_metric_samples_v1" }
}
