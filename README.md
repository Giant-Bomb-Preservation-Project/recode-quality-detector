# Recode Quality Detector (rqd)
RQD is a tool that accepts a video file, and generates a matrix of possible
codecs and settings to encode the inputted video into. RQD then uses VMAF to
perform visual quality detection, and outputs a comparison of the VMAF score to
the size compression.

## Installation
RQD to be installed locally requires [Rust](https://rustup.rs).

```rust
cargo install --git https://github.com/Giant-Bomb-Preservation-Project/recode-quality-detector.git
```

## Usage
To use RQD you must already have `ffmpeg` available on the system. The only thing
required is providing the sample video to encode. When ran you'll be prompted to
select which codecs you'd like to select.

```
rqd sample.mp4
```
