use std::{
    env,
    path::{Path, PathBuf},
};

use ffmpeg_next as ffmpeg;

use iced::{
    Color, Element, Event, Length, Subscription, Task, Theme,
    alignment::{Horizontal, Vertical},
    color, event,
    keyboard::{self, Key, key},
    widget::{
        Image, button, checkbox, column,
        image::Handle,
        operation::{self, focus_next},
        row, slider, text, text_input,
    },
    window,
};

mod files;
use files::*;

#[derive(Debug, Clone)]
enum Message {
    InputChange(String),
    OutputChange(String, bool),

    PickInput,
    PickOutput,
    InputPicked(Option<PathBuf>),
    OutputPicked(Option<PathBuf>),

    StartChange(f64),
    EndChange(f64),

    ToggleVideo,
    ToggleAudio,

    Submitted,

    Update,

    LoadedStartPreview(Result<Vec<u8>, String>),
    LoadedEndPreview(Result<Vec<u8>, String>),

    Event(Event),

    Instantiate,
    InstantiateFinished(Result<(), String>),
}

#[derive(Debug, Default)]
struct State {
    input: String,
    input_changed: bool,
    input_exists: bool,

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
    output_folder_exists: bool,

    error: String,
    status: String,
}

impl State {
    fn new() -> (Self, Task<Message>) {
        ffmpeg::init().unwrap();

        let state = State::default();

        // Uses the first argument as the input file path,
        // and creates the output file path from it
        let mut args = env::args();
        if let Some(str) = args.nth(1) {
            (
                state,
                Task::done(Message::InputChange(str)).chain(Task::done(Message::Update)),
            )
        } else {
            (state, Task::none())
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::InputChange(str) => {
                self.input = str;
                self.input_changed = true;
                if let Ok(exists) = Path::new(&self.input).try_exists().inspect_err(|e| {
                    eprintln!("failed to check if input '{}' exists: {e}", self.input)
                }) {
                    self.input_exists = exists;
                }
            }
            Message::OutputChange(str, is_generated) => {
                self.output = str;
                self.output_is_generated = is_generated;
                if let Some(path) = Path::new(&self.output).parent()
                    && let Ok(exists) = path
                        .try_exists()
                        .inspect_err(|e| eprintln!("failed to check if input exists: {e}"))
                {
                    self.output_folder_exists = exists;
                }
            }
            Message::StartChange(val) => {
                self.start = val;
                self.number_changed = true;
            }
            Message::EndChange(val) => {
                self.end = val;
                self.number_changed = true;
            }

            Message::PickInput => return Task::perform(pick_file(), Message::InputPicked),
            Message::PickOutput => return Task::perform(pick_folder(), Message::OutputPicked),
            Message::InputPicked(opt) => {
                if let Some(path) = opt
                    && let Some(str) = path.to_str()
                {
                    return Task::done(Message::InputChange(str.to_owned()))
                        .chain(Task::done(Message::Update));
                }
            }
            Message::OutputPicked(opt) => {
                if let Some(mut path) = opt {
                    // push instead of setting filename
                    // since picked folder is interpreted as the filename here
                    path.push(Path::new(&self.output).file_name().unwrap_or_default());
                    if let Some(str) = path.to_str() {
                        return Task::done(Message::OutputChange(str.to_owned(), false));
                    }
                }
            }

            Message::Submitted => return Task::batch([focus_next(), self.check_inputs()]),
            Message::Update => return self.check_inputs(),

            Message::ToggleVideo => self.use_video = !self.use_video,
            Message::ToggleAudio => self.use_audio = !self.use_audio,

            Message::LoadedStartPreview(Ok(bytes)) => {
                self.start_preview = Some(Handle::from_bytes(bytes))
            }
            Message::LoadedEndPreview(Ok(bytes)) => {
                self.end_preview = Some(Handle::from_bytes(bytes))
            }
            Message::LoadedStartPreview(Err(e)) | Message::LoadedEndPreview(Err(e)) => {
                eprintln!("{}", e)
            }

            Message::Event(event) => {
                if let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event {
                    match key.as_ref() {
                        // input field cycling
                        Key::Named(key::Named::Tab) => {
                            if modifiers.shift() {
                                return operation::focus_previous();
                            } else {
                                return operation::focus_next();
                            }
                        }

                        Key::Character("v") => return Task::done(Message::ToggleVideo),
                        Key::Character("a") => return Task::done(Message::ToggleAudio),

                        // early-exit hotkeys
                        Key::Named(key::Named::Escape) | Key::Character("q") => {
                            return window::latest().and_then(window::close);
                        }

                        Key::Named(key::Named::Enter) => {
                            if modifiers.shift() {
                                return Task::done(Message::Instantiate);
                            } else {
                                return focus_next();
                            }
                        }

                        _ => {}
                    }
                }
            }

            Message::Instantiate => {
                self.error.clear();
                self.status = "Loading...".to_string();
                return self.instantiate();
            }
            Message::InstantiateFinished(result) => match result {
                Ok(()) => {
                    self.status = "Finished".to_string();
                    return window::latest().and_then(window::close);
                }
                Err(e) => self.error = e,
            },
        }

        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let input_field = text_input("input file", &self.input)
            .on_input(Message::InputChange)
            .on_submit(Message::Submitted);
        let input_picker =
            button("pick file")
                .on_press(Message::PickInput)
                .style(if self.input_exists {
                    button::primary
                } else {
                    button::warning
                });

        let start_slider = slider(0_f64..=self.end - 1.0, self.start, Message::StartChange)
            .default(0)
            .on_release(Message::Update);
        let start_field = text_input("start", &self.start.to_string())
            .on_input(|str| Message::StartChange(str.parse().unwrap_or_default()))
            .width(200)
            .on_submit(Message::Submitted);

        let end_slider = slider(
            self.start + 1.0..=self.input_length,
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
            .on_input(|str| Message::OutputChange(str, false))
            .on_submit(Message::Submitted);
        let output_picker = button("pick folder").on_press(Message::PickOutput).style(
            if self.output_folder_exists {
                button::primary
            } else {
                button::warning
            },
        );

        let video_checkbox = checkbox(self.use_video).on_toggle(|_| Message::ToggleVideo);
        let space = text("             ");
        let audio_checkbox = checkbox(self.use_audio).on_toggle(|_| Message::ToggleAudio);

        let preview_row = if self.use_video
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
        };

        let status_display = if !self.error.is_empty() {
            row![text(&self.error).style(text::danger)]
        } else if !self.status.is_empty() {
            row![text(&self.status).style(text::primary)]
        } else {
            row![]
        };

        let instantiate_button = button("Instantiate!").on_press(Message::Instantiate);
        let duration_string = format!("Duration: {} seconds", self.end - self.start);

        #[rustfmt::skip]
        return column![
            row![input_field, input_picker],

            row![text("Start time (seconds):  "), start_field, start_slider]
                .align_y(Vertical::Center),

            row![text("End time (seconds):    "), end_field, end_slider]
                .align_y(Vertical::Center),

            row![text("Video stream:"), video_checkbox, space, text("Audio stream:"), audio_checkbox]
                .spacing(10)
                .align_y(Vertical::Center),

            row![output_field, output_picker],

            preview_row,

            status_display,

            row![text("Press Shift-Enter, or:"), instantiate_button, text(duration_string)]
                .spacing(10)
                .align_y(Vertical::Center)
        ]
        .spacing(20)
        .align_x(Horizontal::Center)
        .into();
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
                Ok(task) => {
                    tasks.push(task);
                    tasks.push(self.create_preview_images());
                }
            }

            self.input_changed = false;
        } else if self.output.is_empty() && !self.output_is_generated {
            tasks.push(self.generate_output_path());
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

    fn update_from_input(&mut self) -> Result<Task<Message>, ffmpeg::Error> {
        if !self.input_exists {
            eprintln!("input_exists is set to false, not attempting to update from input");
            return Err(ffmpeg::Error::Unknown);
        }

        (self.input_length, self.use_video, self.use_audio) = get_video_params(&self.input)?;

        // Set the end to the duration of the video
        self.end = self.input_length;

        // Generate a template output path if there is none from user input
        if self.output.is_empty() || self.output_is_generated {
            Ok(self.generate_output_path())
        } else {
            Ok(Task::none())
        }
    }

    fn generate_output_path(&mut self) -> Task<Message> {
        let input_path = PathBuf::from(&self.input);

        Task::perform(modify_path(input_path), |path| {
            Message::OutputChange(
                path.into_os_string().into_string().unwrap_or_default(),
                true,
            )
        })
    }

    fn instantiate(&self) -> Task<Message> {
        Task::perform(
            Video {
                seek: self.start.to_string(),
                dur: (self.end - self.start).to_string(),

                input: self.input.clone(),
                output: self.output.clone(),

                copy_video: self.use_video,
                copy_audio: self.use_audio,
            }
            .create(),
            Message::InstantiateFinished,
        )
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

fn main() -> Result<(), iced::Error> {
    iced::application(State::new, State::update, State::view)
        .subscription(State::subscription)
        .theme(Theme::custom(
            "custom",
            iced::theme::Palette {
                background: color!(0x080808),
                text: Color::WHITE,
                primary: color!(0x00ffff),
                success: color!(0x00ff00),
                warning: color!(0x880000),
                danger: color!(0xff0000),
            },
        ))
        .run()?;

    Ok(())
}
