use image::DynamicImage;
use thiserror::Error;

pub mod buffer;
pub mod decoder;
pub mod functions;

#[derive(Debug, Clone)]
pub enum VideoSource {
    File(String),
    Bytes(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct VideoMetadata {
    pub fps: f64,
    pub frames: Option<u64>,
    pub width: u32,
    pub height: u32,
    pub duration: f64,
    pub pix_fmt: String,
    pub codec: String,
}

impl std::fmt::Display for VideoMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}x{} @ {:.2} fps, codec {}, pix_fmt {}, duration {:.2}s, frames {}",
            self.width,
            self.height,
            self.fps,
            self.codec,
            self.pix_fmt,
            self.duration,
            self.frames.unwrap_or(0)
        )
    }
}

#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub image: DynamicImage,
    pub timestamp: f64,
    pub frame_number: u64,
}

#[derive(Error, Debug)]
pub enum VideoError {
    #[error("Failed to initialize FFmpeg.")]
    InitFailed,

    #[error("Video stream not found.")]
    NoVideoStream,
    #[error("FFmpeg错误: {0}")]
    FFmpeg(String),
    #[error("Failed to decode stream")]
    DecodingFailed,
}
