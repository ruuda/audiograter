// Spekje -- Spectrogram viewer
// Copyright 2019 Ruud van Asseldonk

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 3. A copy
// of the License is available in the root of the repository.

use std::ffi::OsStr;
use std::path::PathBuf;

#[macro_use] extern crate relm;
#[macro_use] extern crate relm_derive;

use gtk::prelude::*;

use relm::{Relm, Update, Widget};

struct Model {
    file_name: Option<PathBuf>,
}

#[derive(Msg)]
enum Msg {
    DropBad,
    DropFile(PathBuf),
    Quit,
}

#[derive(Clone)]
struct Widgets {
    image: gtk::Image,
    window: gtk::Window,
}

struct Win {
    model: Model,
    widgets: Widgets,
}

impl Update for Win {
    type Model = Model;
    type ModelParam = ();
    type Msg = Msg;

    fn model(_: &Relm<Self>, _: ()) -> Model {
        Model {
            file_name: None,
        }
    }

    fn update(&mut self, event: Msg) {
        match event {
            Msg::DropBad => eprintln!("The drop data did not contain a valid file name."),
            Msg::DropFile(fname) => {
                if let Some(name) = fname.file_name().and_then(OsStr::to_str) {
                    // For some reason, when we get a file name, it has a
                    // newline at the end. Trim it when setting the title.
                    self.widgets.window.set_title(&format!("{} - Spekje", name.trim_end()));
                }
                self.model.file_name = Some(fname);
            }
            Msg::Quit => {}
        }
    }
}

impl Widget for Win {
    type Root = gtk::Window;

    fn root(&self) -> Self::Root {
        self.widgets.window.clone()
    }

    fn view(relm: &Relm<Self>, model: Self::Model) -> Self {
        let window = gtk::Window::new(gtk::WindowType::Toplevel);

        window.set_title("Spekje");
        window.set_border_width(10);
        window.set_position(gtk::WindowPosition::Center);
        window.set_default_size(1280, 720);

        let vbox = gtk::Box::new(
            gtk::Orientation::Vertical,
            10,
        );
        window.add(&vbox);

        let canvas = build_canvas();
        let image = gtk::Image::new_from_pixbuf(canvas.as_ref());

        let expand = true;
        let fill = true;
        let padding = 0;
        vbox.pack_start(&image, expand, fill, padding);

        // Accept single strings for dropping. We could accept "text/uri-list" too,
        // but the application cannot handle more than one file at a time anyway.
        let drag_event_info = 0;
        let drag_targets = [
            gtk::TargetEntry::new(
                "text/plain",
                gtk::TargetFlags::OTHER_APP,
                drag_event_info,
            ),
        ];

        window.drag_dest_set(
            gtk::DestDefaults::ALL,
            &drag_targets[..],
            gdk::DragAction::COPY,
        );

        connect!(relm, window,
            connect_drag_data_received(_self, _drag_ctx, _x, _y, data, _info, _time),
            match data.get_text() {
                None => Msg::DropBad,
                Some(uri) => match glib::filename_from_uri(uri.as_str()) {
                    Ok((fname, _)) => Msg::DropFile(fname),
                    Err(_) => Msg::DropBad,
                }
            }
        );
        connect!(relm, window,
            connect_delete_event(_, _),
            return (Msg::Quit, Inhibit(false))
        );

        window.show_all();

        Win {
            model: model,
            widgets: Widgets {
                image: image,
                window: window,
            },
        }
    }

}

fn build_canvas() -> Option<gdk_pixbuf::Pixbuf> {
    let has_alpha = false;
    let bits_per_sample = 8;
    let width = 1280;
    let height = 720;
    gdk_pixbuf::Pixbuf::new(
        gdk_pixbuf::Colorspace::Rgb,
        has_alpha,
        bits_per_sample,
        width,
        height,
    )
}

fn main() {
    Win::run(()).unwrap();
}
