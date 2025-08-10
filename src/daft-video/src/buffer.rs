use std::{
    io::{Cursor, Read},
    os::raw::{c_int, c_void},
};

use ffmpeg_next as ffmpeg;
use ffmpeg_next::{ffi, format::context, util::error::Error};

struct MemReader(Cursor<Vec<u8>>);

impl MemReader {
    fn new(buf: Vec<u8>) -> Self {
        Self(Cursor::new(buf))
    }
}

unsafe extern "C" fn read_packet(opaque: *mut c_void, buf: *mut u8, buf_size: c_int) -> c_int {
    let reader = &mut *(opaque as *mut MemReader);
    let dst = std::slice::from_raw_parts_mut(buf, buf_size as usize);
    match reader.0.read(dst) {
        Ok(n) => n as c_int,
        Err(_) => ffi::AVERROR(ffi::EIO),
    }
}

pub fn input(data: Vec<u8>, buf_len: Option<usize>) -> Result<context::Input, Error> {
    unsafe {
        let io_buf_len = buf_len.unwrap_or(0x10000);
        let io_buf = libc::malloc(io_buf_len) as *mut u8;
        if io_buf.is_null() {
            return Err(Error::from(ffi::AVERROR(ffi::ENOMEM)));
        }

        let mut reader = MemReader::new(data);
        let avio = ffi::avio_alloc_context(
            io_buf,
            io_buf_len as c_int,
            0,
            &mut reader as *mut _ as *mut c_void,
            Some(read_packet),
            None,
            None,
        );

        if avio.is_null() {
            libc::free(io_buf as *mut libc::c_void);
            return Err(ffmpeg::util::error::Error::from(ffi::AVERROR(ffi::ENOMEM)));
        }

        let mut ps = ffi::avformat_alloc_context();
        if ps.is_null() {
            ffi::av_freep(&mut (*avio).buffer as *mut _ as *mut _);
            return Err(Error::from(ffi::AVERROR(ffi::ENOMEM)));
        }

        (*ps).pb = avio;
        (*ps).flags |= ffmpeg::ffi::AVFMT_FLAG_CUSTOM_IO;

        match ffi::avformat_open_input(
            &mut ps,
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ) {
            0 => match ffi::avformat_find_stream_info(ps, std::ptr::null_mut()) {
                r if r >= 0 => Ok(context::Input::wrap(ps)),
                e => {
                    ffi::avformat_close_input(&mut ps);
                    Err(Error::from(e))
                }
            },
            e => {
                ffi::avformat_close_input(&mut ps);
                Err(Error::from(e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use ffmpeg_next::format::context::Input;

    use super::*;

    fn print_video_metadata(context: Input) -> Result<(), Box<dyn std::error::Error>> {
        for (k, v) in context.metadata().iter() {
            println!("{}: {}", k, v);
        }

        if let Some(stream) = context.streams().best(ffmpeg::media::Type::Video) {
            println!("Best video stream index: {}", stream.index());
        }

        if let Some(stream) = context.streams().best(ffmpeg::media::Type::Audio) {
            println!("Best audio stream index: {}", stream.index());
        }

        if let Some(stream) = context.streams().best(ffmpeg::media::Type::Subtitle) {
            println!("Best subtitle stream index: {}", stream.index());
        }

        println!(
            "duration (seconds): {:.2}",
            context.duration() as f64 / f64::from(ffmpeg::ffi::AV_TIME_BASE)
        );

        for stream in context.streams() {
            println!("stream index {}:", stream.index());
            println!("\ttime_base: {}", stream.time_base());
            println!("\tstart_time: {}", stream.start_time());
            println!("\tduration (stream timebase): {}", stream.duration());
            println!(
                "\tduration (seconds): {:.2}",
                stream.duration() as f64 * f64::from(stream.time_base())
            );
            println!("\tframes: {}", stream.frames());
            println!("\tdisposition: {:?}", stream.disposition());
            println!("\tdiscard: {:?}", stream.discard());
            println!("\trate: {}", stream.rate());

            let codec = ffmpeg::codec::context::Context::from_parameters(stream.parameters())?;
            println!("\tmedium: {:?}", codec.medium());
            println!("\tid: {:?}", codec.id());

            if codec.medium() == ffmpeg::media::Type::Video {
                if let Ok(video) = codec.decoder().video() {
                    println!("\tbit_rate: {}", video.bit_rate());
                    println!("\tmax_rate: {}", video.max_bit_rate());
                    println!("\tdelay: {}", video.delay());
                    println!("\tvideo.width: {}", video.width());
                    println!("\tvideo.height: {}", video.height());
                    println!("\tvideo.format: {:?}", video.format());
                    println!("\tvideo.has_b_frames: {}", video.has_b_frames());
                    println!("\tvideo.aspect_ratio: {}", video.aspect_ratio());
                    println!("\tvideo.color_space: {:?}", video.color_space());
                    println!("\tvideo.color_range: {:?}", video.color_range());
                    println!("\tvideo.color_primaries: {:?}", video.color_primaries());
                    println!(
                        "\tvideo.color_transfer_characteristic: {:?}",
                        video.color_transfer_characteristic()
                    );
                    println!("\tvideo.chroma_location: {:?}", video.chroma_location());
                    println!("\tvideo.references: {}", video.references());
                    println!("\tvideo.intra_dc_precision: {}", video.intra_dc_precision());
                }
            } else if codec.medium() == ffmpeg::media::Type::Audio {
                if let Ok(audio) = codec.decoder().audio() {
                    println!("\tbit_rate: {}", audio.bit_rate());
                    println!("\tmax_rate: {}", audio.max_bit_rate());
                    println!("\tdelay: {}", audio.delay());
                    println!("\taudio.rate: {}", audio.rate());
                    println!("\taudio.channels: {}", audio.channels());
                    println!("\taudio.format: {:?}", audio.format());
                    println!("\taudio.frames: {}", audio.frames());
                    println!("\taudio.align: {}", audio.align());
                    println!("\taudio.channel_layout: {:?}", audio.channel_layout());
                }
            }
        }

        Ok(())
    }

    #[test]
    fn test_read_from_memory() -> Result<(), Box<dyn std::error::Error>> {
        ffmpeg::init()?;
        let blob = std::fs::read("/Users/bytedance/Downloads/sample.mp4")?;
        let ctx = input(blob, None)?;

        print_video_metadata(ctx)?;
        let blob = std::fs::read("/Users/bytedance/Downloads/sample.mp4")?;
        let ctx = input(blob, None)?;

        println!("=== Container ===");
        println!("format  : {}", ctx.format().name());
        println!("duration: {:.3} s", ctx.duration() as f64 / 1_000_000.0);
        println!("bit_rate: {} bps", ctx.bit_rate());
        println!("metadata: {:?}", ctx.metadata());

        for st in ctx.streams() {
            let p = st.parameters();
            println!("=== Stream {} ({:?}) ===", st.index(), p.medium());
            println!("codec      : {}", p.id().name());
            // println!("bit_rate   : {:?} bps", p.bit_rate());
            if matches!(p.medium(), ffmpeg::util::media::Type::Video) {
                let codec_ctx = ffmpeg::codec::context::Context::from_parameters(p).unwrap();
                let video_decoder = codec_ctx.decoder().video().unwrap();
                println!(
                    "width/height: {}x{}",
                    video_decoder.width(),
                    video_decoder.height()
                );
                if st.avg_frame_rate().1 != 0 {
                    println!(
                        "fps        : {:.2}",
                        st.avg_frame_rate().0 as f64 / st.avg_frame_rate().1 as f64
                    );
                }
            }
        }

        Ok(())
    }
}
