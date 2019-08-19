// Spekje -- Spectrogram viewer
// Copyright 2019 Ruud van Asseldonk

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 3. A copy
// of the License is available in the root of the repository.

use std::env;
use std::path::{PathBuf};
use std::ffi::OsStr;
use std::sync::mpsc;
use std::thread;

use gio::prelude::*;
use gtk::prelude::*;

fn build_canvas() -> Option<gdk_pixbuf::Pixbuf> {
    let has_alpha = false;
    let bits_per_sample = 8;
    let width = 640;
    let height = 480;
    gdk_pixbuf::Pixbuf::new(
        gdk_pixbuf::Colorspace::Rgb,
        has_alpha,
        bits_per_sample,
        width,
        height,
    )
}

#[derive(Clone)]
struct View {
    window: gtk::ApplicationWindow,
    image: gtk::DrawingArea,
    sender: mpsc::SyncSender<ModelEvent>,
}

enum ViewEvent {
    SetTitle(String),
}

struct Model {
    file_name: Option<PathBuf>,
    sender: glib::SyncSender<ViewEvent>,
}

enum ModelEvent {
    OpenFile(PathBuf),
}

impl View {
    fn new(
        application: &gtk::Application,
        sender: mpsc::SyncSender<ModelEvent>,
    ) -> View {
        let window = gtk::ApplicationWindow::new(application);

        window.set_title("Spekje");
        window.set_border_width(10);
        window.set_position(gtk::WindowPosition::Center);
        window.set_default_size(640, 480);

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 10);
        window.add(&vbox);

        let image = gtk::DrawingArea::new();

        let expand = true;
        let fill = true;
        let padding = 0;
        vbox.pack_start(&image, expand, fill, padding);

        // Accept single strings for dropping. We could accept "text/uri-list" too,
        // but the application cannot handle more than one file at a time anyway.
        const DRAG_EVENT_INFO: u32 = 0;
        let drag_targets = [
            gtk::TargetEntry::new(
                "text/plain",
                gtk::TargetFlags::OTHER_APP,
                DRAG_EVENT_INFO,
            ),
        ];

        window.drag_dest_set(
            gtk::DestDefaults::ALL,
            &drag_targets[..],
            gdk::DragAction::COPY,
        );

        let view = View {
            window,
            image,
            sender,
        };

        let view_clone = view.clone();
        view.window.connect_drag_data_received(move |_self, _drag_context, _x, _y, data, info, _time| {
            assert_eq!(info, DRAG_EVENT_INFO);
            view_clone.on_drag_data_received(data);
        });

        let view_clone = view.clone();
        view.image.connect_draw(move |_self, ctx| {
            view_clone.on_draw(ctx);
            glib::signal::Inhibit(true)
        });

        view.window.show_all();

        view
    }

    fn on_drag_data_received(&self, data: &gtk::SelectionData) {
        // We registered only this target, so we should only be signalled for
        // this target.
        if let Some(uri) = data.get_text() {
            // When dropped, the uri is terminated by a newline. Strip it.
            let uri_stripped = uri.as_str().trim_end();
            if let Ok((fname, _)) = glib::filename_from_uri(uri_stripped) {
                println!("{:?}", fname);
                self.sender.send(ModelEvent::OpenFile(fname)).unwrap();
            }
        }
    }

    fn on_draw(&self, ctx: &cairo::Context) {
        ctx.set_source_rgb(0.0, 0.0, 0.0);
        ctx.set_line_width(1.0);
        ctx.move_to(10.0, 10.0);
        ctx.line_to(100.0, 100.0);
        ctx.stroke();
    }

    /// Handle one event. Should only be called on the main thread.
    fn handle_event(&self, event: ViewEvent) {
        match event {
            ViewEvent::SetTitle(fname) => {
                self.window.set_title(&format!("{} - Spekje", fname));
            }
        }
    }
}

impl Model {
    fn new(sender: glib::SyncSender<ViewEvent>) -> Model {
        Model {
            file_name: None,
            sender: sender,
        }
    }

    pub fn handle_event(&mut self, event: ModelEvent) {
        match event {
            ModelEvent::OpenFile(fname) => {
                match fname.file_name().and_then(OsStr::to_str) {
                    // I don't care to support non-utf8 filenames.
                    None => return eprintln!("Invalid file name to open."),
                    Some(fname_str) => {
                        let event = ViewEvent::SetTitle(fname_str.into());
                        self.sender.send(event).unwrap();
                    },
                }
                self.file_name = Some(fname);
                // TODO: Start actual file load.
            }
        }
    }
}

fn main() {
    let application = gtk::Application::new(
        Some("nl.ruuda.spekje"),
        Default::default(),
    ).unwrap();

    // When the application starts, run all of this on the main thread.
    application.connect_activate(move |app| {
        // Create two bounded one-way message queues. The one that sends
        // messages back to the view is a tailored glib channel, but it behaves
        // the same as the mpsc one.
        let (send_model, recv_model) = mpsc::sync_channel(10);
        let (send_view, recv_view) = glib::MainContext::sync_channel(glib::PRIORITY_DEFAULT_IDLE, 10);

        // On a background thread, construct the model, and run its event loop.
        thread::spawn(move || {
            let mut model = Model::new(send_view);
            loop {
                match recv_model.recv() {
                    Ok(event) => model.handle_event(event),
                    Err(_) => break,
                };
            }
        });

        // Back on the main thread, construct the view.
        let view = View::new(app, send_model);

        // Handle the view's events on this thread, the main thread.
        recv_view.attach(None, move |event| {
            view.handle_event(event);
            glib::source::Continue(true)
        });
    });

    // And run the UI event loop on the main thread.
    let args: Vec<_> = env::args().collect();
    application.run(&args);
}
