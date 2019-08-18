// Spekje -- Spectrogram viewer
// Copyright 2019 Ruud van Asseldonk

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 3. A copy
// of the License is available in the root of the repository.

use std::env;

use gdk_pixbuf as gdk;
use gio::prelude::*;
use glib;
use gtk::prelude::*;

fn build_canvas() -> Option<gdk::Pixbuf> {
    let has_alpha = false;
    let bits_per_sample = 8;
    let width = 1280;
    let height = 720;
    gdk::Pixbuf::new(
        gdk::Colorspace::Rgb,
        has_alpha,
        bits_per_sample,
        width,
        height,
    )
}

fn build_ui(application: &gtk::Application) {
    let window = gtk::ApplicationWindow::new(application);

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

    let expand = false;
    let fill = false;
    let padding = 0;
    vbox.pack_start(&image, expand, fill, padding);

    window.show_all();
}

fn main() {
    let application = gtk::Application::new(
        Some("nl.ruuda.spekje"),
        Default::default(),
    ).unwrap();

    application.connect_activate(move |app| {
        build_ui(app);
    });

    let args: Vec<_> = env::args().collect();
    application.run(&args);
}
