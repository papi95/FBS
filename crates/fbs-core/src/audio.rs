use crate::ids::SourceId;
use serde::{Deserialize, Serialize};

/// One mixer strip. Gain is linear here; the UI shows dB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Strip {
    pub source: SourceId,
    pub name: String,
    pub gain: f32,
    pub muted: bool,
    pub solo: bool,
    pub pan: f32,
    /// Peak hold for the meter, decays each tick.
    pub peak: f32,
}

impl Strip {
    pub fn new(source: SourceId, name: impl Into<String>) -> Self {
        Self {
            source,
            name: name.into(),
            gain: 1.0,
            muted: false,
            solo: false,
            pan: 0.0,
            peak: 0.0,
        }
    }

    pub fn gain_db(&self) -> f32 {
        if self.gain <= f32::EPSILON {
            return f32::NEG_INFINITY;
        }
        20.0 * self.gain.log10()
    }

    pub fn set_gain_db(&mut self, db: f32) {
        self.gain = 10f32.powf(db / 20.0).clamp(0.0, 4.0);
    }
}

/// The whole desk. Solo is additive: if anything is soloed, only soloed
/// strips reach the master, which matches hardware consoles.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Mixer {
    pub strips: Vec<Strip>,
    pub master: f32,
    pub monitor: f32,
}

const PEAK_DECAY_PER_TICK: f32 = 0.06;

impl Mixer {
    pub fn add(&mut self, strip: Strip) {
        self.strips.push(strip);
    }

    pub fn remove(&mut self, source: SourceId) {
        self.strips.retain(|s| s.source != source);
    }

    pub fn strip_mut(&mut self, source: SourceId) -> Option<&mut Strip> {
        self.strips.iter_mut().find(|s| s.source == source)
    }

    pub fn any_solo(&self) -> bool {
        self.strips.iter().any(|s| s.solo)
    }

    /// Effective linear gain reaching the master bus for one strip.
    pub fn effective_gain(&self, source: SourceId) -> f32 {
        let Some(strip) = self.strips.iter().find(|s| s.source == source) else {
            return 0.0;
        };
        if strip.muted {
            return 0.0;
        }
        if self.any_solo() && !strip.solo {
            return 0.0;
        }
        strip.gain * self.master
    }

    /// Feed a measured peak (0.0..=1.0) and decay others.
    pub fn report_peak(&mut self, source: SourceId, peak: f32) {
        if let Some(strip) = self.strip_mut(source) {
            strip.peak = strip.peak.max(peak.clamp(0.0, 1.0));
        }
    }

    pub fn tick(&mut self) {
        for strip in &mut self.strips {
            strip.peak = (strip.peak - PEAK_DECAY_PER_TICK).max(0.0);
        }
    }

    /// Warn when the summed mix is likely to clip; the UI shows this red.
    pub fn is_clipping(&self) -> bool {
        let sum: f32 = self
            .strips
            .iter()
            .filter(|s| !s.muted && (!self.any_solo() || s.solo))
            .map(|s| s.gain)
            .sum();
        sum * self.master > 1.0
    }
}
