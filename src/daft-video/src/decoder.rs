use ffmpeg_next as ffmpeg;
use image::{DynamicImage, RgbImage};
use tokio::{sync::mpsc, task::spawn_blocking};

use crate::{buffer, VideoError, VideoFrame, VideoMetadata, VideoSource};

pub struct FfmpegDecoder {
    rx: mpsc::UnboundedReceiver<Result<VideoFrame, VideoError>>,
    meta: VideoMetadata,
}

impl FfmpegDecoder {
    pub async fn new(src: VideoSource) -> Result<Self, VideoError> {
        ffmpeg_next::init().map_err(|_| VideoError::InitFailed)?;
        let copied = src.clone();
        let meta = spawn_blocking(move || Self::detect_metadata(copied))
            .await
            .map_err(|e| VideoError::FFmpeg(e.to_string()))??;

        let (tx, rx) = mpsc::unbounded_channel();
        spawn_blocking(move || Self::decode_frame(src, tx));
        Ok(Self { meta, rx })
    }

    fn decode_frame(src: VideoSource, tx: mpsc::UnboundedSender<Result<VideoFrame, VideoError>>) {
        let _ = || -> Result<(), VideoError> {
            let mut ictx = match src {
                VideoSource::File(p) => ffmpeg_next::format::input(&p),
                VideoSource::Bytes(b) => buffer::input(b, None),
            }
            .map_err(|e| VideoError::FFmpeg(e.to_string()))?;

            let st_idx = ictx
                .streams()
                .best(ffmpeg_next::media::Type::Video)
                .ok_or(VideoError::NoVideoStream)?
                .index();

            let mut dec = ffmpeg_next::codec::context::Context::from_parameters(
                ictx.streams()
                    .best(ffmpeg_next::media::Type::Video)
                    .unwrap()
                    .parameters(),
            )
            .and_then(|c| c.decoder().video())
            .map_err(|e| VideoError::FFmpeg(e.to_string()))?;

            let mut scaler = ffmpeg_next::software::scaling::Context::get(
                dec.format(),
                dec.width(),
                dec.height(),
                ffmpeg_next::format::Pixel::RGB24,
                dec.width(),
                dec.height(),
                ffmpeg_next::software::scaling::Flags::FAST_BILINEAR,
            )
            .map_err(|e| VideoError::FFmpeg(e.to_string()))?;

            let mut decoded = ffmpeg_next::util::frame::Video::empty();
            let mut rgb = ffmpeg_next::util::frame::Video::empty();
            let mut cnt = 0u64;

            for (stream, pkt) in ictx.packets() {
                if stream.index() != st_idx {
                    continue;
                }
                dec.send_packet(&pkt)
                    .map_err(|e| VideoError::FFmpeg(e.to_string()))?;
                while dec.receive_frame(&mut decoded).is_ok() {
                    cnt += 1;
                    scaler
                        .run(&decoded, &mut rgb)
                        .map_err(|e| VideoError::FFmpeg(e.to_string()))?;

                    let ts = decoded
                        .pts()
                        .map(|pts| pts as f64 * f64::from(stream.time_base()))
                        .unwrap_or(0.0);

                    let img = RgbImage::from_raw(rgb.width(), rgb.height(), rgb.data(0).to_vec())
                        .ok_or(VideoError::DecodingFailed)?;

                    let frame = VideoFrame {
                        image: DynamicImage::ImageRgb8(img),
                        timestamp: ts,
                        frame_number: cnt,
                    };
                    if tx.send(Ok(frame)).is_err() {
                        return Ok(()); // 接收端关闭
                    }
                }
            }
            dec.send_eof().ok();
            Ok(())
        }();
    }

    pub fn detect_metadata(src: VideoSource) -> Result<VideoMetadata, VideoError> {
        let ictx = match src {
            VideoSource::File(p) => ffmpeg_next::format::input(&p),
            VideoSource::Bytes(b) => buffer::input(b, None),
        }
        .map_err(|e| VideoError::FFmpeg(e.to_string()))?;

        let st = ictx
            .streams()
            .best(ffmpeg_next::media::Type::Video)
            .ok_or(VideoError::NoVideoStream)?;

        let dec = ffmpeg_next::codec::context::Context::from_parameters(st.parameters())
            .and_then(|c| c.decoder().video())
            .map_err(|e| VideoError::FFmpeg(e.to_string()))?;

        let fps = st.avg_frame_rate();
        let fps = if fps.denominator() != 0 {
            fps.numerator() as f64 / fps.denominator() as f64
        } else {
            30.0
        };

        Ok(VideoMetadata {
            fps,
            frames: Some(st.frames() as u64),
            width: dec.width(),
            height: dec.height(),
            duration: ictx.duration() as f64 / 1_000_000.0,
            pix_fmt: dec
                .format()
                .descriptor()
                .map(|d| d.name().to_string())
                .unwrap_or_else(|| "unknown".into()),
            codec: dec
                .codec()
                .map(|c| c.name().to_string())
                .unwrap_or_else(|| "unknown".into()),
        })
    }

    fn run_sync(src: VideoSource) -> Result<Option<VideoFrame>, VideoError> {
        ffmpeg::init().map_err(|_| VideoError::InitFailed)?;

        let mut ictx = match src {
            VideoSource::File(p) => ffmpeg::format::input(&p),
            VideoSource::Bytes(b) => super::buffer::input(b, None),
        }
        .map_err(|e| VideoError::FFmpeg(e.to_string()))?;

        let st_idx = ictx
            .streams()
            .best(ffmpeg_next::media::Type::Video)
            .ok_or(VideoError::NoVideoStream)?
            .index();

        let mut dec = ffmpeg_next::codec::context::Context::from_parameters(
            ictx.streams()
                .best(ffmpeg_next::media::Type::Video)
                .unwrap()
                .parameters(),
        )
        .and_then(|c| c.decoder().video())
        .map_err(|e| VideoError::FFmpeg(e.to_string()))?;

        let mut scaler = ffmpeg_next::software::scaling::Context::get(
            dec.format(),
            dec.width(),
            dec.height(),
            ffmpeg_next::format::Pixel::RGB24,
            dec.width(),
            dec.height(),
            ffmpeg_next::software::scaling::Flags::FAST_BILINEAR,
        )
        .map_err(|e| VideoError::FFmpeg(e.to_string()))?;

        let mut decoded = ffmpeg::util::frame::Video::empty();
        let mut rgb = ffmpeg::util::frame::Video::empty();
        let mut cnt = 0u64;

        for (stream, pkt) in ictx.packets() {
            if stream.index() != st_idx {
                continue;
            }
            dec.send_packet(&pkt)
                .map_err(|e| VideoError::FFmpeg(e.to_string()))?;
            while dec.receive_frame(&mut decoded).is_ok() {
                cnt += 1;
                scaler
                    .run(&decoded, &mut rgb)
                    .map_err(|e| VideoError::FFmpeg(e.to_string()))?;

                let ts = decoded
                    .pts()
                    .map(|pts| pts as f64 * f64::from(stream.time_base()))
                    .unwrap_or(0.0);

                let img = image::RgbImage::from_raw(
                    rgb.width() as u32,
                    rgb.height() as u32,
                    rgb.data(0).to_vec(),
                )
                .ok_or(VideoError::DecodingFailed)?;

                return Ok(Some(VideoFrame {
                    image: image::DynamicImage::ImageRgb8(img),
                    timestamp: ts,
                    frame_number: cnt,
                }));
            }
        }
        Ok(None)
    }

    pub fn metadata(&self) -> &VideoMetadata {
        &self.meta
    }

    pub async fn next_frame(&mut self) -> Result<Option<VideoFrame>, VideoError> {
        self.rx.recv().await.transpose()
    }
}

#[cfg(test)]
mod tests {
    use crate::{decoder::FfmpegDecoder, VideoSource};

    #[tokio::test]
    async fn test_decoder() -> Result<(), Box<dyn std::error::Error>> {
        let mut dec = FfmpegDecoder::new(VideoSource::File(
            "/Users/bytedance/Downloads/sample.mp4".into(),
        ))
        .await?;
        println!("codec = {}", dec.metadata());

        while let Some(vf) = dec.next_frame().await? {
            println!("frame {} @ {:.3}s", vf.frame_number, vf.timestamp);
            // vf.image.save(format!("frame_{}.png", vf.frame_number))?;
        }

        Ok(())
    }
}
