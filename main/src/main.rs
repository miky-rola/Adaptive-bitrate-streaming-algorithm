
/// this here is to test my implementation of 
use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct QualityLevel {
    pub bitrate: u32,      // bits per second
    pub width: u32,
    pub height: u32,
    pub codec: String,
}

#[derive(Debug)]
pub struct SegmentInfo {
    pub quality_level: usize,
    pub size_bytes: u32,
    pub duration: Duration,
    pub download_time: Duration,
}

#[derive(Debug)]
pub struct BufferState {
    pub current_level: Duration,
    pub target_level: Duration,
    pub max_level: Duration,
    pub min_level: Duration,
}

pub struct AdaptiveBitrateStreamer {
    quality_levels: Vec<QualityLevel>,
    current_quality: usize,
    bandwidth_history: VecDeque<(Instant, u32)>, // (timestamp, bytes_per_second)
    buffer_state: BufferState,
    segment_history: VecDeque<SegmentInfo>,
    
    // Algorithm parameters
    bandwidth_window: Duration,
    safety_factor: f32,
    buffer_panic_threshold: Duration,
    buffer_seek_threshold: Duration,
    min_bandwidth_samples: usize,
}

impl AdaptiveBitrateStreamer {
    pub fn new(quality_levels: Vec<QualityLevel>) -> Self {
        let initial_quality: usize = quality_levels.len() / 2; // Start with middle quality
        
        Self {
            quality_levels,
            current_quality: initial_quality,
            bandwidth_history: VecDeque::new(),
            buffer_state: BufferState {
                current_level: Duration::from_secs(0),
                target_level: Duration::from_secs(30),
                max_level: Duration::from_secs(60),
                min_level: Duration::from_secs(5),
            },
            segment_history: VecDeque::new(),
            bandwidth_window: Duration::from_secs(10),
            safety_factor: 0.8, // Use 80% of estimated bandwidth
            buffer_panic_threshold: Duration::from_secs(3),
            buffer_seek_threshold: Duration::from_secs(45),
            min_bandwidth_samples: 3,
        }
    }

    pub fn record_segment_download(
        &mut self,
        segment_size: u32,
        download_duration: Duration,
        segment_duration: Duration,
    ) {
        let now: Instant = Instant::now();
        
        let bandwidth: u32 = if download_duration.as_millis() > 0 {
            (segment_size as f64 / download_duration.as_secs_f64()) as u32
        } else {
            u32::MAX // Instantaneous download
        };
        
        self.bandwidth_history.push_back((now, bandwidth));
        
        self.cleanup_bandwidth_history(now);
        
        let segment_info: SegmentInfo = SegmentInfo {
            quality_level: self.current_quality,
            size_bytes: segment_size,
            duration: segment_duration,
            download_time: download_duration,
        };
        
        self.segment_history.push_back(segment_info);
        if self.segment_history.len() > 50 {
            self.segment_history.pop_front();
        }
        
        self.buffer_state.current_level += segment_duration;
        if self.buffer_state.current_level > self.buffer_state.max_level {
            self.buffer_state.current_level = self.buffer_state.max_level;
        }
    }

    /// Update buffer level after playback consumption
    pub fn update_buffer_consumption(&mut self, consumed_duration: Duration) {
        if self.buffer_state.current_level >= consumed_duration {
            self.buffer_state.current_level -= consumed_duration;
        } else {
            self.buffer_state.current_level = Duration::from_secs(0);
        }
    }

    pub fn get_next_quality(&mut self) -> usize {
        let estimated_bandwidth: u32 = self.estimate_bandwidth();
        
        // Buffer-based adaptation
        let buffer_factor: f64 = self.calculate_buffer_factor();
        
        // Apply buffer factor to bandwidth estimate
        let effective_bandwidth: u32 = (estimated_bandwidth as f64 * buffer_factor) as u32;
        
        // Find the highest quality that fits within the effective bandwidth
        let target_quality: usize = self.find_suitable_quality(effective_bandwidth);
        
        // Apply smoothing to avoid oscillations
        let next_quality: usize = self.apply_quality_smoothing(target_quality);
        
        self.current_quality = next_quality;
        next_quality
    }

    fn estimate_bandwidth(&self) -> u32 {
        if self.bandwidth_history.len() < self.min_bandwidth_samples {
            // Not enough samples, use conservative estimate based on current quality
            return self.quality_levels[self.current_quality].bitrate / 8; // Convert to bytes/sec
        }

        let harmonic_mean: u32 = self.calculate_harmonic_mean_bandwidth();
        let weighted_average: u32 = self.calculate_weighted_average_bandwidth();
        let percentile_estimate: u32 = self.calculate_percentile_bandwidth(0.2); // 20th percentile for conservative estimate
        
        // Use the minimum of these estimates for robustness
        harmonic_mean.min(weighted_average).min(percentile_estimate)
    }

    fn calculate_harmonic_mean_bandwidth(&self) -> u32 {
        let sum_reciprocals: f64 = self.bandwidth_history
            .iter()
            .map(|(_, bw)| 1.0 / (*bw as f64).max(1.0))
            .sum();
        
        (self.bandwidth_history.len() as f64 / sum_reciprocals) as u32
    }

    fn calculate_weighted_average_bandwidth(&self) -> u32 {
        let now: Instant = Instant::now();
        let mut weighted_sum: f64 = 0.0;
        let mut weight_sum: f64 = 0.0;
        
        for (timestamp, bandwidth) in &self.bandwidth_history {
            let age = now.duration_since(*timestamp).as_secs_f64();
            let weight = (-age / self.bandwidth_window.as_secs_f64()).exp();
            
            weighted_sum += *bandwidth as f64 * weight;
            weight_sum += weight;
        }
        
        if weight_sum > 0.0 {
            (weighted_sum / weight_sum) as u32
        } else {
            0
        }
    }

    fn calculate_percentile_bandwidth(&self, percentile: f64) -> u32 {
        let mut bandwidths: Vec<u32> = self.bandwidth_history
            .iter()
            .map(|(_, bw)| *bw)
            .collect();
        
        if bandwidths.is_empty() {
            return 0;
        }
        
        bandwidths.sort_unstable();
        let index: usize = (bandwidths.len() as f64 * percentile) as usize;
        bandwidths[index.min(bandwidths.len() - 1)]
    }

    fn calculate_buffer_factor(&self) -> f64 {
        let current_buffer: f64 = self.buffer_state.current_level.as_secs_f64();
        let target_buffer: f64 = self.buffer_state.target_level.as_secs_f64();
        let panic_threshold: f64 = self.buffer_panic_threshold.as_secs_f64();
        let seek_threshold: f64 = self.buffer_seek_threshold.as_secs_f64();
        
        if current_buffer < panic_threshold {
            // Buffer panic: be very conservative
            0.3
        } else if current_buffer < target_buffer {
            // Below target: be somewhat conservative
            0.6 + 0.3 * (current_buffer / target_buffer)
        } else if current_buffer > seek_threshold {
            // Buffer seeking: can be more aggressive
            1.5
        } else {
            // Normal operation
            1.0
        }
    }

    fn find_suitable_quality(&self, available_bandwidth: u32) -> usize {
        let safe_bandwidth: u32 = (available_bandwidth as f64 * self.safety_factor as f64) as u32;
        
        // Find the highest quality that fits within safe bandwidth
        for (i, quality) in self.quality_levels.iter().enumerate().rev() {
            let required_bandwidth: u32 = quality.bitrate / 8; // Convert to bytes/sec
            if required_bandwidth <= safe_bandwidth {
                return i;
            }
        }
        
        // If no quality fits, return the lowest quality
        0
    }

    fn apply_quality_smoothing(&self, target_quality: usize) -> usize {
        let current = self.current_quality as i32;
        let target = target_quality as i32;
        let diff = target - current;
        
        // Limit quality changes to prevent oscillations
        let max_change = if self.buffer_state.current_level < self.buffer_panic_threshold {
            // In panic mode, allow immediate downgrade
            if diff < 0 { diff } else { 1 }
        } else {
            // Normal operation: limit changes
            diff.signum() * 1.min(diff.abs())
        };
        
        ((current + max_change).max(0) as usize).min(self.quality_levels.len() - 1)
    }

    fn cleanup_bandwidth_history(&mut self, now: Instant) {
        while let Some((timestamp, _)) = self.bandwidth_history.front() {
            if now.duration_since(*timestamp) > self.bandwidth_window {
                self.bandwidth_history.pop_front();
            } else {
                break;
            }
        }
    }

    pub fn get_current_quality(&self) -> &QualityLevel {
        &self.quality_levels[self.current_quality]
    }

    pub fn get_buffer_state(&self) -> &BufferState {
        &self.buffer_state
    }

    pub fn get_estimated_bandwidth(&self) -> u32 {
        self.estimate_bandwidth()
    }

    pub fn is_buffer_healthy(&self) -> bool {
        self.buffer_state.current_level >= self.buffer_state.min_level
    }

    pub fn should_pause_playback(&self) -> bool {
        self.buffer_state.current_level < Duration::from_secs(1)
    }
}

fn create_test_quality_levels() -> Vec<QualityLevel> {
    vec![
        QualityLevel {
            bitrate: 500_000,   // 500 kbps
            width: 640,
            height: 360,
            codec: "h264".to_string(),
        },
        QualityLevel {
            bitrate: 1_000_000, // 1 Mbps
            width: 1280,
            height: 720,
            codec: "h264".to_string(),
        },
        QualityLevel {
            bitrate: 2_500_000, // 2.5 Mbps
            width: 1920,
            height: 1080,
            codec: "h264".to_string(),
        },
        QualityLevel {
            bitrate: 5_000_000, // 5 Mbps
            width: 3840,
            height: 2160,
            codec: "h264".to_string(),
        },
    ]
}

fn main() {
    println!("Adaptive Bitrate Streaming Algorithm Demo");
    
    let quality_levels: Vec<QualityLevel> = create_test_quality_levels();
    let mut streamer: AdaptiveBitrateStreamer = AdaptiveBitrateStreamer::new(quality_levels);
    
    println!("Initial quality: {} ({}x{} @ {} kbps)", 
        streamer.current_quality,
        streamer.get_current_quality().width,
        streamer.get_current_quality().height,
        streamer.get_current_quality().bitrate / 1000
    );
    

    println!("\nSimulating segment downloads...");
    
    // Simulate a fast download (good network)
    streamer.record_segment_download(
        1_000_000, // 1MB segment
        Duration::from_millis(800), // Downloaded in 800ms
        Duration::from_secs(4), // 4-second segment
    );
    
    let next_quality = streamer.get_next_quality();
    println!("After fast download - Next quality: {} (estimated bandwidth: {} kbps)", 
        next_quality,
        streamer.get_estimated_bandwidth() * 8 / 1000
    );
    
    // Simulate a slow download (poor network)
    streamer.record_segment_download(
        500_000, // 500KB segment
        Duration::from_secs(3), // Downloaded in 3 seconds
        Duration::from_secs(4), // 4-second segment
    );
    
    let next_quality = streamer.get_next_quality();
    println!("After slow download - Next quality: {} (estimated bandwidth: {} kbps)", 
        next_quality,
        streamer.get_estimated_bandwidth() * 8 / 1000
    );
    
    let buffer = streamer.get_buffer_state();
    println!("\nBuffer state:");
    println!("  Current level: {:.1}s", buffer.current_level.as_secs_f64());
    println!("  Target level: {:.1}s", buffer.target_level.as_secs_f64());
    println!("  Buffer healthy: {}", streamer.is_buffer_healthy());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_quality_selection() {
        let quality_levels = create_test_quality_levels();
        let streamer = AdaptiveBitrateStreamer::new(quality_levels);
        
        // Should start with middle quality (index 2 for 4 levels)
        assert_eq!(streamer.current_quality, 2);
    }

    #[test]
    fn test_bandwidth_recording() {
        let mut streamer = AdaptiveBitrateStreamer::new(create_test_quality_levels());
        
        // Record a segment download
        streamer.record_segment_download(
            1_000_000, // 1MB segment
            Duration::from_secs(1), // Downloaded in 1 second
            Duration::from_secs(4), // 4-second segment
        );
        
        assert_eq!(streamer.bandwidth_history.len(), 1);
        assert_eq!(streamer.segment_history.len(), 1);
    }
}