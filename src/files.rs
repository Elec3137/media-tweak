use std::{
    ffi::OsStr,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
};

use iced::widget;
use smol::process::Command;

use ffmpeg_next as ffmpeg;

#[derive(Debug, Default, PartialEq, Clone)]
pub struct Preview {
    pub seek: i64,
    pub input: String,
    pub prev_hash: u64,
}

impl Preview {
    pub async fn decode_preview_image(self) -> Result<(widget::image::Handle, u64), String> {
        let mut ictx = ffmpeg::format::input(&self.input)
            .map_err(|e| format!("failed to open '{}' with ffmpeg: {e}", self.input))?;

        let input = ictx
            .streams()
            .best(ffmpeg_next::media::Type::Video)
            .ok_or(ffmpeg::Error::StreamNotFound)
            .map_err(|e| format!("Failed to find video stream: {e}"))?;

        let context_decoder = ffmpeg::codec::context::Context::from_parameters(input.parameters())
            .map_err(|e| format!("failed to get context decoder: {e}"))?;

        let mut decoder = context_decoder
            .decoder()
            .video()
            .map_err(|e| format!("failed to get final decoder: {e}"))?;

        let mut scalar = ffmpeg::software::scaling::Context::get(
            decoder.format(),
            decoder.width(),
            decoder.height(),
            ffmpeg::format::Pixel::RGB24,
            decoder.width(),
            decoder.height(),
            ffmpeg::software::scaling::Flags::BILINEAR,
        )
        .map_err(|e| format!("failed to get scalar of created decoder: {e}"))?;

        let target_stream = input.index();
        let mut decoded = ffmpeg::util::frame::video::Video::empty();
        let mut rgb_frame = ffmpeg::util::frame::video::Video::empty();

        ictx.seek(self.seek, i64::MIN..i64::MAX)
            .map_err(|e| format!("failed to seek to '{}': {e}", self.seek))?;

        for packet in ictx.packets().filter_map(|(stream, packet)| {
            if stream.index() == target_stream {
                Some(packet)
            } else {
                None
            }
        }) {
            // skip empty packets
            if unsafe { packet.is_empty() } {
                continue;
            }

            let mut hasher = DefaultHasher::new();
            packet.data().hash(&mut hasher);
            let new_hash = hasher.finish();

            // make sure that the hash is different before decoding
            if new_hash == self.prev_hash {
                return Err(String::from(
                    "benign: identical hash of encoded packet, not decoding",
                ));
            }

            decoder
                .send_packet(&packet)
                .map_err(|e| format!("decoder failed to send packet: {e}"))?;

            if let Err(e) = decoder.receive_frame(&mut decoded) {
                match e {
                    // skip the rest of the loop on benign "Resource temporarily unavailable" error
                    ffmpeg::Error::Other { errno: 11 } => continue,
                    _ => eprintln!("decoder failed to recieve frame: {e}"),
                }
            }

            scalar
                .run(&decoded, &mut rgb_frame)
                .map_err(|e| format!("failed to scale rgb_frame: {e}"))?;

            dbg!(
                rgb_frame.width() * rgb_frame.height() * 4
                    == rgb_frame.data(0).len().try_into().unwrap()
            );
            eprintln!(
                "\n{} nonzero pixels\n",
                rgb_frame.data(0).iter().filter(|p| **p != 0).count()
            );
            let handle = dbg!(widget::image::Handle::from_rgba(
                rgb_frame.width(),
                rgb_frame.height(),
                rgb_frame.data(0).to_owned(),
            ));
            eprintln!();

            return Ok((handle, new_hash));
        }

        Err(String::from("No valid packets found"))
    }
}

#[derive(Debug, Default, PartialEq, Clone)]
pub struct Video {
    pub seek: String,
    pub dur: String,

    pub input: String,
    pub output: String,

    pub copy_video: bool,
    pub copy_audio: bool,
}

impl Video {
    pub async fn create(self) -> Result<(), String> {
        let mut args = vec!["-ss", &self.seek, "-t", &self.dur, "-i", &self.input];

        if self.copy_audio {
            args.push("-c:a");
            args.push("copy");
        } else {
            args.push("-an");
        }

        if self.copy_video {
            args.push("-c:v");
            args.push("copy");
        } else {
            args.push("-vn");
        }

        args.push(&self.output);

        match Command::new("ffmpeg").args(&args).spawn() {
            Err(e) => Err(e.to_string()),
            Ok(mut child) => match child.status().await {
                Err(e) => Err(e.to_string()),
                Ok(status) => {
                    if status.success() {
                        Ok(())
                    } else {
                        Err(format!(
                            "ffmpeg returned {status}. Check stderr for full error"
                        ))
                    }
                }
            },
        }
    }
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

    let has_video =
        streams.any(|stream| stream.parameters().medium() == ffmpeg::media::Type::Video);

    let has_audio =
        streams.any(|stream| stream.parameters().medium() == ffmpeg::media::Type::Audio);

    Ok((len, has_video, has_audio))
}

pub async fn pick_file() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .pick_file()
        .await
        .map(|file| file.path().to_path_buf())
}
pub async fn pick_folder() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .pick_folder()
        .await
        .map(|file| file.path().to_path_buf())
}

/// returns a path with a different filename
pub async fn modify_path(mut path: PathBuf) -> PathBuf {
    path.set_file_name(format!(
        "{}_edited.{}",
        path.file_stem()
            .unwrap_or_else(|| OsStr::new("media"))
            .to_str()
            .unwrap_or_else(|| {
                eprintln!("Failed to decode file_stem");
                ""
            }),
        path.extension()
            .unwrap_or_else(|| OsStr::new("mkv"))
            .to_str()
            .unwrap_or_else(|| {
                eprintln!("Failed to decode extension");
                ""
            })
    ));

    path
}
