use std::path::{Path, PathBuf};

use ffmpeg_next as ffmpeg;

#[derive(Debug, Default, PartialEq, Clone)]
pub struct Preview {
    pub seek: i64,
    pub input: String,
}

impl Preview {
    pub async fn decode_preview_image(self) -> Option<Vec<u8>> {
        if let Ok(mut ictx) = ffmpeg::format::input(&self.input)
            .inspect_err(|e| eprintln!("failed to open '{}' with ffmpeg: {e}", self.input))
            && let Ok(input) = ictx
                .streams()
                .best(ffmpeg_next::media::Type::Video)
                .ok_or(ffmpeg::Error::StreamNotFound)
                .inspect_err(|e| eprintln!("Failed to find video stream: {e}"))
            && let Ok(context_decoder) =
                ffmpeg::codec::context::Context::from_parameters(input.parameters())
                    .inspect_err(|e| eprintln!("failed to get context decoder: {e}"))
            && let Ok(mut decoder) = context_decoder
                .decoder()
                .video()
                .inspect_err(|e| eprintln!("failed to get final decoder: {e}"))
            && let Ok(mut scalar) = ffmpeg::software::scaling::Context::get(
                decoder.format(),
                decoder.width(),
                decoder.height(),
                ffmpeg::format::Pixel::RGB24,
                decoder.width(),
                decoder.height(),
                ffmpeg::software::scaling::Flags::BILINEAR,
            )
            .inspect_err(|e| eprintln!("failed to get scalar of created decoder: {e}"))
        {
            let target_stream = input.index();
            let mut decoded = ffmpeg::util::frame::video::Video::empty();
            let mut rgb_frame = ffmpeg::util::frame::video::Video::empty();

            if ictx
                .seek(self.seek, i64::MIN..i64::MAX)
                .inspect_err(|e| eprintln!("failed to seek to '{}': {e}", self.seek))
                .is_ok()
            {
                for packet in ictx.packets().filter_map(|(stream, packet)| {
                    if stream.index() == target_stream {
                        Some(packet)
                    } else {
                        None
                    }
                }) {
                    if decoder
                        .send_packet(&packet)
                        .inspect_err(|e| eprintln!("decoder failed to send packet: {e}"))
                        .is_ok()
                        && decoder
                            .receive_frame(&mut decoded)
                            .inspect_err(|e| {
                                eprintln!("decoder failed to recieve frame (likely benign): {e}")
                            })
                            .is_ok()
                        && scalar
                            .run(&decoded, &mut rgb_frame)
                            .inspect_err(|e| eprintln!("failed to scale rgb_frame: {e}"))
                            .is_ok()
                    {
                        let mut buf = Vec::new();

                        // copy the PPM signature
                        buf.extend_from_slice(
                            format!("P6\n{} {}\n255\n", rgb_frame.width(), rgb_frame.height())
                                .as_bytes(),
                        );
                        buf.extend_from_slice(rgb_frame.data(0));

                        // write output to a file (for debugging)
                        // use std::{fs::File, io::Write};
                        // if let Ok(mut file) =
                        //     File::create_new(format!("/tmp/frame{}.ppm", self.seek))
                        //         .inspect_err(|e| eprintln!("failed to create file: {e}"))
                        // {
                        //     match file.write_all(&buf) {
                        //         Ok(_) => println!("successfully wrote to file"),
                        //         Err(e) => eprintln!("failed to write to file: {e}"),
                        //     }
                        // }

                        return Some(buf);
                    }
                }
            }
        }

        None
    }
}

pub async fn pick_file() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .pick_file()
        .await
        .and_then(|file| Some(file.path().to_path_buf()))
}
pub async fn pick_folder() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .pick_folder()
        .await
        .and_then(|file| Some(file.path().to_path_buf()))
}

/// returns Ok((len, has_video, has_audio))
pub fn get_video_params<P: AsRef<Path> + ?Sized>(
    path: &P,
) -> Result<(f64, bool, bool), ffmpeg::Error> {
    // try to load the media
    let context = ffmpeg::format::input(path)?;

    // get the media length
    let len = context.duration() as f64 / f64::from(ffmpeg::ffi::AV_TIME_BASE);

    let mut streams = context.streams();

    let has_video = streams
        .find(|stream| stream.parameters().medium() == ffmpeg::media::Type::Video)
        .is_some();

    let has_audio = streams
        .find(|stream| stream.parameters().medium() == ffmpeg::media::Type::Audio)
        .is_some();

    Ok((len, has_video, has_audio))
}
