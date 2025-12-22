use std::{
    env,
    error::Error,
    ffi::OsStr,
    path::PathBuf,
    process::{Child, Command},
};

use ffmpeg_next as ffmpeg;

use iced::{
    Color, Element, Event, Length, Subscription, Task, Theme,
    alignment::{Horizontal, Vertical},
    event,
    keyboard::{self, Key, key},
    widget::{
        Image, button, checkbox, column,
        image::Handle,
        operation::{self, focus_next},
        row, slider, text, text_input,
    },
    window,
};

#[derive(Debug, Default, PartialEq, Clone)]
struct Preview {
    seek: i64,
    input: String,
}

impl Preview {
    async fn decode_preview_image(self) -> Option<Vec<u8>> {
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
                // We only look at the first packet
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
                            .inspect_err(|e| eprintln!("decoder failed to recieve frame: {e}"))
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

#[derive(Debug, Clone)]
enum Message {
    InputChange(String),
    OutputChange(String),

    StartChange(f64),
    EndChange(f64),
    StartSliderChange(f64),
    EndSliderChange(f64),

    ToggleVideo,
    ToggleAudio,

    Submitted,

    Update,

    LoadedStartPreview(Option<Vec<u8>>),
    LoadedEndPreview(Option<Vec<u8>>),

    Event(Event),

    Instantiate,
}

#[derive(Debug, Default)]
struct State {
    input: String,
    input_changed: bool,

    input_length: f64,

    start: f64,
    end: f64,
    number_changed: bool,

    use_video: bool,
    use_audio: bool,

    last_start_preview: Preview,
    last_end_preview: Preview,

    start_preview: Option<Handle>,
    end_preview: Option<Handle>,

    output: String,
    output_is_generated: bool,
}

impl State {
    fn new() -> (Self, Task<Message>) {
        ffmpeg::init().unwrap();

        let mut state = State::default();

        // Uses the first argument as the input file path,
        // and creates the output file path from it
        let mut args = env::args();
        if let Some(str) = args.nth(1) {
            state.input = str;

            if let Ok(()) = state
                .update_from_input()
                .inspect_err(|e| eprintln!("failed to inspect input media '{}': {e}", state.input))
            {
                let preview_tasks = state.create_preview_images();
                return (state, preview_tasks);
            }
        }

        (state, Task::none())
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::InputChange(str) => {
                self.input = str;
                self.input_changed = true;
                Task::none()
            }
            Message::OutputChange(str) => {
                self.output = str;
                self.output_is_generated = false;
                Task::none()
            }
            Message::StartChange(val) => {
                self.start = val;
                self.number_changed = true;
                Task::none()
            }
            Message::EndChange(val) => {
                self.end = val;
                self.number_changed = true;
                Task::none()
            }

            // we don't need to keep track of if number_changed
            // since we can be sure that the slider itself clamps inputs
            Message::StartSliderChange(val) => {
                self.start = val;
                self.create_preview_images()
            }
            Message::EndSliderChange(val) => {
                self.end = val;
                self.create_preview_images()
            }

            Message::Submitted => focus_next().chain(self.check_inputs()),
            Message::Update => self.check_inputs(),

            Message::ToggleVideo => {
                self.use_video = !self.use_video;
                Task::none()
            }
            Message::ToggleAudio => {
                self.use_audio = !self.use_audio;
                Task::none()
            }

            Message::LoadedStartPreview(o) => {
                if let Some(b) = o {
                    self.start_preview = Some(Handle::from_bytes(b));
                }
                Task::none()
            }
            Message::LoadedEndPreview(o) => {
                if let Some(b) = o {
                    self.end_preview = Some(Handle::from_bytes(b));
                }
                Task::none()
            }

            Message::Event(event) => {
                if let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event {
                    match key.as_ref() {
                        // input field cycling
                        Key::Named(key::Named::Tab) => {
                            if modifiers.shift() {
                                operation::focus_previous()
                            } else {
                                operation::focus_next()
                            }
                        }

                        Key::Character("v") => Task::done(Message::ToggleVideo),
                        Key::Character("a") => Task::done(Message::ToggleAudio),

                        // early-exit hotkeys
                        Key::Named(key::Named::Escape) | Key::Character("q") => {
                            window::latest().and_then(window::close)
                        }

                        Key::Named(key::Named::Enter) => {
                            if modifiers.shift() {
                                Task::done(Message::Instantiate)
                            } else {
                                focus_next()
                            }
                        }

                        _ => Task::none(),
                    }
                } else {
                    Task::none()
                }
            }

            Message::Instantiate => {
                self.instantiate()
                    .map_or_else(|e| eprintln!("failed to instantiate: {e}"), |_| {});
                window::latest().and_then(window::close)
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let input_field = text_input("input file", &self.input)
            .on_input(Message::InputChange)
            .on_submit(Message::Submitted)
            .id("first");

        let start_slider = slider(
            0_f64..=self.end - 1.0,
            self.start,
            Message::StartSliderChange,
        )
        .default(0)
        .on_release(Message::Update);
        let start_field = text_input("start", &self.start.to_string())
            .on_input(|str| Message::StartChange(str.parse().unwrap_or_default()))
            .width(200)
            .on_submit(Message::Submitted);

        let end_slider = slider(
            self.start + 0.9..=self.input_length,
            self.end,
            Message::EndSliderChange,
        )
        .default(self.input_length)
        .on_release(Message::Update);
        let end_field = text_input("end", &self.end.to_string())
            .on_input(|str| Message::EndChange(str.parse().unwrap_or_default()))
            .width(200)
            .on_submit(Message::Submitted);

        let output_field = text_input("output file", &self.output)
            .on_input(Message::OutputChange)
            .on_submit(Message::Submitted);

        let video_checkbox = checkbox(self.use_video).on_toggle(|_| Message::ToggleVideo);
        let audio_checkbox = checkbox(self.use_audio).on_toggle(|_| Message::ToggleAudio);

        let instantiate_button = button("Instantiate!").on_press(Message::Instantiate);

        column![
            input_field,
            row![text("Start time (seconds):  "), start_field, start_slider]
                .align_y(Vertical::Center),
            row![text("End time (seconds):    "), end_field, end_slider].align_y(Vertical::Center),
            row![
                text("Video stream: "),
                video_checkbox,
                text("          Audio stream: "),
                audio_checkbox
            ]
            .spacing(10)
            .align_y(Vertical::Center),
            output_field,
            if self.use_video
                && let Some(h_start) = self.start_preview.clone()
                && let Some(h_end) = self.end_preview.clone()
            {
                row![
                    Image::<Handle>::new(h_start)
                        .width(Length::Fill)
                        .height(Length::Fill),
                    Image::<Handle>::new(h_end)
                        .width(Length::Fill)
                        .height(Length::Fill)
                ]
            } else {
                row![]
            },
            row![text("Press Shift-Enter, or:"), instantiate_button]
                .spacing(10)
                .align_y(Vertical::Center)
        ]
        .spacing(20)
        .align_x(Horizontal::Center)
        .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        event::listen().map(Message::Event)
    }

    fn check_inputs(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();

        if self.number_changed {
            self.clamp_numbers();
            if !self.input_changed {
                tasks.push(self.create_preview_images());
            }

            self.number_changed = false;
        }
        if self.input_changed {
            match self.update_from_input() {
                Err(e) => eprintln!("failed to inspect input media '{}': {e}", self.input),
                Ok(()) => tasks.push(self.create_preview_images()),
            };

            self.input_changed = false;
        } else if self.output.is_empty() && !self.output_is_generated {
            self.generate_output_path();
        }

        Task::batch(tasks)
    }

    fn clamp_numbers(&mut self) {
        if self.end > self.input_length {
            self.end = self.input_length;
        }

        if self.start > self.end {
            self.start = self.end;
        }

        if self.end < self.start {
            self.end = self.start;
        }
    }

    fn update_from_input(&mut self) -> Result<(), ffmpeg::Error> {
        // Try to load the input
        let context = ffmpeg::format::input(&self.input)?;

        // set the input media length
        self.input_length = context.duration() as f64 / f64::from(ffmpeg::ffi::AV_TIME_BASE);

        // Check for audio and video streams and set them to be used if avaliable
        let mut streams = context.streams();
        if let Some(_video) =
            streams.find(|stream| stream.parameters().medium() == ffmpeg::media::Type::Video)
        {
            self.use_video = true;
        }
        if let Some(_audio) =
            streams.find(|stream| stream.parameters().medium() == ffmpeg::media::Type::Audio)
        {
            self.use_audio = true;
        }

        // Generate a template output path if there is none from user input
        if self.output.is_empty() || self.output_is_generated {
            self.generate_output_path();
        }

        // Set the end to the duration of the video
        self.end = self.input_length;

        Ok(())
    }

    fn generate_output_path(&mut self) {
        self.output_is_generated = true;

        let input_path = PathBuf::from(&self.input);

        self.output = input_path
            .with_file_name(format!(
                "{}_edited.{}",
                input_path
                    .file_stem()
                    .unwrap_or_else(|| OsStr::new("media"))
                    .to_str()
                    .unwrap_or_else(|| {
                        eprintln!("Failed to decode file_stem");
                        ""
                    }),
                input_path
                    .extension()
                    .unwrap_or_else(|| OsStr::new("mkv"))
                    .to_str()
                    .unwrap_or_else(|| {
                        eprintln!("Failed to decode extension");
                        ""
                    })
            ))
            .into_os_string()
            .into_string()
            .unwrap_or_default();
    }

    fn instantiate(&self) -> Result<Child, impl Error> {
        let mut args = vec!["-ss"];
        let start = self.start.to_string();
        args.push(&start);

        args.push("-t");
        let duration = (self.end - self.start).to_string();
        args.push(&duration);

        args.push("-i");
        args.push(&self.input);

        if self.use_audio {
            args.push("-c:a");
            args.push("copy");
        } else {
            args.push("-an");
        }

        if self.use_video {
            args.push("-c:v");
            args.push("copy");
        } else {
            args.push("-vn");
        }

        args.push(&self.output);

        eprintln!("{:#?}", args);
        Command::new("ffmpeg").args(args).spawn()
    }

    /// makes a batch of tasks to create start and end preview images
    /// no effect if use_video is false
    fn create_preview_images(&mut self) -> Task<Message> {
        if !self.use_video {
            return Task::none();
        }

        let start_preview = Preview {
            seek: (self.start * 1_000_000.0).round() as i64,
            input: self.input.clone(),
        };
        let end_preview = Preview {
            seek: // seek slightly before the end of the video to get a frame
                (if self.end > self.input_length - 0.1 {
                    self.end - 0.5
                } else {
                    self.end
                } * 1_000_000.0).round() as i64,
            input: self.input.clone(),
        };

        Task::batch([
            if start_preview == self.last_start_preview {
                // No need to reload the same image
                Task::none()
            } else {
                self.last_start_preview = start_preview.clone();
                Task::perform(
                    start_preview.decode_preview_image(),
                    Message::LoadedStartPreview,
                )
            },
            if end_preview == self.last_end_preview {
                // No need to reload the same image
                Task::none()
            } else {
                self.last_end_preview = end_preview.clone();
                Task::perform(
                    end_preview.decode_preview_image(),
                    Message::LoadedEndPreview,
                )
            },
        ])
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    iced::application(State::new, State::update, State::view)
        .subscription(State::subscription)
        .theme(Theme::custom(
            "custom",
            iced::theme::Palette {
                background: Color::from_rgb8(0x0f, 0x0f, 0x0f),
                text: Color::WHITE,
                primary: Color::from_rgb8(0, u8::MAX, u8::MAX),
                success: Color::from_rgb8(0, u8::MAX, 0),
                warning: Color::from_rgb8(128, 0, 0),
                danger: Color::from_rgb8(u8::MAX, 0, 0),
            },
        ))
        .run()?;

    Ok(())
}
