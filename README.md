# Adaptive Bitrate Streaming in Rust

This project implements a simplified **Adaptive Bitrate Streaming (ABR)** algorithm using Rust. It simulates the behavior of a streaming client dynamically adjusting video quality based on real-time network conditions and buffer health.

## Features

- Multiple quality levels with different bitrates and resolutions
- Real-time bandwidth estimation using:
  - Harmonic mean
  - Weighted average (decay-based)
  - Percentile estimate
- Buffer-aware bitrate adaptation logic
- Smoothing to reduce quality oscillation
- Simulation of segment downloads and playback behavior
- Basic CLI output for inspection
- Unit tests for core logic

## Tech Stack

- **Rust** for safe and performant system-level logic
- Uses `std::time` for timing and durations
- In-memory simulation (no actual media involved)

## Structure

- `QualityLevel`: Defines the video quality (bitrate, resolution, codec)
- `SegmentInfo`: Metadata for each video segment downloaded
- `BufferState`: Tracks current, target, and panic buffer levels
- `AdaptiveBitrateStreamer`: Core ABR logic including:
  - Bandwidth history and estimation
  - Quality level selection
  - Buffer-based adaptation
  - Quality smoothing
