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
    task::{self},
    widget::{
        Image, button, checkbox, column, image,
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
    SliderStartChange(f64),
    SliderEndChange(f64),

    ToggleVideo,
    ToggleAudio,
    ToggleSubs,
    ToggleAllStreams,

    Submitted,

    Update,

    LoadedStartPreview(Result<(image::Handle, u64), String>),
    LoadedEndPreview(Result<(image::Handle, u64), String>),

    Event(Event),

    Instantiate,
    InstantiateFinished(Result<(), String>),
}

#[derive(Debug, Default)]
struct State {
    media: Media,

    input_changed: bool,
    input_exists: bool,

    input_length: f64,

    end: f64,
    number_changed: bool,

    last_start_preview: Preview,
    last_end_preview: Preview,

    last_start_preview_hash: u64,
    last_end_preview_hash: u64,

    start_preview: Option<image::Handle>,
    end_preview: Option<image::Handle>,

    start_preview_task_handle: Option<task::Handle>,
    end_preview_task_handle: Option<task::Handle>,

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
                self.media.input = str;
                self.input_changed = true;
                if let Ok(exists) = Path::new(&self.media.input).try_exists().inspect_err(|e| {
                    eprintln!(
                        "failed to check if input '{}' exists: {e}",
                        self.media.input
                    )
                }) {
                    self.input_exists = exists;
                }
            }
            Message::OutputChange(str, is_generated) => {
                self.media.output = str;
                self.output_is_generated = is_generated;
                if let Some(path) = Path::new(&self.media.output).parent()
                    && let Ok(exists) = path
                        .try_exists()
                        .inspect_err(|e| eprintln!("failed to check if input exists: {e}"))
                {
                    self.output_folder_exists = exists;
                }
            }
            Message::StartChange(val) => {
                self.media.start = val;
                self.number_changed = true;
                self.media.dur = self.end - self.media.start;
            }
            Message::EndChange(val) => {
                self.end = val;
                self.number_changed = true;
                self.media.dur = self.end - self.media.start;
            }

            Message::SliderStartChange(val) => {
                self.media.start = val;
                self.number_changed = true;
                self.media.dur = self.end - self.media.start;
                return self.check_inputs();
            }
            Message::SliderEndChange(val) => {
                self.end = val;
                self.number_changed = true;
                self.media.dur = self.end - self.media.start;
                return self.check_inputs();
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
                    path.push(
                        Path::new(&self.media.output)
                            .file_name()
                            .unwrap_or_default(),
                    );
                    if let Some(str) = path.to_str() {
                        return Task::done(Message::OutputChange(str.to_owned(), false));
                    }
                }
            }

            Message::Submitted => return Task::batch([focus_next(), self.check_inputs()]),
            Message::Update => return self.check_inputs(),

            Message::ToggleVideo => self.media.use_video = !self.media.use_video,
            Message::ToggleAudio => self.media.use_audio = !self.media.use_audio,
            Message::ToggleSubs => self.media.use_subs = !self.media.use_subs,
            Message::ToggleAllStreams => self.media.use_all_streams = !self.media.use_all_streams,

            Message::LoadedStartPreview(Ok((handle, hash))) => {
                self.last_start_preview_hash = hash;
                self.start_preview = Some(handle)
            }
            Message::LoadedEndPreview(Ok((handle, hash))) => {
                self.last_end_preview_hash = hash;
                self.end_preview = Some(handle)
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
                        Key::Character("s") => {
                            return if modifiers.shift() {
                                Task::done(Message::ToggleAllStreams)
                            } else {
                                Task::done(Message::ToggleSubs)
                            };
                        }

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
        let input_field = text_input("input file", &self.media.input)
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

        let start_slider = slider(
            0_f64..=self.end - 1.0,
            self.media.start,
            Message::SliderStartChange,
        )
        .default(0);
        let start_field = text_input("start", &self.media.start.to_string())
            .on_input(|str| Message::StartChange(str.parse().unwrap_or_default()))
            .width(200)
            .on_submit(Message::Submitted);

        let end_slider = slider(
            self.media.start + 1.0..=self.input_length,
            self.end,
            Message::SliderEndChange,
        )
        .default(self.input_length);
        let end_field = text_input("end", &self.end.to_string())
            .on_input(|str| Message::EndChange(str.parse().unwrap_or_default()))
            .width(200)
            .on_submit(Message::Submitted);

        let output_field = text_input("output file", &self.media.output)
            .on_input(|str| Message::OutputChange(str, false))
            .on_submit(Message::Submitted);
        let output_picker = button("pick folder").on_press(Message::PickOutput).style(
            if self.output_folder_exists {
                button::primary
            } else {
                button::warning
            },
        );

        let video_checkbox = checkbox(self.media.use_video)
            .on_toggle(|_| Message::ToggleVideo)
            .label("video");
        let audio_checkbox = checkbox(self.media.use_audio)
            .on_toggle(|_| Message::ToggleAudio)
            .label("audio");
        let subs_checkbox = checkbox(self.media.use_subs)
            .on_toggle(|_| Message::ToggleSubs)
            .label("subtitles");
        let all_streams_checkbox = checkbox(self.media.use_all_streams)
            .on_toggle(|_| Message::ToggleAllStreams)
            .label("all streams");

        let preview_row = if self.media.use_video
            && let Some(h_start) = self.start_preview.clone()
            && let Some(h_end) = self.end_preview.clone()
        {
            row![
                Image::<image::Handle>::new(h_start)
                    .width(Length::Fill)
                    .height(Length::Fill),
                Image::<image::Handle>::new(h_end)
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
        let duration_string = format!("Duration: {} seconds", self.media.dur);

        #[rustfmt::skip]
        return column![
            row![input_field, input_picker],

            row![text("Start time (seconds):  "), start_field, start_slider]
                .align_y(Vertical::Center),

            row![text("End time (seconds):    "), end_field, end_slider]
                .align_y(Vertical::Center),

            row![video_checkbox, audio_checkbox, subs_checkbox, all_streams_checkbox]
                .spacing(100)
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
                Err(e) => eprintln!("failed to inspect input media '{}': {e}", self.media.input),
                Ok(task) => {
                    tasks.push(task);
                    tasks.push(self.create_preview_images());
                }
            }

            self.input_changed = false;
        } else if self.media.output.is_empty() && !self.output_is_generated {
            tasks.push(self.generate_output_path());
        }

        Task::batch(tasks)
    }

    fn clamp_numbers(&mut self) {
        if self.end > self.input_length {
            self.end = self.input_length;
        }

        if self.media.start > self.end {
            self.media.start = self.end;
        }

        if self.end < self.media.start {
            self.end = self.media.start;
        }
    }

    fn update_from_input(&mut self) -> Result<Task<Message>, ffmpeg::Error> {
        if !self.input_exists {
            eprintln!("input_exists is set to false, not attempting to update from input");
            return Err(ffmpeg::Error::Unknown);
        }

        (
            self.input_length,
            self.media.use_video,
            self.media.use_audio,
        ) = get_video_params(&self.media.input)?;

        // Set the end to the duration of the video
        self.end = self.input_length;

        // Generate a template output path if there is none from user input
        if self.media.output.is_empty() || self.output_is_generated {
            Ok(self.generate_output_path())
        } else {
            Ok(Task::none())
        }
    }

    fn generate_output_path(&mut self) -> Task<Message> {
        let input_path = PathBuf::from(&self.media.input);

        Task::perform(modify_path(input_path), |path| {
            Message::OutputChange(
                path.into_os_string().into_string().unwrap_or_default(),
                true,
            )
        })
    }

    fn instantiate(&self) -> Task<Message> {
        Task::perform(self.media.clone().create(), Message::InstantiateFinished)
    }

    /// makes a batch of tasks to create start and end preview images
    /// no effect if use_video is false
    fn create_preview_images(&mut self) -> Task<Message> {
        if !self.media.use_video {
            return Task::none();
        }

        let start_preview = Preview {
            seek: (self.media.start * 1_000_000.0).round() as i64,
            input: self.media.input.clone(),
            prev_hash: self.last_start_preview_hash,
        };
        let end_preview = Preview {
            seek: // seek slightly before the end of the video to get a frame
                (if self.end > self.input_length - 0.1 {
                    self.end - 0.5
                } else {
                    self.end
                } * 1_000_000.0).round() as i64,
            input: self.media.input.clone(),
            prev_hash: self.last_end_preview_hash,
        };

        Task::batch([
            if start_preview == self.last_start_preview {
                // No need to reload the same image
                Task::none()
            } else {
                self.last_start_preview = start_preview.clone();
                let (task, handle) = Task::perform(
                    start_preview.decode_preview_image(),
                    Message::LoadedStartPreview,
                )
                .abortable();
                if let Some(extra_handle) = &self.start_preview_task_handle {
                    extra_handle.abort();
                }
                self.start_preview_task_handle = Some(handle);

                task
            },
            if end_preview == self.last_end_preview {
                // No need to reload the same image
                Task::none()
            } else {
                self.last_end_preview = end_preview.clone();
                let (task, handle) = Task::perform(
                    end_preview.decode_preview_image(),
                    Message::LoadedEndPreview,
                )
                .abortable();
                if let Some(extra_handle) = &self.end_preview_task_handle {
                    extra_handle.abort();
                }
                self.end_preview_task_handle = Some(handle);

                task
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
        .window_size((1000, 600))
        .run()?;

    Ok(())
}
