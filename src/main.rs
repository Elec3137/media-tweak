use std::{
    env,
    error::Error,
    ffi::OsStr,
    path::PathBuf,
    process::{Child, Command},
};

use ffmpeg_next as ffmpeg;

use iced::{
    Color, Element, Event, Subscription, Task, Theme,
    alignment::Horizontal,
    event,
    keyboard::{self, Key, key},
    widget::{
        checkbox, column,
        operation::{self, focus_next},
        row, slider, text, text_input,
    },
    window,
};

#[derive(Debug, Clone)]
enum Message {
    InputChange(String),
    OutputChange(String),

    StartChange(f64),
    EndChange(f64),

    ToggleVideo,
    ToggleAudio,

    InputSubmitted,
    Submitted,

    Update,

    Event(Event),
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

    output: String,
    output_is_generated: bool,
}

impl State {
    fn new() -> Self {
        ffmpeg::init().unwrap();

        let mut state = State::default();

        // Uses the first argument as the input file path,
        // and creates the output file path from it
        let mut args = env::args();
        if let Some(str) = args.nth(1) {
            state.input = str;

            match state.update_from_input() {
                Err(e) => eprintln!("Failed to inspect video: {}: {e}", state.input),
                Ok(()) => {}
            }
        }

        state
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

            Message::InputSubmitted => {
                #[allow(unused_must_use)]
                self.update_from_input();
                self.input_changed = false;
                focus_next()
            }
            Message::Submitted => {
                self.check_inputs();
                focus_next()
            }
            Message::Update => {
                self.check_inputs();
                Task::none()
            }

            Message::ToggleVideo => {
                self.use_video = !self.use_video;
                Task::none()
            }
            Message::ToggleAudio => {
                self.use_audio = !self.use_audio;
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

                        // toggle video/audio
                        Key::Character("v") => {
                            self.use_video = !self.use_video;
                            Task::none()
                        }
                        Key::Character("a") => {
                            self.use_audio = !self.use_audio;
                            Task::none()
                        }

                        // early-exit hotkeys
                        Key::Named(key::Named::Escape) | Key::Character("q") => {
                            window::latest().and_then(window::close)
                        }

                        Key::Named(key::Named::Enter) => {
                            if modifiers.shift() {
                                #[allow(unused_must_use)]
                                self.instantiate();
                                window::latest().and_then(window::close)
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
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let input_field = text_input("input file", &self.input)
            .on_input(Message::InputChange)
            .on_submit(Message::InputSubmitted)
            .id("first");

        let start_slider = slider(0_f64..=self.end - 1.0, self.start, Message::StartChange)
            .default(0)
            .on_release(Message::Update);
        let start_field = text_input("start", &self.start.to_string())
            .on_input(|str| Message::StartChange(str.parse().unwrap_or_default()))
            .width(200)
            .on_submit(Message::Submitted);

        let end_slider = slider(
            self.start + 0.9..=self.input_length,
            self.end,
            Message::EndChange,
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

        column![
            input_field,
            row![text("Start time (seconds):  "), start_field, start_slider],
            row![text("End time (seconds):    "), end_field, end_slider],
            row![
                text("Video stream: "),
                video_checkbox,
                text("          Audio stream: "),
                audio_checkbox
            ]
            .spacing(10),
            output_field,
            text("Press Shift-Enter to execute")
        ]
        .spacing(20)
        .align_x(Horizontal::Center)
        .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        event::listen().map(Message::Event)
    }

    fn check_inputs(&mut self) {
        if self.number_changed {
            self.clamp_numbers();
        }
        if self.input_changed {
            #[allow(unused_must_use)]
            self.update_from_input();
        } else if self.output.is_empty() && !self.output_is_generated {
            self.generate_output_path();
        }
    }

    fn clamp_numbers(&mut self) {
        if self.end >= self.input_length {
            self.end = self.input_length;
        } else if self.end < self.start {
            self.end = self.start;
        }
        if self.start >= self.end {
            self.start = self.end;
        } else if self.start > self.input_length {
            self.start = self.input_length;
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
        let mut args = vec!["-i", &self.input];

        args.push("-ss");
        let start = self.start.to_string();
        args.push(&start);

        args.push("-t");
        let duration = (self.end - self.start).to_string();
        args.push(&duration);

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
