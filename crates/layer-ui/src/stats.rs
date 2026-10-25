//! Compact diagnostics, labels and summaries shared by native hosts.
use layer_render::RendererTelemetry;
use serde::Serialize;
#[derive(Clone, Debug, Serialize)]
pub struct StatRow {
    pub label: String,
    pub value: String,
    pub description: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct StatsView {
    pub rows: Vec<StatRow>,
    pub samples: Vec<f32>,
    /// Raw execution timings for host qualification; keep presentation separate.
    pub gpu_samples: Vec<f32>,
    pub resident_bytes: u64,
    /// Select › Modify values received and previews drawn since the session
    /// began, for host qualification.
    pub selection_values: u64,
    pub selection_previews: u64,
    pub budget_ms: f32,
    pub chart_label: &'static str,
    /// Insert the chart after this many metric rows on every frontend.
    pub chart_after_rows: usize,
}
pub(super) fn view(t: RendererTelemetry, localization: &crate::Localizer) -> StatsView {
    fn percentiles(values: Vec<f32>) -> String {
        if values.is_empty() {
            return "—".into();
        }
        let mut v = values;
        v.sort_by(f32::total_cmp);
        let p = |q: f32| v[((v.len() - 1) as f32 * q).round() as usize];
        format!("{:.2} / {:.2} / {:.2}", p(0.5), p(0.95), p(0.99))
    }
    let samples = t.cpu.ordered();
    let submitted = t.frame_rate.is_some_and(|rate| rate.source == layer_render::FrameRateSource::Submitted);
    let rows = vec![
        StatRow {
            label: "CPU · ms".into(),
            value: percentiles(samples.clone()),
            description: "Median / p95 / p99: prepare, encode and submit. Not GPU completion or input latency.".into(),
        },
        StatRow {
            label: "GPU · ms".into(),
            value: if t.gpu_timestamps {
                percentiles(t.gpu.ordered())
            } else {
                "Unavailable".into()
            },
            description: "Median / p95 / p99 GPU execution, including GPU scheduling gaps. Excludes presentation. Browser timestamps may be quantized.".into(),
        },
        StatRow {
            label: localization.text(if submitted { crate::MessageId::DIAGNOSTICS_SUBMITTED_FPS } else { crate::MessageId::DIAGNOSTICS_FPS }).to_string(),
            value: t.frame_rate.map_or_else(|| "—".into(), |rate| format!("{:.0}", rate.fps)),
            description: localization.text(if submitted { crate::MessageId::DIAGNOSTICS_SUBMITTED_FPS_DESCRIPTION } else { crate::MessageId::DIAGNOSTICS_FPS_DESCRIPTION }).to_string(),
        },
        StatRow {
            label: "Frames".into(),
            value: t.submissions.to_string(),
            description: "Submitted drawing updates. Cached canvas navigation and compositor frames are not counted.".into(),
        },
        StatRow {
            label: "Canvas storage".into(),
            value: format!("{:.1} MiB", t.resident_bytes as f64 / 1048576.),
            description: "Tracked paint, preview, proof, masks, effect parameters, composite and scene scratch allocation. Excludes driver overhead and imported assets; not total VRAM use.".into(),
        },
        StatRow {
            label: "Dabs".into(),
            value: t.dabs.to_string(),
            description: "Brush dabs submitted since renderer creation.".into(),
        },
        StatRow {
            label: "Effect passes".into(),
            value: t.effect_passes.to_string(),
            description: "GPU effect render passes in the latest drawing update. Compatible chains and tile draws share passes.".into(),
        },
        StatRow {
            label: "Pipelines".into(),
            value: t.compiled_effects.to_string(),
            description: "Cached effect pipelines. Parameter edits do not compile new pipelines.".into(),
        },
    ];
    StatsView {
        rows,
        samples,
        gpu_samples: if t.gpu_timestamps { t.gpu.ordered() } else { Vec::new() },
        resident_bytes: t.resident_bytes,
        selection_values: 0,
        selection_previews: 0,
        budget_ms: 1000. / 120.,
        chart_label: "CPU render · last 120 updates",
        chart_after_rows: 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_orders_chart_and_storage_with_their_metrics() {
        let localization = crate::Localizer::shared(crate::UiLanguage::English);
        let view = view(RendererTelemetry::default(), &localization);
        assert_eq!(
            view.rows.iter().map(|row| row.label.as_str()).collect::<Vec<_>>(),
            [
                "CPU · ms",
                "GPU · ms",
                "FPS",
                "Frames",
                "Canvas storage",
                "Dabs",
                "Effect passes",
                "Pipelines"
            ]
        );
        assert_eq!(view.rows[view.chart_after_rows - 1].label, "GPU · ms");
        assert_eq!(view.rows[view.chart_after_rows].label, "FPS");
        assert_eq!(serde_json::to_value(&view).unwrap()["chart_after_rows"], 2);
    }

    #[test]
    fn fps_uses_presentation_feedback_without_substituting_submission_or_gpu_rates() {
        let localization = crate::Localizer::shared(crate::UiLanguage::English);
        let mut telemetry = RendererTelemetry { submissions: 1200, ..Default::default() };
        telemetry.gpu.push(1.);
        assert_eq!(view(telemetry.clone(), &localization).rows[2].value, "—");
        telemetry.frame_rate = Some(layer_render::FrameRate { fps: 119., source: layer_render::FrameRateSource::Presented });
        assert_eq!(view(telemetry.clone(), &localization).rows[2].value, "119");
        telemetry.frame_rate = Some(layer_render::FrameRate { fps: 0., source: layer_render::FrameRateSource::Presented });
        assert_eq!(view(telemetry, &localization).rows[2].value, "0");
    }

    #[test]
    fn submission_fallback_is_explicit_and_does_not_claim_presentation() {
        let localization = crate::Localizer::shared(crate::UiLanguage::English);
        let telemetry = RendererTelemetry { frame_rate: Some(layer_render::FrameRate { fps: 120., source: layer_render::FrameRateSource::Submitted }), ..Default::default() };
        let metric = &view(telemetry, &localization).rows[2];
        assert_eq!(metric.label, "Submitted FPS");
        assert_eq!(metric.value, "120");
        assert!(metric.description.contains("unavailable"));
    }
}
